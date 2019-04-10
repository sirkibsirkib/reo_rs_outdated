use parking_lot::MutexGuard;
use std::cell::UnsafeCell;
use parking_lot::Mutex;
use crossbeam::{Sender, Receiver};
use crate::bitset::BitSet;
use std::marker::PhantomData;
use std::sync::Arc;
use std::mem;
type Id = usize;
type DatumPtr = *const ();
const NullDatumPtr: DatumPtr = std::ptr::null();

struct Guard<P: ProtoMemory> {
	must_be_ready: BitSet,
	data_constraint: Vec<fn(&P)->bool>,
	fire_actions: Vec<fn(&mut P)>,
}

struct Shared<P: ProtoMemory> {
	ready: Mutex<BitSet>,
	p_stack_ptrs: UnsafeCell<Vec<DatumPtr>>,
	senders: Vec<Sender<MetaMsg>>,
	_proto_state: PhantomData<*const P>,
}

// differs per 
pub trait ProtoMemory: Default {}

// only one implementor, but used so we can be generic over ProtoMemory
// pub trait GenShared {
// 	fn into(&self) -> &Shared
// }
pub trait GenShared<T> {
	fn get(&self, common: &PortCommon<T>) -> Result<T,()>;
	fn put(&self, common: &PortCommon<T>, datum: T) -> Result<(),T>;
}

#[derive(Debug)]
enum MetaMsg {
	// getters -> {putters, l-getters}
	CountMe{i_moved: bool},
	// proto -> putters
	WaitFor{getter_count: usize},
	// proto -> getters
	GetFromPutter{putter_id: Id, move_allowed: bool},
	GetFromMem{mem_id: Id, move_allowed: bool},
	BeLeader{follower_count: usize},
	BeFollower{leader: Id},
}

impl<T, P: ProtoMemory> GenShared<T> for Shared<P> {
	// here both T and P are both specific
	fn get(&self, common: &PortCommon<T>) -> Result<T,()> {
		use MetaMsg::*;
		// 1. wait at barrier
		self.yield_to_proto({
			let mut r = self.ready_lock();
			r.set(common.id);
			r
		});

		let datum = match common.get_msg() {
			GetFromPutter{putter_id, move_allowed} => {
				let p = self.get_putter_ptr(putter_id);
				let datum = if move_allowed {
					Self::move_from(p)
				} else {
					Self::clone_from(p)
				};
				let msg = CountMe{i_moved: move_allowed};
				self.senders[putter_id].send(msg).unwrap();
				datum
			}
			GetFromMem{mem_id, move_allowed} => {
				// ok need 2nd message to determine leader
				let p = self.get_mem_ptr(mem_id);
				let datum = if move_allowed {
					Self::move_from(p)
				} else {
					Self::clone_from(p)
				};
				match common.get_msg() {
					BeLeader{follower_count} => {
						let mut was_moved = move_allowed;
						for _ in 0..follower_count {
							match common.get_msg() {
								CountMe{i_moved: true} if was_moved => panic!("L DOUBLE MOVE!"),
								CountMe{i_moved: true} => was_moved=true,
								CountMe{..} => {},
								wrong_msg => panic!("WRONG GETTER LOOP MSG {:?}", wrong_msg),
							}
							self.leader_clearing_mem(mem_id); // yield control once again
						}
					},
					BeFollower{leader} => {
						let msg = CountMe{i_moved: move_allowed};
						self.senders[leader].send(msg).unwrap();
					},
					wrong_msg => panic!("B TYPE WRONG {:?}", wrong_msg),
				}
				datum
			}
			wrong_msg => panic!("A type wrong! {:?}", wrong_msg),
		};
		Ok(datum)
	}
	fn put(&self, common: &PortCommon<T>, datum: T) -> Result<(),T> {
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
			WaitFor{getter_count} => getter_count,
			wrong_msg => panic!("WRONG {:?}", wrong_msg),
		};
		let mut was_moved = false;
		for _ in 0..wait_count {
			match common.receiver.recv().unwrap() {
				CountMe{i_moved: false} => {},
				CountMe{i_moved: true} if was_moved => panic!("moved twice!"),
				CountMe{i_moved: true} => was_moved = true,
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
	fn clone_from<T>(_p: DatumPtr) -> T {
		unimplemented!()
	}
	fn get_mem_ptr(&self, _mem_id: Id) -> DatumPtr {
		// TODO can verify that it belongs to a mem
		unimplemented!()
	}
	fn get_putter_ptr(&self, putter_id: Id) -> DatumPtr {
		// TODO can verify that belongs to a putter
		unsafe {
			(*self.p_stack_ptrs.get())[putter_id]
		}
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
	}
	fn yield_to_proto(&self, mut ready: MutexGuard<BitSet>) {
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
	pub fn get(&mut self) -> Result<T,()> {
		self.common.shared.get(&self.common)
	}
}

pub struct Putter<T> {
	common: PortCommon<T>,
}
impl<T> Putter<T> {
	pub fn put(&mut self, datum: T) -> Result<(),T> {
		self.common.shared.put(&self.common, datum)
	}
}