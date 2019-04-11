
use std::mem;
use crate::bitset::BitSet;
use crossbeam::Receiver;
use parking_lot::Mutex;
use hashbrown::HashMap;
use crossbeam::Sender;
use std::sync::Arc;
use std::marker::PhantomData;

type Ptr = *const ();
type Id = usize;

#[derive(Debug)]
enum OutMessage {
	PutAwait{count: usize},
	GetNotify{ptr: Ptr, notify: Id},
	Notification{},
}

#[derive(Debug)]
enum InMessage {
	PutReq{id: Id, ptr: Ptr},
	GetReq{id: Id},
}

pub trait Proto {
	fn getter_ready(&mut self, id: Id);
	fn putter_ready(&mut self, id: Id, ptr: Ptr);
	fn advance_state(&mut self, w: &SharedCommunications);
} 

struct SharedCommunications {
	s_out: HashMap<Id, Sender<OutMessage>>,
	r_in: Receiver<InMessage>,
}
struct Shared<P: Proto> {
	comm: SharedCommunications,
	proto: Mutex<P>,
}

trait SharedTrait<T> {
	fn get(&self, pc: &PortCommon<T>) -> T;
	fn put(&self, pc: &PortCommon<T>, datum: T);
}

impl<P:Proto,T:TryClone> SharedTrait<T> for Shared<P> {
	fn get(&self, pc: &PortCommon<T>) -> T {
		let mut p = self.proto.lock();
		{
			p.getter_ready(pc.id);
			p.advance_state(&self.comm);
		}
		use OutMessage::*;
		match pc.r_out.recv().unwrap() {
			GetNotify{ptr, notify} => {
				let r: &T = unsafe{ mem::transmute(ptr) };
				let datum = r.try_clone();
				self.comm.out_message(notify, OutMessage::Notification{});
				datum
			},
			wrong => panic!("WRONG {:?}", wrong),
		}
	}
	fn put(&self, pc: &PortCommon<T>, datum: T) {
		let ptr = mem::transmute(&datum);
		{
			let mut p = self.proto.lock();
			p.putter_ready(pc.id, ptr);
			p.advance_state(&self.comm);
		}
		use OutMessage::*;
		match pc.r_out.recv().unwrap() {
			PutAwait{count} => {
				for _ in 0..count {
					match pc.r_out.recv().unwrap() {
						Notification{} => {},
						wrong => panic!("WRONG {:?}", wrong),
					}
					mem::forget(datum);
					// return
				}
			},
			wrong => panic!("WRONG {:?}", wrong),
		}
	}
}
impl SharedCommunications {
	fn out_message(&self, dest: Id, msg: OutMessage) {
		self.s_out.get(&dest).expect("bad communique").send(msg).expect("DEAD");
	}
}

struct PortCommon<T> {
	id: Id,
	phantom: PhantomData<*const T>,
	s_in: Sender<OutMessage>,
	r_out: Receiver<OutMessage>,
	shared: Arc<dyn SharedTrait<T>>,
}

struct Getter<T>(PortCommon<T>);
impl<T> Getter<T> {
	fn get(&self) -> T {
		self.0.shared.get(&self.0)
	}
}
struct Putter<T>(PortCommon<T>);
impl<T> Putter<T> {
	fn put(&self, datum: T) {
		self.0.shared.put(&self.0, datum)
	}
} 

struct Guard<P> {
	min_ready: BitSet,
	constraint: fn(&P) -> bool,
	action: fn(&mut P, &SharedCommunications),
}

pub trait TryClone: Sized {
	fn try_clone(&self) -> Self {
		panic!("Don't know how to clone this!")
	}
}

////////////// EXAMPLE concrete ///////////////

macro_rules! id_iter {
	($($id:expr),*) => {
        [{
            $id, 
        }].iter().cloned()
    };
}

struct SyncProto {
	getter_ids: BitSet,
	putter_ids: BitSet,
	// checking ^
	ready: BitSet,
	put: HashMap<Id, Ptr>,
	memory: (),
	guards: [Guard<Self>; 1],
}
impl Default for SyncProto {
	fn default() -> Self {
		Self {
			ready: BitSet::with_capacity(2),
			putter_ids: bitset!{0},
			getter_ids: bitset!{1},
			put: HashMap::default(),
			memory: (),
			guards: [
				Guard {
					min_ready: bitset!{0,1},
					constraint: |_x| true,
					action: |m, w| {
						let putter_id = 0;
						let ptr = *m.put.get(&putter_id).unwrap();
						let msg = OutMessage::GetNotify{ptr, notify: putter_id};
						for getter in id_iter![1] {
							w.out_message(getter, msg);
						}
					},
				}
			],
		}
	}
}
impl Proto for SyncProto {
	fn getter_ready(&mut self, id: Id) {
		assert!(self.getter_ids.test(id));
		self.ready.set(id);
	}
	fn putter_ready(&mut self, id: Id, ptr: Ptr) {
		assert!(self.putter_ids.test(id));
		if let Some(_) = self.put.insert(id, ptr) {
			panic!("PUT ptr where there was already one");
		}
	}
	fn advance_state(&mut self, w: &SharedCommunications) {
		'redo: loop {
			for g in self.guards.iter() {
				if !g.min_ready.is_subset(&self.ready) {
					continue;
				}
				if !(g.constraint)(self) {
					continue;
				}
				// FIRE!
				(g.action)(self, w);
				continue 'redo;
			}
			break; // no call to REDO
		}
	}
}

#[test]
pub fn test() {

}