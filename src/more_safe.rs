
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

#[derive(Debug, Copy, Clone)]
enum OutMessage {
	PutAwait{count: usize},
	GetNotify{ptr: Ptr, notify: Id},
	Notification{},
}

#[derive(Debug, Copy, Clone)]
enum InMessage {
	PutReq{id: Id, ptr: Ptr},
	GetReq{id: Id},
}



pub trait Proto {
	type Interface;
	type Memory;

	fn initialize() -> Self::Interface;
	fn destructure(&mut self) -> (&mut BitSet, &mut HashMap<Id, Ptr>, &mut Self::Memory, &[Guard<Self>]);
	fn get_ready_bitset(&mut self) -> &mut BitSet;
	fn getter_ready(&mut self, id: Id) {
		self.get_ready_bitset().set(id);
	}
	fn putter_ready(&mut self, id: Id, ptr: Ptr) {
		let (r, m, _, _) = self.destructure();
		r.set(id);
		m.insert(id, ptr);
	}
	fn advance_state(&mut self, w: &SharedCommunications) {
		let (ready, put, memory, guards) = self.destructure();
		'redo: loop {
			println!("READY: {:?}", ready);
			for (i,g) in guards.iter().enumerate() {
				if ready.is_superset(&g.min_ready) {
					if (g.constraint)(put, memory) {
						println!("GUARD {} FIRING START", i);
						(g.action)(put, memory, w);
						println!("GUARD {} FIRING END", i);
						println!("BEFORE DIFFERENCE {:?} and {:?}", ready, &g.min_ready);
						ready.difference_with(&g.min_ready);
						println!("AFTER  DIFFERENCE {:?} and {:?}", ready, &g.min_ready);
						continue 'redo; // re-check!
					}
				}
			}
			break; // no call to REDO
		}
		println!("ADVANCE STATE OVER");
	}
} 

pub struct SharedCommunications {
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
		println!("{:?} entering...", pc.id);
		{
			let mut p = self.proto.lock();
			println!("{:?} got lock", pc.id);
			p.getter_ready(pc.id);
			p.advance_state(&self.comm);
			println!("{:?} dropping lock", pc.id);
		}
		use OutMessage::*;
		match pc.r_out.recv().expect("LEL") {
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
		println!("{:?} entering...", pc.id);
		let ptr = unsafe { mem::transmute(&datum) };
		println!("{:?} finished putting", pc.id);
		{
			let mut p = self.proto.lock();
			println!("{:?} got lock", pc.id);
			p.putter_ready(pc.id, ptr);
			p.advance_state(&self.comm);
			println!("{:?} dropping lock", pc.id);
		}
		use OutMessage::*;
		match pc.r_out.recv().expect("HUAA") {
			PutAwait{count} => {
				for _ in 0..count {
					match pc.r_out.recv().expect("HEE") {
						Notification{} => {},
						wrong => panic!("WRONG {:?}", wrong),
					}
				}

				mem::forget(datum);
				// return
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

unsafe impl<T> Send for PortCommon<T> {}
unsafe impl<T> Sync for PortCommon<T> {}
struct PortCommon<T> {
	id: Id,
	phantom: PhantomData<*const T>,
	s_in: Sender<InMessage>,
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

pub struct Guard<P: Proto + ?Sized> {
	min_ready: BitSet,
	constraint: fn(&mut HashMap<Id, Ptr>, & P::Memory) -> bool,
	action: fn(&mut HashMap<Id, Ptr>, &mut P::Memory, &SharedCommunications),
}

pub trait TryClone: Sized {
	fn try_clone(&self) -> Self {
		panic!("Don't know how to clone this!")
	}
}

////////////// EXAMPLE concrete ///////////////

macro_rules! id_iter {
	($($id:expr),*) => {
        [$( $id, )*].iter().cloned()
    };
}

struct SyncProto {
	getter_ids: BitSet,
	putter_ids: BitSet,
	// checking ^
	ready: BitSet,
	put: HashMap<Id, Ptr>,
	memory: <Self as Proto>::Memory,
	guards: [Guard<Self>; 1],
}
impl Proto for SyncProto {
	type Interface = (Putter<u32>, Getter<u32>);
	type Memory = ();


	fn initialize() -> <Self as Proto>::Interface {
		let proto = Self {
			ready: BitSet::with_capacity(2),
			putter_ids: bitset!{0},
			getter_ids: bitset!{1},
			put: HashMap::default(),
			memory: (),
			guards: [
				Guard {
					min_ready: bitset!{0,1},
					constraint: |_x, _y| true,
					action: |p, _m, w| {
						let putter_id = 0;
						let ptr = *p.get(&putter_id).expect("HARK");
						let getter_id_iter = id_iter![1];
						let p_msg = OutMessage::PutAwait{count: getter_id_iter.clone().count()};
						w.out_message(putter_id, p_msg);
						let g_msg = OutMessage::GetNotify{ptr, notify: putter_id};
						for getter_id in getter_id_iter {
							w.out_message(getter_id, g_msg);
						}
					},
				}
			],
		};
		println!("{:?}", &proto.ready);
		println!("{:?}", &proto.putter_ids);
		println!("{:?}", &proto.getter_ids);

		let (s_in, r_in) = crossbeam::channel::bounded(0);
		let mut s_out = HashMap::default();
		let mut r_out = HashMap::<Id, Receiver<OutMessage>>::default();
		for id in id_iter![0,1] {
			let (s, r) = crossbeam::channel::bounded(5);
			s_out.insert(id, s);
			r_out.insert(id, r);
		}

		let comm = SharedCommunications { s_out, r_in };
		let shared = Arc::new(Shared { comm, proto: Mutex::new(proto) });

		let c0 = {
			let id = 0;
			PortCommon {
				id,
				r_out: r_out.remove(&id).expect("oo"),
				s_in: s_in.clone(),
				shared: shared.clone(),
				phantom: PhantomData::default(),
			}
		};

		let c1 = {
			let id = 1;
			PortCommon {
				id,
				r_out: r_out.remove(&id).expect("www"),
				s_in: s_in.clone(),
				shared: shared.clone(),
				phantom: PhantomData::default(),
			}
		};
		(Putter(c0), Getter(c1))
	}

	fn destructure(&mut self) -> (&mut BitSet, &mut HashMap<Id, Ptr>, &mut Self::Memory, &[Guard<Self>]) {
		(&mut self.ready, &mut self.put, &mut self.memory, &self.guards)
	}
	fn get_ready_bitset(&mut self) -> &mut BitSet {
		&mut self.ready
	}
}

impl<T: Clone> TryClone for T {
	fn try_clone(&self) -> Self { self.clone() }
} 

#[test]
pub fn test() {
	let (p, g) = SyncProto::initialize();
	println!("INITIALIZED");
	crossbeam::scope(|s| {
		s.spawn(move |_| {
			for i in 0..1 {
				p.put(i);
			}
		});
		s.spawn(move |_| {
			for i in 0..1 {
				let i2 = g.get();
				println!("{:?}", (i, i2));
			}
		});
	}).expect("Fale");
}
