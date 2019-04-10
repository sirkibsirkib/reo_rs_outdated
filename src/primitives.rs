use crate::bitset::BitSet;
use crossbeam::{Receiver, Sender};
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;
// use std::any::TypeId;

type Id = usize;


#[derive(Debug, Copy, Clone)]
pub struct DatumPtr(*const ());
impl DatumPtr {
    const NULL: Self = Self(std::ptr::null());
}
impl Default for DatumPtr {
    fn default() -> Self {
        Self::NULL
    }
}

pub struct Guard<P: ProtoMemory> {
    must_be_ready: BitSet,
    data_constraint: fn(&P) -> bool,
    fire_action: fn(&P),
}

struct Shared<P: ProtoMemory> {
    ready: Mutex<BitSet>,
    p_stack_ptrs: UnsafeCell<Vec<DatumPtr>>,
    senders: Vec<Sender<MetaMsg>>,
    _proto_state: PhantomData<*const P>,
    proto_mem: P,
}

// differs per
pub trait ProtoMemory: Default {
    type Memory;
    type Ptrs;
    fn get_guards(&self) -> &[Guard<Self>];
    fn putter_write(&self, id: Id, ptr: DatumPtr);
    fn write_mem(&self, id: Id, src: DatumPtr);
    fn empty_mem(&self, id: Id);
    fn mem_ptr(&self, id: Id) -> DatumPtr;
}

#[derive(Debug)]
struct MemCell<T> {
    datum: Option<T>,
    get_empty_bit: usize,
    get_full_bit: usize,
}
impl<T> MemCell<T> {
    fn new(init: Option<T>, get_empty_bit: usize, get_full_bit: usize) -> Self {
        Self {
            datum: init, get_empty_bit, get_full_bit,
        }
    }
}
struct EgProto {
    memory: <Self as ProtoMemory>::Memory,
    ptrs: <Self as ProtoMemory>::Ptrs,
    guards: [Guard<Self>; 2],
}
impl Default for EgProto {
    fn default() -> Self {
        Self {
            ptrs: Default::default(),
            memory: Default::default(),
            guards: [
                Guard {
                    must_be_ready: bitset!{0,1},
                    data_constraint: |_| true,
                    fire_action: |_| {

                    },
                },
                Guard {
                    must_be_ready: bitset!{1,1},
                    data_constraint: |_| true,
                    fire_action: |_| {

                    },
                },
            ]
        }
    }
}
#[derive(Debug, Default)]
struct EgProtoPtrs {
    datum_0: DatumPtr,
}
#[derive(Debug)]
struct EgProtoMemory {
    mem_0: MemCell<u32>,
}
impl Default for EgProtoMemory {
    fn default() -> Self {
        Self {
            mem_0: MemCell::new(None, 1, 2),
        }
    }
}
impl ProtoMemory for EgProto {
    type Memory = EgProtoMemory;
    type Ptrs = EgProtoMemory;
    fn get_guards(&self) -> &[Guard<Self>] {
        unimplemented!()
    }
    fn putter_write(&self, id: Id, ptr: DatumPtr) {
        unimplemented!()
    }

    fn write_mem(&self, id: Id, src: DatumPtr) {
        unimplemented!()
    }
    fn empty_mem(&self, id: Id) {
        unimplemented!()
    }
    fn mem_ptr(&self, id: Id) -> DatumPtr {
        unimplemented!()
    }
}

// only one implementor, but used so we can be generic over ProtoMemory
// pub trait GenShared {
// 	fn into(&self) -> &Shared
// }
pub trait GenShared<T> {
    fn get(&self, common: &PortCommon<T>) -> Result<T, ()>;
    fn put(&self, common: &PortCommon<T>, datum: T) -> Result<(), T>;
}

#[derive(Debug)]
enum MetaMsg {
    // getters -> {putters, l-getters}
    CountMe { i_moved: bool },
    // proto -> putters
    WaitFor { getter_count: usize },
    // proto -> getters
    GetFromPutter { putter_id: Id, move_allowed: bool },
    GetFromMem { mem_id: Id, move_allowed: bool },
    BeLeader { follower_count: usize },
    BeFollower { leader: Id },
}

impl<T, P: ProtoMemory> GenShared<T> for Shared<P> {
    // here both T and P are both specific
    fn get(&self, common: &PortCommon<T>) -> Result<T, ()> {
        use MetaMsg::*;
        // 1. wait at barrier
        self.yield_to_proto({
            let mut r = self.ready_lock();
            r.set(common.id);
            r
        });

        let datum = match common.get_msg() {
            GetFromPutter { putter_id, move_allowed } => {
                let p = self.get_putter_ptr(putter_id);
                let datum = if move_allowed {
                    Self::move_from(p)
                } else {
                    Self::clone_from(p)
                };
                let msg = CountMe {
                    i_moved: move_allowed,
                };
                self.senders[putter_id].send(msg).unwrap();
                datum
            }
            GetFromMem { mem_id, move_allowed } => {
                // ok need 2nd message to determine leader
                let p = self.get_mem_ptr(mem_id);
                let datum = if move_allowed {
                    Self::move_from(p)
                } else {
                    Self::clone_from(p)
                };
                match common.get_msg() {
                    BeLeader { follower_count } => {
                        let mut was_moved = move_allowed;
                        for _ in 0..follower_count {
                            match common.get_msg() {
                                CountMe { i_moved: true } if was_moved => panic!("L DOUBLE MOVE!"),
                                CountMe { i_moved: true } => was_moved = true,
                                CountMe { .. } => {}
                                wrong_msg => panic!("WRONG GETTER LOOP MSG {:?}", wrong_msg),
                            }
                            self.leader_clearing_mem(mem_id); // yield control once again
                        }
                    }
                    BeFollower { leader } => {
                        let msg = CountMe {
                            i_moved: move_allowed,
                        };
                        self.senders[leader].send(msg).unwrap();
                    }
                    wrong_msg => panic!("B TYPE WRONG {:?}", wrong_msg),
                }
                datum
            }
            wrong_msg => panic!("A type wrong! {:?}", wrong_msg),
        };
        Ok(datum)
    }
    fn put(&self, common: &PortCommon<T>, datum: T) -> Result<(), T> {
        use MetaMsg::*;
        // 1. update value
        unsafe {
            let p: DatumPtr = mem::transmute(&datum);
            self.set_putter_ptr(common.id, p);
        }
        // 2. wait at barrier
        self.yield_to_proto({
            let mut r = self.ready_lock();
            r.set(common.id);
            r
        });
        // 3. receive message. possibly return value
        let wait_count = match common.receiver.recv().unwrap() {
            WaitFor { getter_count } => getter_count,
            wrong_msg => panic!("WRONG {:?}", wrong_msg),
        };
        let mut was_moved = false;
        for _ in 0..wait_count {
            match common.receiver.recv().unwrap() {
                CountMe { i_moved: false } => {}
                CountMe { i_moved: true } if was_moved => panic!("moved twice!"),
                CountMe { i_moved: true } => was_moved = true,
                wrong_msg => panic!("WRONG {:?}", wrong_msg),
            };
        }
        if was_moved {
            mem::forget(datum)
        }
        Ok(())
    }
}

impl<P: ProtoMemory> Shared<P> {
    #[inline]
    fn move_from<T>(p: DatumPtr) -> T {
        unsafe {
            let p: *mut T = mem::transmute(p);
            mem::replace(&mut *p, mem::uninitialized())
        }
    }
    #[inline]
    fn clone_from<T>(p: DatumPtr) -> T {
        unsafe {
            let r: &T = mem::transmute(p);
            r.try_clone()
        }
    }
    fn get_mem_ptr(&self, _mem_id: Id) -> DatumPtr {
        // TODO can verify that it belongs to a mem
        unimplemented!()
    }
    fn get_putter_ptr(&self, putter_id: Id) -> DatumPtr {
        // TODO can verify that belongs to a putter
        unsafe { (*self.p_stack_ptrs.get())[putter_id] }
    }
    fn set_putter_ptr(&self, putter_id: Id, ptr: DatumPtr) {
        unsafe {
            (*self.p_stack_ptrs.get())[putter_id] = ptr;
        }
    }
    #[inline]
    fn ready_lock(&self) -> MutexGuard<BitSet> {
        self.ready.lock()
    }
    fn leader_clearing_mem(&self, mem_id: Id) {
        // USE CAREFULLY
        unimplemented!()
    }
    fn yield_to_proto(&self, mut ready: MutexGuard<BitSet>) {
        //1 
        for g in self.proto_mem.get_guards().iter() {
            if g.must_be_ready.is_subset(&ready) && (g.data_constraint)(&self.proto_mem) {
                // unset bits
                ready.difference_with(&g.must_be_ready);
                (g.fire_action)(&self.proto_mem)
            }
        }
        unimplemented!()
    }
}
///////////////////////////

pub struct PortCommon<T> {
    shared: Arc<Box<dyn GenShared<T>>>,
    id: Id,
    receiver: Receiver<MetaMsg>,
    _data_type: PhantomData<*const T>,
}
impl<T> PortCommon<T> {
    fn get_msg(&self) -> MetaMsg {
        self.receiver.recv().expect("PortCommon Receiver err!")
    }
}

pub struct Getter<T> {
    common: PortCommon<T>,
}
impl<T> Getter<T> {
    pub fn get(&mut self) -> Result<T, ()> {
        self.common.shared.get(&self.common)
    }
}

pub struct Putter<T> {
    common: PortCommon<T>,
}
impl<T> Putter<T> {
    pub fn put(&mut self, datum: T) -> Result<(), T> {
        self.common.shared.put(&self.common, datum)
    }
}


pub trait TryClone: Sized {
    fn try_clone(&self) -> Self {
        unimplemented!()
    }
}
impl<T: Sized> TryClone for T {}