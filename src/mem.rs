
////////// DEBUG DEBUG
#![allow(dead_code)]



use std::cell::UnsafeCell;
use parking_lot::Mutex;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use crate::proto::PortId;
use std::marker::PhantomData;
use crate::bitset::BitSet;
use slab::Slab;
use std_semaphore::Semaphore;
use std::sync::atomic::{AtomicBool, Ordering};
use anymap::AnyMap;

struct Ptr {
	ptr: UnsafeCell<*const ()>,
	owned: AtomicBool,
}
impl Ptr {
	fn get_clone<T: Clone>(&self) -> T {
		let t: &T = unsafe {
			std::mem::transmute(*self.ptr.get())
		};
		t.clone()
	}
	fn get_move_else_clone<T: Clone>(&self) -> T {
		self.get_move().unwrap_or_else(|| self.get_clone())
	}
	fn get_move<T>(&self) -> Option<T> {
		if self.try_dec() {
			let t: *const T = unsafe {
				std::mem::transmute(*self.ptr.get())
			};
			Some(unsafe { std::ptr::read(t) })
		} else {
			None
		}
	}
	fn try_dec(&self) -> bool {
		self.owned.swap(false, Ordering::SeqCst)
	}
	fn stack_put<T>(&self, datum_ref: &T) {
		unsafe {
			*self.ptr.get() = std::mem::transmute(datum_ref);
		}
		assert_eq!(false, self.owned.swap(true, Ordering::SeqCst));
	}
	fn new_dangling() -> Self {
		Self {
			ptr: UnsafeCell::new(std::ptr::null()),
			owned: false.into(),
		}
	}

	fn new_mem(at: *const u8, initialized: bool) -> Self {
		Ptr {
			ptr: UnsafeCell::new(unsafe { std::mem::transmute(at) }),
			owned: initialized.into(),
		}
	}
}

struct PutterSpace {
	dups: AtomicUsize,
	ptr: Ptr,
	sema: Semaphore,
}
impl PutterSpace {
	fn new(ptr: Ptr) -> Self {
		Self {dups: 1.into(), ptr, sema: Semaphore::new(0)}
	}
}

struct ProtoW {
	ready: BitSet,
	rules: Vec<Rule>,
	free_mem_slots: AnyMap,
}
impl ProtoW {
	fn enter(&mut self, r: &ProtoR, my_id: PortId) {
		self.ready.set(my_id);
		'outer: loop {
			'inner: for rule in self.rules.iter() {
				if self.ready.is_superset(&rule.guard) {
					// check guard
					(rule.actions)(r);
					if rule.guard.test(my_id) {
						// job done!
						break 'inner;
					} else {
						continue 'outer;
					}
				}
			}
			// none matched
			return
		}
	}
}

struct Rule {
	guard: BitSet,
	actions: fn(&ProtoR),
}

struct ProtoR {
	mem_data: Vec<u8>,
	spaces: Vec<PutterSpace>, // id range 0..(#MePu + #PoPu)
	num_mems: usize, // == #MePu
	dropboxes: Vec<MsgDropbox>, // id range #MePu..#PoPu
}

struct MsgDropbox {
	sema: Semaphore,
	msg: UnsafeCell<usize>,
}
impl MsgDropbox {
	fn new() -> Self {
		Self {
			sema: Semaphore::new(0),
			msg: 0.into(),
		}
	}
	fn recv(&self) -> usize {
		self.sema.acquire();
		unsafe { 
			*self.msg.get()
		}
	}
}

struct ProtoAll {
	r: ProtoR,
	w: Mutex<ProtoW>,
}

struct Getter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	id: PortId,
}
impl<T> Getter<T> {
	fn get(&mut self) -> T {
		let dropbox = &self.p.r.dropboxes[self.id - self.p.r.num_mems];

		// 1. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for a putter
		let putter_id = dropbox.recv();

		// 4. get datum
		let space = &self.p.r.spaces[putter_id];
		let datum: T = space.ptr.get_move().expect("MOVE FAILED");

		// 5. release putter
		space.sema.release();

		// 6. return datum
		datum
	}
}
struct Putter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	id: PortId,
}
impl<T> Putter<T> {
	fn put(&mut self, datum: T) -> Option<T> {
		let space = &self.p.r.spaces[self.id];
		let dropbox = &self.p.r.dropboxes[self.id - self.p.r.num_mems];

		// 1. make ready my datum
		space.ptr.stack_put(&datum);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_getters = dropbox.recv();

		// 4. wait for getters to finish with my value
		for _ in 0..num_getters {
			space.sema.acquire();
		}

		// 5. return datum if nobody moved it
		let owned = space.ptr.try_dec();
		if owned {
			Some(datum)
		} else {
			std::mem::forget(datum);
			None
		}
	}
}


trait Proto: Sized {
	type Interface: Sized;
	fn instantiate() -> Self::Interface;	
}
struct MyProto;
impl Proto for MyProto {

	type Interface = (Putter<u32>, Putter<u32>, Getter<u32>);
	fn instantiate() -> Self::Interface {

		// buf and memspaces
		let mem_infos = vec![
			// one line per BUFFER putter
			TypeMemInfo::new::<u32>(),
		];
		let (mem_data, ptrs) = build_buffer(mem_infos);
		let num_mems = ptrs.len();
		let mut spaces: Vec<_> = ptrs.into_iter().map(|ptr| PutterSpace::new(ptr)).collect();

		let port_putter_ids = 1..=2;
		let num_port_putters = port_putter_ids.clone().count();
		for _ in port_putter_ids {
			spaces.push(PutterSpace::new(Ptr::new_dangling()));
		};

		let port_getter_ids = 3..=3;
		let num_port_getters = port_getter_ids.count();
		let dropboxes = std::iter::repeat_with(|| MsgDropbox::new()).take(num_port_putters + num_port_getters).collect();

		let r = ProtoR { mem_data, spaces, num_mems, dropboxes };
		let rules = vec![
			Rule {
				guard: bitset!{0, 3},
				actions: |_p| {

				},
			},
			Rule {
				guard: bitset!{1, 2, 4},
				actions: |_p| {

				},
			},
		];
		let w = Mutex::new(ProtoW {
			rules,
			ready: BitSet::default(),
			free_mem_slots: {
				let mut m = AnyMap::new();
				m.insert::<Slab<u32>>(Slab::with_capacity(1));
				m
			},
		});
		let p = Arc::new(ProtoAll {w, r});
		(
			// 0 => m1 // putter
			Putter {p: p.clone(), id: 1, phantom: Default::default() },
			Putter {p: p.clone(), id: 2, phantom: Default::default() },
			Getter {p: p.clone(), id: 3, phantom: Default::default() },
			// 4 => m1^ // getter
		)
	}
}

#[derive(Debug)]
struct TypeMemInfo {
	size: usize,
	align: usize,
}
impl TypeMemInfo {
	pub fn new<T>() -> Self {
		Self {
			size: std::mem::size_of::<T>(),
			align: std::mem::align_of::<T>(),
		}
	}
}

fn build_buffer<I>(infos: I) -> (Vec<u8>, Vec<Ptr>) where I: IntoIterator<Item=TypeMemInfo> {
	let mut capacity = 0;
	let mut offsets = vec![];
	for info in infos.into_iter() {
		let rem = capacity % info.align;
		if rem > 0 {
			capacity += info.align - rem;
		}
		println!("@ {:?} for info {:?}", capacity, &info);
		offsets.push(capacity);
		capacity += info.size;
	}
	println!("CAP IS {:?}", capacity);

	let mut buf: Vec<u8> = Vec::with_capacity(capacity);
	unsafe {
		buf.set_len(capacity);
	}
	let ptrs = offsets.into_iter().map(|offset| unsafe {
		let p = buf.as_ptr().offset(offset as isize);
		Ptr::new_mem(p, false)
	}).collect();
	(buf, ptrs)
}

#[test]
fn test_my_proto() {
	let x = MyProto::instantiate();
}