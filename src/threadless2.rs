use crossbeam::Sender;
use parking_lot::Mutex;
use crossbeam::Receiver;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use bit_set::BitSet;
use std::sync::Arc;
use hashbrown::HashMap;

struct Action {
	from: usize,
	to: Vec<usize>,
}
impl Action {
	fn new(from: usize, to: impl Iterator<Item=usize>) -> Self {
		Self {
			from,
			to: to.collect(),
		}
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum GuardState {
	NotReady,
	ReadyButConstFail,
	Ready,
}

struct GuardCmd {
	firing_set: BitSet,
	data_const: &'static dyn Fn() -> bool,
	actions: Vec<Action>,
}
impl GuardCmd {
	fn new(
		firing_set: BitSet,
		data_const: &'static dyn Fn() -> bool,
		actions: Vec<Action>
	) -> Self {
		Self { firing_set, data_const, actions }
	}
	fn satisfied(&self, ready: &BitSet) -> GuardState {
		if self.firing_set.is_superset(ready) {
			if (self.data_const)(/*TODO*/) {
				GuardState::Ready
			} else {
				GuardState::ReadyButConstFail
			}
		} else {
			GuardState::NotReady
		}
	}
}

#[derive(Copy, Clone)]
struct StackPtr(*mut ());
impl StackPtr {
	const NULL: Self = StackPtr(std::ptr::null_mut());
}
impl<T> From<*mut T> for StackPtr {
	fn from(p: *mut T) -> Self {
		StackPtr(unsafe { std::mem::transmute(p) })
	}
} 
impl<T> Into<*mut T> for StackPtr {
	fn into(self) -> *mut T {
		unsafe { std::mem::transmute(self.0) }
	}
} 

struct ProtoShared {
	ready: Mutex<BitSet>,
	guards: Vec<GuardCmd>,
	put_ptrs: UnsafeCell<Vec<StackPtr>>,
	meta_send: Vec<Sender<MetaMsg>>,
	// TODO id2guards
}
impl ProtoShared {
	fn arrive(&self, id: usize) {
		let mut ready = self.ready.lock();
		ready.insert(id);
		for g in self.guards.iter() {
			if ready.is_superset(&g.firing_set) {
				if (g.data_const)() {
					ready.difference_with(&g.firing_set);
					for a in g.actions.iter() {
						let num_getters = a.to.len();
						self.meta_send[a.from].send(MetaMsg::SetWaitSum(num_getters));
						for &t in a.to.iter().take(1) {
							self.meta_send[t].send(MetaMsg::MoveFrom(a.from));
						}
						for &t in a.to.iter().skip(1) {
							self.meta_send[t].send(MetaMsg::CloneFrom(a.from));
						}
					}
				}
			}
		}
	}
}

struct PortCommon {
	shared: Arc<ProtoShared>,
	id: usize,
	meta_recv: Receiver<MetaMsg>,
}

pub struct Getter<T> {
	port: PortCommon,
	_port_type: PhantomData<T>,
}
impl<T> Getter<T> {
	fn new(port: PortCommon) -> Self {
		Self {
			port,
			_port_type: PhantomData::default(),
		}
	}
	pub fn get(&mut self) -> Result<T,()> {
		self.port.shared.arrive(self.port.id);
		let (src_id, datum) = match self.port.meta_recv.recv().unwrap() {
			MetaMsg::MoveFrom(src_id) => (src_id, self.move_from(src_id)),
			MetaMsg::CloneFrom(src_id) => (src_id, self.clone_from(src_id)),
			wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
		};
		self.port.shared.meta_send[src_id].send(MetaMsg::DecWaitSum).unwrap();
		Ok(datum)
	}
	#[inline]
	fn move_from(&self, id: usize) -> T {
		let stack_ptr: StackPtr = unsafe {
			(*self.port.shared.put_ptrs.get())[id]
		};
		let p: *mut T = stack_ptr.into();
		unsafe {std::mem::replace(
			&mut *p,
			std::mem::uninitialized(),
		)}
	}

	#[inline]
	fn clone_from(&self, id: usize) -> T {
		// unsafe {unsafe {
		// 	let r: *mut T = (&*self.shared.data.get()).try_clone()
		// 	&*r
		// };}
		self.move_from(id)
	}
}

#[derive(Debug)]
enum MetaMsg {
	SetWaitSum(usize),
	MoveFrom(usize),
	CloneFrom(usize),
	DecWaitSum,
}

unsafe impl<T> Send for Putter<T> {}
unsafe impl<T> Sync for Putter<T> {}
unsafe impl<T> Send for Getter<T> {}
unsafe impl<T> Sync for Getter<T> {}
pub struct Putter<T> {
	port: PortCommon,
	_port_type: PhantomData<T>,
}
impl<T> Putter<T> {
	fn new(port: PortCommon) -> Self {
		Self {
			port,
			_port_type: PhantomData::default(),
		}
	}
	pub fn put(&mut self, mut datum: T) -> Result<(),T> {
		//// PUTTER HAS ACCESS
		let r: *mut T = &mut datum;
		unsafe { ( *self.port.shared.put_ptrs.get())[self.port.id] = r.into() };
		self.port.shared.arrive(self.port.id);
		let mut decs = 0;
		let mut wait_for = std::usize::MAX;
		while wait_for != decs {
			match self.port.meta_recv.recv().unwrap() {
				MetaMsg::SetWaitSum(x) => wait_for = x,
				MetaMsg::DecWaitSum => decs += 1,
				wrong_meta => panic!("putter wasn't expecting {:?}", wrong_meta),
			}
		}
		if wait_for > 0 {
			std::mem::forget(datum);
		}
		Ok(())
	}
}

macro_rules! usize_iter_literal {
	($array:expr) => {
		$array.iter().cloned()
	}
}

pub fn new_proto() -> (Putter<u32>, Getter<u32>) {
	const NUM_PORTS: usize = 2;
	const NUM_PUTTERS: usize = 1;
	fn guard_0_data_const() -> bool {
		true
	}
	let ready = Mutex::new(BitSet::new());
	let guards = vec![
		GuardCmd::new(
			bitset! {0,1},
			&guard_0_data_const,
			vec![
				Action::new(0, usize_iter_literal!([1])),
			],
		),
	];
	let put_ptrs = UnsafeCell::new(std::iter::repeat(StackPtr::NULL).take(NUM_PUTTERS).collect());
	let mut meta_send = Vec::with_capacity(NUM_PORTS);
	let mut meta_recv = Vec::with_capacity(NUM_PORTS);
	for _ in 0..NUM_PORTS {
		let (s,r) = crossbeam::channel::bounded(NUM_PORTS);
		meta_send.push(s);
		meta_recv.push(r);
	}
	let shared = Arc::new(ProtoShared {
		ready, guards, put_ptrs, meta_send,
	});
	(
		Putter::new(PortCommon {
			shared: shared.clone(),
			id: 0,
			meta_recv: meta_recv.remove(0), //remove vec head
		}),
		Getter::new(PortCommon {
			shared: shared.clone(),
			id: 1,
			meta_recv: meta_recv.remove(0), //remove vec head
		}),
	)
}
