use crate::bitset::BitSet;
use crossbeam::{Receiver, Sender};
use hashbrown::{HashMap, HashSet};
use parking_lot::Mutex;
use std::{marker::PhantomData, mem, sync::Arc};

pub type RuleId = u64;

/*
Represents a putters PUT value in no more than |usize| bytes (pointer size).
// If the value passed is <= size, then this Ptr value IS the value, padded
if necessary with uninitialized data.
Otherwise, its a pointer directly to the putter's stack which getters will
clone / move from as needed.
*/
#[derive(Debug, Copy, Clone)]
struct Ptr {
    raw: *const (),
}
impl Ptr {
    fn produce<T>(t: &T) -> Self {
        unsafe {
            if std::mem::size_of::<T>() <= std::mem::size_of::<Ptr>() {
                // DIRECT VALUE
                let mut ret: Ptr = std::mem::uninitialized();
                let dest: *mut T = std::mem::transmute(&mut ret);
                std::ptr::copy_nonoverlapping(t, dest, 1);
                // println!("DIRECT {:p}", ret.raw);
                ret
            } else {
                // INDIRECT VALUE
                std::mem::transmute(t)
            }
        }
    }
    fn consume_cloning<T: Clone>(self) -> T {
        unsafe {
            if std::mem::size_of::<T>() <= std::mem::size_of::<Ptr>() {
                // DIRECT VALUE
                let p: &T = std::mem::transmute(&self);
                p.clone()
            } else {
                // INDIRECT VALUE
                let p: &T = std::mem::transmute(self);
                p.clone()
            }
        }
    }
    fn consume_moving<T>(self) -> T {
        unsafe {
            if std::mem::size_of::<T>() <= std::mem::size_of::<Ptr>() {
                // DIRECT VALUE
                let src: *const T = std::mem::transmute(&self);
                let mut ret: T = std::mem::uninitialized();
                std::ptr::copy_nonoverlapping(src, &mut ret, 1);
                ret
            } else {
                // INDIRECT VALUE
                let src: *const T = std::mem::transmute(self);
                let mut dest: T = std::mem::uninitialized();
                std::ptr::copy_nonoverlapping(src, &mut dest, 1);
                dest
            }
        }
    }
}

// associated with putter OR getter OR mem-in OR mem-out
pub type Id = usize;

// protocol-to-port messaging with instructions
#[derive(Debug, Clone)]
enum OutMessage {
    PutAwait { count: usize },
    GetNotify { ptr: Ptr, notify: Id, move_allowed: bool },
    StateNotify {},
    GottenNotify { moved: bool },
}

// the trait that constrains the properties of specific protocol structures
pub trait Proto: Sized + 'static {
    type Interface;
    fn instantiate() -> Self::Interface;
    fn interface_ids() -> &'static [Id];
    fn build_guards() -> Vec<Guard<Self>>;
    fn state_predicate(&self, predicate: &StatePred) -> bool;
}

#[derive(Debug)]
pub struct StatePred {
    pred: Vec<crate::rbpa::Val>,
}

// this is NOT generic over P, but is passed to the protocol
#[derive(Debug, Default)]
pub struct ProtoCrGen {
    put: HashMap<Id, Ptr>,
}

// part of the Cr that is provided as argument to the Proto-trait implementor
#[derive(Debug)]
pub struct ProtoCr<P: Proto> {
    generic: ProtoCrGen,
    specific: P,
}

#[derive(Debug)]
pub struct StateWaiter {
    predicate: StatePred,
    notify_when: Id,
}

// writable component: locking needed
#[derive(Debug)]
pub struct ProtoCrAll<P: Proto> {
    ready: BitSet,
    ready_groups: HashSet<Id>,
    state_waiters: Vec<StateWaiter>,
    groups: HashMap<Id, HashSet<Id>>,
    inner: ProtoCr<P>,
}
impl<P: Proto> ProtoCrAll<P> {
    fn getter_ready(&mut self, id: Id) {
        self.ready.set(id);
    }
    fn putter_ready(&mut self, id: Id, ptr: Ptr) {
        self.ready.set(id);
        self.inner.generic.put.insert(id, ptr);
    }
    fn advance_state(&mut self, readable: &ProtoReadable<P>) {
        'redo: loop {
            // println!("READY: {:?}", &self.ready);
            for (_i, g) in readable.guards.iter().enumerate() {
                if self.ready.is_superset(&g.min_ready) {
                    if (g.constraint)(&self.inner) {
                        // println!("GUARD {} FIRING START", i);
                        (g.action)(&mut self.inner, readable);
                        // println!("GUARD {} FIRING END", i);
                        // println!("BEFORE DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
                        self.ready.difference_with(&g.min_ready);
                        // println!("AFTER  DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
                        continue 'redo; // re-check!
                    }
                }
            }
            if !self.state_waiters.is_empty() {
                let Self {
                    state_waiters,
                    inner,
                    ..
                } = self;
                state_waiters.retain(|waiter| {
                    if inner.specific.state_predicate(&waiter.predicate) {
                        readable
                            .s_out
                            .get(&waiter.notify_when)
                            .expect("WHAYT")
                            .send(OutMessage::StateNotify {})
                            .expect("ZPYP");
                        false
                    } else {
                        // keep waiting
                        true
                    }
                });
            }
            break; // no call to REDO
        }
        // println!("ADVANCE STATE OVER");
    }
}

// read-only component: no locking needed
struct ProtoReadable<P: Proto> {
    s_out: HashMap<Id, Sender<OutMessage>>,
    guards: Vec<Guard<P>>,
}
impl<P: Proto> ProtoReadable<P> {
    fn out_message(&self, dest: Id, msg: OutMessage) {
        self.s_out
            .get(&dest)
            .expect("bad proto_gen_stateunique")
            .send(msg)
            .expect("DEAD");
    }
}

pub trait Port<P: Proto> {
    fn get_common(&self) -> &PortCommon<P>;
}
impl<T, P: Proto> Port<P> for Putter<T, P> {
    fn get_common(&self) -> &PortCommon<P> {
        &self.0
    }
}
impl<T, P: Proto> Port<P> for Getter<T, P> {
    fn get_common(&self) -> &PortCommon<P> {
        &self.0
    }
}

pub enum GroupMakeError {
    DifferentProtoInstances,
    DuplicateIds,
    EmptyGroup,
    OverlapsWithExisting,
}

// represents a group of ports.
// creation performs registration with the
pub struct GroupCommunicator<P: Proto> {
    leader: Id,
    proto_common: Arc<ProtoCommon<P>>,
    r_out: Receiver<OutMessage>,
}

impl<P: Proto> GroupCommunicator<P> {
    pub fn register_port_group<'a, I>(predicate: StatePred, it: I) -> Result<Self, GroupMakeError>
    where
        P: Proto,
        I: Iterator<Item = &'a (dyn Port<P>)>,
    {
        use GroupMakeError::*;
        let mut group_ids: HashSet<Id> = HashSet::default();
        let mut comm = None;
        // 1. build the GroupCommunicator object
        for port in it {
            let comm = comm.get_or_insert_with(|| GroupCommunicator {
                leader: port.get_common().id,
                r_out: port.get_common().r_out.clone(),
                proto_common: port.get_common().proto_common.clone(),
            });
            if !comm
                .proto_common
                .share_instance(&port.get_common().proto_common)
            {
                return Err(DifferentProtoInstances);
            }
            if group_ids.insert(port.get_common().id) {
                return Err(DuplicateIds);
            }
        }
        let comm = match comm {
            Some(comm) => comm,
            None => return Err(EmptyGroup),
        };
        // TODO no IDs should be ready, as non-readiness
        // is invariant outside put() and get()
        // TODO add a check for protocol IDENTITY
        // 2. try register the group
        let mut cra = comm.proto_common.cra.lock();
        for existing_group in cra.groups.values() {
            for id in group_ids.iter() {
                // TODO

                if existing_group.contains(id) {
                    return Err(OverlapsWithExisting);
                }
            }
        }
        cra.groups.insert(comm.leader, group_ids);

        // 3. wait until the state predicate is satisifed
        let satisfied = cra.inner.specific.state_predicate(&predicate);
        if !satisfied {
            // wait
            let waiter = StateWaiter {
                notify_when: comm.leader,
                predicate,
            };
            cra.state_waiters.push(waiter);
            mem::drop(cra); // release lock
            match comm.r_out.recv().expect("KABLOEEY") {
                OutMessage::StateNotify {} => {}
                wrong_msg => panic!("Group waiter got {:?}", wrong_msg),
            }
        } else {
            mem::drop(cra); // release lock
        }
        Ok(comm)
    }
}
impl<P: Proto> Drop for GroupCommunicator<P> {
    fn drop(&mut self) {
        let mut cra = self.proto_common.cra.lock();
        assert!(cra.groups.remove(&self.leader).is_some());
        cra.ready_groups.remove(&self.leader);
    }
}

// the "shared" concrete protocol object
pub struct ProtoCommon<P: Proto> {
    readable: ProtoReadable<P>,
    cra: Mutex<ProtoCrAll<P>>,
}
impl<P: Proto> ProtoCommon<P> {
    fn share_instance(&self, other: &Self) -> bool {
        let left: *const ProtoCommon<P> = self;
        let right: *const ProtoCommon<P> = other;
        left == right
    }
    fn new(specific: P) -> (Self, HashMap<Id, Receiver<OutMessage>>) {
        let ids = <P as Proto>::interface_ids();
        let num_ids = ids.len();
        let mut s_out = HashMap::with_capacity(num_ids);
        let mut r_out = HashMap::with_capacity(num_ids);
        for &id in ids.iter() {
            let (s, r) = crossbeam::channel::bounded(num_ids);
            s_out.insert(id, s);
            r_out.insert(id, r);
        }
        let inner = ProtoCr {
            generic: ProtoCrGen::default(),
            specific,
        };
        let cra = ProtoCrAll {
            inner,
            state_waiters: vec![],
            ready: BitSet::default(),
            ready_groups: HashSet::default(),
            groups: Default::default(),
        };
        let guards = <P as Proto>::build_guards();
        let readable = ProtoReadable { s_out, guards };
        let common = ProtoCommon {
            readable,
            cra: Mutex::new(cra),
        };
        (common, r_out)
    }
    fn get<T>(&self, pc: &PortCommon<P>) -> T {
        // println!("{:?} entering...", pc.id);
        {
            let mut cra = self.cra.lock();
            // println!("{:?} got lock", pc.id);
            cra.getter_ready(pc.id);
            cra.advance_state(&self.readable);
            // println!("{:?} dropping lock", pc.id);
        }
        use OutMessage::*;
        match pc.r_out.recv().expect("LEL") {
            GetNotify { ptr, notify, move_allowed } => {
                // TODO handle if !move_allowed
                let datum = ptr.consume_moving();
                self.readable
                    .out_message(notify, OutMessage::GottenNotify { moved: move_allowed });
                datum
            }
            wrong => panic!("WRONG {:?}", wrong),
        }
    }

    fn get_signal(&self, pc: &PortCommon<P>) {
        // println!("{:?} entering...", pc.id);
        {
            let mut cra = self.cra.lock();
            // println!("{:?} got lock", pc.id);
            cra.getter_ready(pc.id);
            cra.advance_state(&self.readable);
            // println!("{:?} dropping lock", pc.id);
        }
        use OutMessage::*;
        match pc.r_out.recv().expect("LEL") {
            GetNotify { ptr: _, notify, move_allowed } => {
                self.readable
                    .out_message(notify, OutMessage::GottenNotify { moved: move_allowed });
            }
            wrong => panic!("WRONG {:?}", wrong),
        }
    }
    fn put<T>(&self, pc: &PortCommon<P>, datum: T) -> Option<T> {
        // println!("{:?} entering...", pc.id);
        let ptr = Ptr::produce(&datum);
        // println!("{:?} finished putting", pc.id);
        {
            let mut cra = self.cra.lock();
            // println!("{:?} got lock", pc.id);
            cra.putter_ready(pc.id, ptr);
            cra.advance_state(&self.readable);
            // println!("{:?} dropping lock", pc.id);
        }
        use OutMessage::*;
        match pc.r_out.recv().expect("HUAA") {
            PutAwait { count } => {
                let mut data_moved = false;
                for _ in 0..count {
                    match pc.r_out.recv().expect("HEE") {
                        GottenNotify { moved } => {
                            if moved {
                                if data_moved {
                                    panic!("Duplicate move!");
                                }
                                data_moved = true;
                            }
                        }
                        wrong => panic!("WRONG {:?}", wrong),
                    }
                }
                if data_moved {
                    mem::forget(datum);
                    None    
                } else {
                    Some(datum)
                }
            }
            wrong => panic!("WRONG {:?}", wrong),
        }
    }
}
// common to Putter and to Getter to minimize boilerplate
pub struct PortCommon<P: Proto> {
    id: Id,
    r_out: Receiver<OutMessage>,
    proto_common: Arc<ProtoCommon<P>>,
}
unsafe impl<P: Proto> Send for PortCommon<P> {}
unsafe impl<P: Proto> Sync for PortCommon<P> {}

// get and put invocations cross the dynamic dispatch barrier here
pub struct Getter<T, P: Proto>(PortCommon<P>, PhantomData<T>);
impl<T, P: Proto> Getter<T, P> {
    pub fn get(&self) -> T {
        self.0.proto_common.get(&self.0)
    }
    pub fn get_signal(&self) {
        self.0.proto_common.get_signal(&self.0)
    }
}
pub struct Putter<T, P: Proto>(PortCommon<P>, PhantomData<T>);
impl<T, P: Proto> Putter<T, P> {
    pub fn put(&self, datum: T) -> Option<T> {
        self.0.proto_common.put(&self.0, datum)
    }
}

pub struct Guard<P: Proto> {
    min_ready: BitSet,
    constraint: fn(&ProtoCr<P>) -> bool,
    action: fn(&mut ProtoCr<P>, &ProtoReadable<P>),
}

pub trait TryClone: Sized {
    fn try_clone(&self) -> Self {
        panic!("Don't know how to clone this!")
    }
}

////////////// EXAMPLE concrete ///////////////

// concrete proto. implements Proto trait
struct SyncProto<T> {
    data_type: PhantomData<T>,
}
impl<T: 'static> Proto for SyncProto<T> {
    fn state_predicate(&self, _predicate: &StatePred) -> bool {
        true
    }
    type Interface = (Putter<T, Self>, Getter<T, Self>);
    fn interface_ids() -> &'static [Id] {
        &[0, 1]
    }
    fn build_guards() -> Vec<Guard<Self>> {
        vec![Guard {
            min_ready: bitset! {0,1},
            constraint: |_cr| true,
            action: |cr, r| {
                let putter_id = 0;
                let ptr = *cr.generic.put.get(&putter_id).expect("HARK");
                let getter_id_iter = id_iter![1];
                let p_msg = OutMessage::PutAwait {
                    count: getter_id_iter.clone().count(),
                };
                r.out_message(putter_id, p_msg);
                for (i, getter_id) in getter_id_iter.enumerate() {
                    let first = i==0;
                    let g_msg = OutMessage::GetNotify {
                        ptr,
                        notify: putter_id,
                        move_allowed: first,
                    };
                    r.out_message(getter_id, g_msg);
                }
            },
        }]
    }
    fn instantiate() -> <Self as Proto>::Interface {
        let proto = Self {
            data_type: Default::default(),
        };
        let (proto_common, mut r_out) = ProtoCommon::new(proto);
        let proto_common = Arc::new(proto_common);
        let mut commons = <Self as Proto>::interface_ids()
            .iter()
            .map(|id| PortCommon {
                id: *id,
                r_out: r_out.remove(id).unwrap(),
                proto_common: proto_common.clone(),
            });
        finalize_ports!(commons => Putter, Getter)
    }
}

impl<T: Clone> TryClone for T {
    fn try_clone(&self) -> Self {
        self.clone()
    }
}

#[test]
pub fn prod_cons() {
    let (p, g) = SyncProto::<String>::instantiate();
    println!("INITIALIZED");
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..10 {
                p.put(format!("HEY {}", i));
            }
        });
        s.spawn(move |_| {
            for i in 0..10 {
                let i2 = g.get();
                println!("{:?}", (i, i2));
            }
        });
    })
    .expect("Fail");
}
