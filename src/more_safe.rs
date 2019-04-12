use crossbeam::sync::ShardedLock;
use crate::bitset::BitSet;
use crossbeam::Receiver;
use crossbeam::Sender;
use hashbrown::HashMap;
use parking_lot::Mutex;
use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;

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
type Id = usize;

// protocol-to-port messaging with instructions
#[derive(Debug, Copy, Clone)]
enum OutMessage {
    PutAwait { count: usize },
    GetNotify { ptr: Ptr, notify: Id },
    Notification {},
}

pub trait Proto: Sized + 'static {
    type Interface;
    fn instantiate() -> Self::Interface;
    fn interface_ids() -> &'static [Id];
    fn build_guards() -> Vec<Guard<Self>>;
}

#[derive(Debug, Default)]
pub struct ProtoCrGen {
    put: HashMap<Id, Ptr>,
}

#[derive(Debug)]
pub struct ProtoCr<P: Proto> {
    generic: ProtoCrGen,
    specific: P,
}

#[derive(Debug)]
pub struct ProtoCrAll<P: Proto> {
    ready: BitSet,
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
            break; // no call to REDO
        }
        // println!("ADVANCE STATE OVER");
    }
}

/// above this line is &mut (inside the lock)

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

struct ProtoCommon<P: Proto> {
    readable: ProtoReadable<P>,
    cra: Mutex<ProtoCrAll<P>>,
}
impl<P: Proto> ProtoCommon<P> {
    pub fn new(specific: P) -> (Self, HashMap<Id, Receiver<OutMessage>>) {
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
            ready: BitSet::default(),
        };
        let guards = <P as Proto>::build_guards();
        let readable = ProtoReadable { s_out, guards };
        let common = ProtoCommon {
            readable,
            cra: Mutex::new(cra),
        };
        (common, r_out)
    }
}

trait ProtoCommonTrait<T> {
    fn get(&self, pc: &PortCommon<T>) -> T;
    fn put(&self, pc: &PortCommon<T>, datum: T);
}

impl<S,T> ProtoCommonTrait<T> for ShardedLock<S> where S: ProtoCommonTrait<T> {
    fn get(&self, pc: &PortCommon<T>) -> T {
        self.read().expect("POISONED").get(pc)
    }
    fn put(&self, pc: &PortCommon<T>, datum: T) {
        self.read().expect("POISONED").put(pc, datum)
    }
}

impl<P: Proto, T: TryClone> ProtoCommonTrait<T> for ProtoCommon<P> {
    fn get(&self, pc: &PortCommon<T>) -> T {
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
            GetNotify { ptr, notify } => {
                let datum = ptr.consume_moving();
                self.readable
                    .out_message(notify, OutMessage::Notification {});
                datum
            }
            wrong => panic!("WRONG {:?}", wrong),
        }
    }
    fn put(&self, pc: &PortCommon<T>, datum: T) {
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
                for _ in 0..count {
                    match pc.r_out.recv().expect("HEE") {
                        Notification {} => {}
                        wrong => panic!("WRONG {:?}", wrong),
                    }
                }
                mem::forget(datum);
                //return
            }
            wrong => panic!("WRONG {:?}", wrong),
        }
    }
}

unsafe impl<T> Send for PortCommon<T> {}
unsafe impl<T> Sync for PortCommon<T> {}
struct PortCommon<T> {
    id: Id,
    phantom: PhantomData<*const T>,
    r_out: Receiver<OutMessage>,
    proto_common: Arc<dyn ProtoCommonTrait<T>>,
}

struct Getter<T>(PortCommon<T>);
impl<T> Getter<T> {
    fn get(&self) -> T {
        self.0.proto_common.get(&self.0)
    }
}
struct Putter<T>(PortCommon<T>);
impl<T> Putter<T> {
    fn put(&self, datum: T) {
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

// struct ProtoMutator<P: Proto> {
//     l: ShardedLock<ProtoCommon<P>>,
// }
// impl<P: Proto> ProtoMutator<P> {
//     pub fn mutate<F,Q>(self, change_fn: F) -> ProtoMutator<Q>
//     where
//         F: Fn(ProtoCommon<P>) -> ProtoCommon<Q>,
//         Q: Proto {
//         let old_common: ProtoCommon<P> = self.l.into_inner().expect("POIS");
//         let new_common: ProtoCommon<Q> = change_fn(old_common);

//         ProtoMutator {
//             l: new_common,
//         }
//     }
// }

////////////// EXAMPLE concrete ///////////////

macro_rules! id_iter {
	($($id:expr),*) => {
        [$( $id, )*].iter().cloned()
    };
}

macro_rules! finalize_ports {
 	($commons:expr => $($struct:path),*) => {
 		(
 			$(
				$struct($commons.next().unwrap()),
			)*
 		)
 	}
}

struct SyncProto {}
impl Proto for SyncProto {
    type Interface = (Putter<String>, Getter<String>);
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
                let g_msg = OutMessage::GetNotify {
                    ptr,
                    notify: putter_id,
                };
                for getter_id in getter_id_iter {
                    r.out_message(getter_id, g_msg);
                }
            },
        }]
    }
    fn instantiate() -> <Self as Proto>::Interface {
        let proto = Self {};
        let (proto_common, mut r_out) = ProtoCommon::new(proto);
        let proto_common = Arc::new(proto_common);
        let mut commons = <Self as Proto>::interface_ids()
            .iter()
            .map(|id| PortCommon {
                id: *id,
                r_out: r_out.remove(id).unwrap(),
                proto_common: proto_common.clone(),
                phantom: PhantomData::default(),
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
pub fn test() {
    let (p, g) = SyncProto::instantiate();
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
