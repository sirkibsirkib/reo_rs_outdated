use crate::bitset::BitSet;
use crate::rbpa::Var;
use crate::tokens::Transition;
use crossbeam::{Receiver, Sender};
use hashbrown::{HashMap, HashSet};
use itertools::izip;
use parking_lot::Mutex;
use std::{marker::PhantomData, mem, sync::Arc};
use crate::helper::WithFirstTrait;

// associated with putter OR getter OR mem-in OR mem-out
pub type PortId = usize;
pub type RuleId = u64;

/*
Represents a putters PUT value in no more than |usize| bytes (pointer size).
// If the value passed is <= size, then this Ptr value IS the value, padded
if necessary with uninitialized data.
Otherwise, its a pointer directly to the putter's stack which getters will
clone / move from as needed.
*/
#[derive(Debug, Copy, Clone)]
pub struct Ptr {
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

// protocol-to-port messaging with instructions
#[derive(Debug, Clone)]
pub enum OutMessage {
    PutAwait {
        count: usize,
    },
    GetNotify {
        ptr: Ptr,
        notify: PortId,
        move_allowed: bool,
    },
    StateNotify {},
    GroupFireNotify {
        rule_id: RuleId,
    },
    GottenNotify {
        moved: bool,
    },
}

// the trait that constrains the properties of specific protocol structures
pub trait Proto: Sized + 'static {
    type Interface;
    fn new() -> Self::Interface;
    fn new_state() -> Self;
    fn interface_ids() -> &'static [PortId];
    fn build_guards() -> Vec<Guard<Self>>;
    fn test_state(&self, predicate: &StatePred) -> bool;
    fn new_in_map() -> HashMap<PortId, PortCommon<Self>> {
        let state = Self::new_state();
        let (proto_common, mut r_out) = ProtoCommon::new(state);
        let proto_common = Arc::new(proto_common);
        <Self as Proto>::interface_ids()
        .iter()
        .cloned()
        .map(|id| {
            let common = PortCommon {
                id,
                r_out: r_out.remove(&id).unwrap(),
                proto_common: proto_common.clone(),
            };
            (id, common)
        }).collect()
    }
}

#[derive(Debug, derive_new::new)]
pub struct StatePred {
    pred: Vec<Var>,
}

// this is NOT generic over P, but is passed to the protocol
#[derive(Debug, Default)]
pub struct ProtoCrGen {
    pub put: HashMap<PortId, Ptr>,
}

// part of the Cr that is provided as argument to the Proto-trait implementor
#[derive(Debug)]
pub struct ProtoCr<P: Proto> {
    pub generic: ProtoCrGen,
    pub specific: P,
}

#[derive(Debug)]
pub struct StateWaiter {
    predicate: StatePred,
    notify_when: PortId,
}

// writable component: locking needed
#[derive(Debug)]
pub struct ProtoCrAll<P: Proto> {
    buf: Vec<PortId>,
    committed: Option<RuleId>,
    ready: BitSet,
    ready_groups: HashSet<PortId>,
    state_waiters: Vec<StateWaiter>,
    groups: HashMap<PortId, BitSet>,
    inner: ProtoCr<P>,
}
impl<P: Proto> ProtoCrAll<P> {
    fn getter_ready(&mut self, id: PortId) {
        self.ready.set(id);
    }
    fn putter_ready(&mut self, id: PortId, ptr: Ptr) {
        self.ready.set(id);
        self.inner.generic.put.insert(id, ptr);
    }
    fn notify_waiters(&mut self, readable: &ProtoReadable<P>) {
        let Self {
            state_waiters,
            inner,
            ..
        } = self;
        state_waiters.retain(|waiter| {
            if inner.specific.test_state(&waiter.predicate) {
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
    fn advance_state(&mut self, readable: &ProtoReadable<P>) {
        'outer: loop {
            // println!("READY: {:?}", &self.ready);
            let range = match self.committed {
                Some(rule_id) => {
                    let r = rule_id as usize;
                    r..(r + 1)
                }
                None => 0..readable.guards.len(),
            };
            'inner: for (rule_id, r) in izip!(range.clone(), readable.guards[range].iter()) {
                if self.ready.is_superset(&r.min_ready) {
                    // rule is unconditionally ready to fire
                    if (r.constraint)(&self.inner) {
                        // println!("GUARD {} FIRING START", i);
                        (r.action)(&mut self.inner, readable);
                        // println!("GUARD {} FIRING END", i);
                        // println!("BEFORE DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
                        self.ready.difference_with(&r.min_ready);
                        // println!("AFTER  DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
                        continue 'outer; // re-check all rules!
                    }
                } else {
                    self.buf.clear();
                    // check if we can commit to this rule
                    for (chunk_id, min_chunk) in r.min_ready.iter_chunks().enumerate() {
                        let mut chunk = 0x0;
                        for (leader, group) in self.groups.iter() {
                            if !self.ready_groups.contains(leader) {
                                continue;
                            }
                            let contribution = min_chunk & group.get_chunk(chunk_id);
                            assert!(contribution & chunk == 0); // must not overlap!!
                            if contribution == 0 {
                                // this group contributes to satisfying this rule
                                chunk |= contribution;
                                self.buf.push(*leader);
                                if min_chunk & (!chunk) == 0 {
                                    // this guard would be satisfied!
                                    self.committed = Some(rule_id as u64);
                                    for leader in self.buf.drain(..) {
                                        self.ready_groups.remove(&leader);
                                        readable
                                            .s_out
                                            .get(&leader)
                                            .expect("HUUEEE")
                                            .send(OutMessage::GroupFireNotify {
                                                rule_id: rule_id as u64,
                                            })
                                            .expect("LOOOOUUU");
                                    }
                                    break 'inner; // cleanup and return
                                }
                            }
                        }
                    }
                    // nevermind. failed to fulfill this rule even with groups
                    self.buf.clear();
                }
            }
            // all rules checked with no progress. finalize
            if !self.state_waiters.is_empty() {
                self.notify_waiters(readable);
            }
            return;
        }
    }
}

// read-only component: no locking needed
pub struct ProtoReadable<P: Proto> {
    s_out: HashMap<PortId, Sender<OutMessage>>,
    guards: Vec<Guard<P>>,
}
impl<P: Proto> ProtoReadable<P> {
    fn out_message(&self, dest: PortId, msg: OutMessage) {
        self.s_out
            .get(&dest)
            .expect("bad proto_gen_stateunique")
            .send(msg)
            .expect("DEAD");
    }
    pub unsafe fn distribute_ptr<I: Iterator<Item=PortId> + Clone>(&self, ptr: Ptr, from: PortId, to: I) {
        let p_msg = OutMessage::PutAwait {
            count: to.clone().count(),
        };
        self.out_message(from, p_msg);
        for (is_first, getter_id) in to.with_first() {
            let g_msg = OutMessage::GetNotify {
                ptr,
                notify: from,
                move_allowed: is_first,
            };
            self.out_message(getter_id, g_msg);
        }
    }
}

pub trait Port<P: Proto> {
    fn get_common(&self) -> &PortCommon<P>;
}
impl<P: Proto, T: Port<P> + ?Sized> Port<P> for &T {
    fn get_common(&self) -> &PortCommon<P> {
        <T>::get_common(self)
    }
}
impl<T: TryClone, P: Proto> Port<P> for Putter<T, P> {
    fn get_common(&self) -> &PortCommon<P> {
        &self.0
    }
}
impl<T: TryClone, P: Proto> Port<P> for Getter<T, P> {
    fn get_common(&self) -> &PortCommon<P> {
        &self.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GroupMakeError {
    DifferentProtoInstances,
    DuplicatePortIds,
    EmptyGroup,
    OverlapsWithExisting,
}

// represents a group of ports.
// creation performs registration with the
pub struct PortGroup<P: Proto> {
    leader: PortId,
    proto_common: Arc<ProtoCommon<P>>,
    r_out: Receiver<OutMessage>,
}

impl<P: Proto> PortGroup<P> {
    pub fn ready_wait_determine<T: Transition<P>>(&self) -> T {
        let mut cra = self.proto_common.cra.lock();
        cra.ready_groups.insert(self.leader);
        cra.advance_state(&self.proto_common.readable);
        match self.r_out.recv().expect("muuu") {
            OutMessage::GroupFireNotify { rule_id } => T::from_rule_id(rule_id),
            wrong => panic!("GROUP WRONG! {:?}", wrong),
        }
    }
    pub fn new<'a, I, X: Port<P>>(predicate: StatePred, it: I) -> Result<Self, GroupMakeError>
    where
        P: Proto,
        I: IntoIterator<Item = X>,
    {
        use GroupMakeError::*;
        let mut group_ids: BitSet = BitSet::default();
        let mut comm = None;
        // 1. build the PortGroup object
        for port in it {
            let comm = comm.get_or_insert_with(|| PortGroup {
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
            if group_ids.set(port.get_common().id) {
                return Err(DuplicatePortIds);
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
            if existing_group.intersects_with(&group_ids) {
                return Err(OverlapsWithExisting);
            }
        }
        cra.groups.insert(comm.leader, group_ids);

        // 3. wait until the state predicate is satisifed
        let satisfied = cra.inner.specific.test_state(&predicate);
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
                wrong => panic!("Group waiter got {:?}", wrong),
            }
        } else {
            mem::drop(cra); // release lock
        }
        Ok(comm)
    }
}
impl<P: Proto> Drop for PortGroup<P> {
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
    pub fn new(specific: P) -> (Self, HashMap<PortId, Receiver<OutMessage>>) {
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
            buf: Vec::with_capacity(8),
            committed: None,
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
            GetNotify {
                ptr,
                notify,
                move_allowed,
            } => {
                // TODO handle if !move_allowed
                let datum = ptr.consume_moving();
                self.readable.out_message(
                    notify,
                    OutMessage::GottenNotify {
                        moved: move_allowed,
                    },
                );
                datum
            }
            wrong => panic!("GET WRONG {:?}", wrong),
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
            GetNotify {
                ptr: _,
                notify,
                move_allowed,
            } => {
                self.readable.out_message(
                    notify,
                    OutMessage::GottenNotify {
                        moved: move_allowed,
                    },
                );
            }
            wrong => panic!("GET SIG WRONG {:?}", wrong),
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
    pub id: PortId,
    pub r_out: Receiver<OutMessage>,
    pub proto_common: Arc<ProtoCommon<P>>,
}
unsafe impl<P: Proto> Send for PortCommon<P> {}
unsafe impl<P: Proto> Sync for PortCommon<P> {}

// get and put invocations cross the dynamic dispatch barrier here
pub struct Getter<T: TryClone, P: Proto>(PortCommon<P>, PhantomData<T>);
impl<T: TryClone, P: Proto> Getter<T, P> {
    pub fn get(&self) -> T {
        self.0.proto_common.get(&self.0)
    }
    pub fn get_signal(&self) {
        self.0.proto_common.get_signal(&self.0)
    }
    pub unsafe fn new(common: PortCommon<P>) -> Self {
        Self(common, Default::default())
    }
}
pub struct Putter<T: TryClone, P: Proto>(PortCommon<P>, PhantomData<T>);
impl<T: TryClone, P: Proto> Putter<T, P> {
    pub fn put(&self, datum: T) -> Option<T> {
        self.0.proto_common.put(&self.0, datum)
    }
    pub unsafe fn new(common: PortCommon<P>) -> Self {
        Self(common, Default::default())
    }
}

pub struct Guard<P: Proto> {
    pub min_ready: BitSet,
    pub constraint: fn(&ProtoCr<P>) -> bool,
    pub action: fn(&mut ProtoCr<P>, &ProtoReadable<P>),
}

pub trait TryClone: Sized {
    fn try_clone(&self) -> Self {
        panic!("Don't know how to clone this!")
    }
}
impl<T: Clone> TryClone for T {
    fn try_clone(&self) -> Self {
        self.clone()
    }
}

pub trait AtomicComponent {
    type P: Proto;
    type Interface;
    type SafeInterface;
    fn new<F, S>(interface: Self::Interface, f: F)
    where
        F: FnOnce(S, PortGroup<Self::P>, Self::Interface);
}

////////////// EXAMPLE concrete ///////////////

// concrete proto. implements Proto trait
pub(crate) struct SyncProto<T> {
    data_type: PhantomData<T>,
}
impl<T: 'static + TryClone> Proto for SyncProto<T> {
    fn test_state(&self, _predicate: &StatePred) -> bool {
        true
    }
    type Interface = (Putter<T, Self>, Getter<T, Self>);
    fn interface_ids() -> &'static [PortId] {
        &[0, 1]
    }
    fn build_guards() -> Vec<Guard<Self>> {
        vec![Guard {
            min_ready: bitset! {0,1},
            constraint: |_cr| true,
            action: data_move_action![0 => 1],
        }]
    }
    fn new_state() -> Self {
        Self {
            data_type: Default::default(),
        }
    }
    fn new() -> <Self as Proto>::Interface {
        finalize_ports!(
            Self::interface_ids().iter(),
            Self::new_in_map(),
            Putter, Getter
        )
    }
}

#[test]
pub fn prod_cons() {
    let (p, g) = SyncProto::<String>::new();
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
