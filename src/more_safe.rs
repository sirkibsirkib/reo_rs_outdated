
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

pub trait Proto: Sized + 'static {
	type Interface;
	// type Memory;

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
			println!("READY: {:?}", &self.ready);
			for (i,g) in readable.guards.iter().enumerate() {
				if self.ready.is_superset(&g.min_ready) {
					if (g.constraint)(&self.inner) {
						println!("GUARD {} FIRING START", i);
						(g.action)(&mut self.inner, readable);
						println!("GUARD {} FIRING END", i);
						println!("BEFORE DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
						self.ready.difference_with(&g.min_ready);
						println!("AFTER  DIFFERENCE {:?} and {:?}", &self.ready, &g.min_ready);
						continue 'redo; // re-check!
					}
				}
			}
			break; // no call to REDO
		}
		println!("ADVANCE STATE OVER");
	}
}

/// above this line is &mut (inside the lock)

struct ProtoReadable<P:Proto> {
	s_out: HashMap<Id, Sender<OutMessage>>,
	guards: Vec<Guard<P>>,
}
impl<P: Proto> ProtoReadable<P> {
	fn out_message(&self, dest: Id, msg: OutMessage) {
		self.s_out.get(&dest).expect("bad proto_gen_stateunique").send(msg).expect("DEAD");
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
		let inner = ProtoCr { generic: ProtoCrGen::default(), specific };
		let cra = ProtoCrAll { inner, ready: BitSet::default() };
		let guards = <P as Proto>::build_guards();
		let readable = ProtoReadable { s_out, guards };
		let common = ProtoCommon { readable, cra: Mutex::new(cra) };
		(common, r_out)
	}
}

trait ProtoCommonTrait<T> {
	fn get(&self, pc: &PortCommon<T>) -> T;
	fn put(&self, pc: &PortCommon<T>, datum: T);
}

impl<P:Proto,T:TryClone> ProtoCommonTrait<T> for ProtoCommon<P> {
	fn get(&self, pc: &PortCommon<T>) -> T {
		println!("{:?} entering...", pc.id);
		{
			let mut cra = self.cra.lock();
			println!("{:?} got lock", pc.id);
			cra.getter_ready(pc.id);
			cra.advance_state(&self.readable);
			println!("{:?} dropping lock", pc.id);
		}
		use OutMessage::*;
		match pc.r_out.recv().expect("LEL") {
			GetNotify{ptr, notify} => {
				let r: &T = unsafe{ mem::transmute(ptr) };
				let datum = r.try_clone();
				self.readable.out_message(notify, OutMessage::Notification{});
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
			let mut cra = self.cra.lock();
			println!("{:?} got lock", pc.id);
			cra.putter_ready(pc.id, ptr);
			cra.advance_state(&self.readable);
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
				//return
			},
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

////////////// EXAMPLE concrete ///////////////

macro_rules! id_iter {
	($($id:expr),*) => {
        [$( $id, )*].iter().cloned()
    };
}

struct SyncProto {

}

impl Proto for SyncProto {
	type Interface = (Putter<u32>, Getter<u32>);
	// type Memory = ();

	fn interface_ids() -> &'static [Id] {
		&[0, 1]
	}

	fn build_guards() -> Vec<Guard<Self>> {
		vec![
			Guard {
				min_ready: bitset!{0,1},
				constraint: |_cr| true,
				action: |cr, r| {
					let putter_id = 0;
					let ptr = *cr.generic.put.get(&putter_id).expect("HARK");
					let getter_id_iter = id_iter![1];
					let p_msg = OutMessage::PutAwait{count: getter_id_iter.clone().count()};
					r.out_message(putter_id, p_msg);
					let g_msg = OutMessage::GetNotify{ptr, notify: putter_id};
					for getter_id in getter_id_iter {
						r.out_message(getter_id, g_msg);
					}
				},
			},
		]
	}

	fn instantiate() -> <Self as Proto>::Interface {
		let proto = Self {
			
		};
		let (proto_common, mut r_out) = ProtoCommon::new(proto);
		let proto_common = Arc::new(proto_common);
		let c0 = {
			let id = 0;
			PortCommon {
				id,
				r_out: r_out.remove(&id).expect("oo"),
				proto_common: proto_common.clone(),
				phantom: PhantomData::default(),
			}
		};

		let c1 = {
			let id = 1;
			PortCommon {
				id,
				r_out: r_out.remove(&id).expect("www"),
				proto_common: proto_common.clone(),
				phantom: PhantomData::default(),
			}
		};
		(Putter(c0), Getter(c1))
	}
}

impl<T: Clone> TryClone for T {
	fn try_clone(&self) -> Self { self.clone() }
} 

#[test]
pub fn test() {
	let (p, g) = SyncProto::instantiate();
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
