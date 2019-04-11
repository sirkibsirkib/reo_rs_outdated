
use std::marker::PhantomData;
use crate::bitset::BitSet;
use std::mem;
use crossbeam::{Receiver, Sender};

#[derive(Debug, Copy, Clone)]
struct DatumPtr(*const ());
impl DatumPtr {
	const NULL: Self = DatumPtr(std::ptr::null()); 
}

#[derive(Debug)]
enum InwardMsg {
	Get{id: usize},
	Put{id: usize, ptr: DatumPtr},
	GetFinished{i_moved: bool, waiting_id: usize},
}

#[derive(Debug)]
enum OutwardMsg {
	GetComplete{ptr: DatumPtr, may_move: bool},
	PutComplete{was_moved: bool},
}

struct Guard<T> {
	min_ready: BitSet,
	constraint: fn(&T) -> bool,
	action: fn(&mut T),
}

struct SyncProto {
	ready: BitSet,
	guards: [Guard<Self>; 1],
	r_inward: Receiver<InwardMsg>,
	s_outward: [Sender<OutwardMsg>; 2],
}

struct ProtoBarrier {
	id: usize,
	was_moved: bool,
	waiting_for: usize,
}
impl ProtoBarrier {
	fn new(id: usize) -> Self {
		Self {id, waiting_for: 0, was_moved: false }
	}
	fn prep(&mut self, waiting_for: usize) {
		self.waiting_for = waiting_for;
		self.was_moved = false;
	}
}

impl SyncProto {
	pub fn new() -> (Self, Putter<u32>, Getter<u32>)  {
		let (s_inward, r_inward) = crossbeam::channel::bounded(0);
		let (x1, s1) = PortCommon::new(0,s_inward.clone());
		let (x2, s2) = PortCommon::new(1,s_inward.clone());
		let me = Self {
			ready: BitSet::with_capacity(2),
			guards: [
				Guard {
					min_ready: bitset!{0,1},
					constraint: |_| true,
					action: |_me: &mut Self| {},
				},
			],
			r_inward,
			s_outward: [s1,s2],
		};
		(me, Putter(x1), Getter(x2))
	}
	pub fn start(&mut self) {
		loop {
			use InwardMsg::*;
			use OutwardMsg::*;
			let mut ptrs = [DatumPtr::NULL; 2];
			let mut barriers = vec![ProtoBarrier::new(0)];
			match self.r_inward.recv().expect("shyat") {
				Get{id} => self.ready.set(id),
				Put{id, ptr} => {
					ptrs[id] = ptr;
					self.ready.set(id);
				},
				GetFinished{i_moved, waiting_id} => {
					let b = &mut barriers[waiting_id];
					b.waiting_for = b.waiting_for.checked_sub(1).expect("BAD WAIT");
					self.s_outward[waiting_id].send(PutComplete{was_moved: b.was_moved}).unwrap();
				},
			}
			'redo: loop {
				for g in self.guards.iter() {
					if g.min_ready.is_subset(&self.ready) {
						// tODO constraint
						(g.action)(self);
						continue 'redo;
					}
				}
				break; // no REDO called
			}
		}
	}
}

struct PortCommon<T> {
	id: usize,
	s_inward: Sender<InwardMsg>,
	r_outward: Receiver<OutwardMsg>,
	t: PhantomData<*const T>,
}
impl<T> PortCommon<T> {
	fn new(id: usize, s_inward: Sender<InwardMsg>) -> (Self, Sender<OutwardMsg>) {
		let (s_outward, r_outward) = crossbeam::channel::unbounded();
		let me = Self {
			id, s_inward, r_outward, t: Default::default(),
		};
		(me, s_outward)
	}
	fn send_recv(&mut self, send: InwardMsg) -> Result<OutwardMsg, ()> {
		self.s_inward.send(send).map_err(|_| {})?;
		self.r_outward.recv().map_err(|_| {})
	}
}


pub struct Getter<T>(PortCommon<T>);
impl<T> Getter<T> {
	pub fn get(&mut self) -> Result<T,()> {
		use InwardMsg::*;
		use OutwardMsg::*;
		let id = self.0.id;
		match self.0.send_recv(Get{id})? {
			GetComplete{ptr, may_move} => {
				let datum: T = unsafe {
					if may_move {
						let p: *const T = mem::transmute(ptr);
						p.read()
					} else {
						let p: &T = mem::transmute(ptr);
						unimplemented!() // TRY CLONE
					}
				};
				self.0.s_inward.send(GetFinished{i_moved: may_move}).unwrap();
				Ok(datum)
			}
			w => wrong_outward(&w),
		}
	}
}

fn wrong_outward(o: &OutwardMsg) -> ! {
	panic!("Wrong outward message! {:?}", o)
}

pub struct Putter<T>(PortCommon<T>);
impl<T> Putter<T> {
	pub fn put(&mut self, datum: T) -> Result<Option<T>,T> {
		use InwardMsg::*;
		use OutwardMsg::*;
		let id = self.0.id;
		let ptr = unsafe {
			mem::transmute(&datum)
		};
		match self.0.send_recv(Put{id, ptr}) {
			Err(_) => Err(datum),
			Ok(PutComplete{was_moved}) => {
				Ok(if was_moved{
					mem::forget(datum);
					None
				} else {
					Some(datum)
				})
			},
			Ok(w) => wrong_outward(&w),
		}
	}
}
