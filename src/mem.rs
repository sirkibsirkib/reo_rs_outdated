
////////// DEBUG DEBUG
#![allow(dead_code)]



use std::mem::ManuallyDrop;
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
	ptr: UnsafeCell<*mut ()>,
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
}

struct MePtr(Ptr);
impl MePtr {
	fn new(storage: *const u8, initialized: bool) -> Self {
		Self(Ptr {
			ptr: UnsafeCell::new(unsafe { std::mem::transmute(storage) }),
			owned: initialized.into(),
		})
	}
}
struct PoPtr(Ptr);
impl PoPtr {
	fn stack_put<T>(&self, datum_ref: &T) {
		unsafe {
			*self.0.ptr.get() = std::mem::transmute(datum_ref);
		}
		assert_eq!(false, self.0.owned.swap(true, Ordering::SeqCst));
	}
}


struct MePuSpace {
	ptr: MePtr,
	getters_left: AtomicUsize, // counts down. every getter decrements by 1. last getter must clean up
	drop_fn: fn(*mut ()), // part of cleaning up. types erased at this stage
}
impl MePuSpace {
	fn new(ptr: MePtr, drop_fn: fn(*mut ())) -> Self {
		Self {
			ptr,
			getters_left: 0.into(),
			drop_fn,
		}
	}
	fn getter_done(&self) {
		let left = self.getters_left.fetch_sub(1, Ordering::SeqCst);
		assert!(left >= 1);
		if left == 1 {
			// I was the last. perform cleanup
			(self.drop_fn)(unsafe {
				*self.ptr.0.ptr.get()
			})
		}
	}
}

struct PoPuSpace {
	ptr: PoPtr,
	dropbox: MsgDropbox, // used only by this guy to recv messages
	getters_sema: Semaphore,
}
impl PoPuSpace {
	fn new() -> Self {
		Self {
			ptr: PoPtr(Ptr {
				ptr: UnsafeCell::new(std::ptr::null_mut()),
				owned: false.into(),
			}),
			dropbox: MsgDropbox::new(),
			getters_sema: Semaphore::new(0),
		}
	} 
}

struct PoGeSpace {
	dropbox: MsgDropbox, // used only by this guy to recv messages
}
impl PoGeSpace {
	fn new() -> Self {
		Self {
			dropbox: MsgDropbox::new(),
		}
	}
}

enum SpaceRef<'a> {
	MePu(&'a MePuSpace),
	PoPu(&'a PoPuSpace),
	PoGe(&'a PoGeSpace),
	None,
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
	me_pu: Vec<MePuSpace>,  // id range 0..#MePu
	po_pu: Vec<PoPuSpace>, // id range #MePu..(#MePu+#PoPu)
 	po_ge: Vec<PoGeSpace>,   // id range (#MePu+#PoPu)..(#MePu+#PoPu+#PoGe)
 	// me_ge doesn't need a space
}
impl ProtoR {
	fn get_space(&self, mut id: PortId) -> SpaceRef {
		if id < self.me_pu.len() {
			return SpaceRef::MePu(unsafe { self.me_pu.get_unchecked(id) })
		}
		id -= self.me_pu.len();
		if id < self.po_pu.len() {
			return SpaceRef::PoPu(unsafe { self.po_pu.get_unchecked(id) })
		}
		id -= self.po_pu.len();
		if id < self.po_ge.len() {
			return SpaceRef::PoGe(unsafe { self.po_ge.get_unchecked(id) })
		}
		SpaceRef::None
	}
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



// [MePu | PoPu | PoGe | MeGe]
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
		let po_ge = if let SpaceRef::PoGe(x) = self.p.r.get_space(self.id) {
			x
		} else {
			panic!("WRONG ID")
		};

		// 1. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for a putter
		let putter_id = po_ge.dropbox.recv();

		let datum = match self.p.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu) => {
				// 4. get datum
				let datum = me_pu.ptr.0.get_move().expect("MOVE FAILED");

				// 5. perform cleanup if last getter
				me_pu.getter_done();

				datum
			},
			SpaceRef::PoPu(po_pu) => {
				// 4. get datum
				let datum = po_pu.ptr.0.get_move().expect("MOVE FAILED");

				// 5. release port putter
				po_pu.getters_sema.release();

				datum
			},
			_ => panic!("bad putter!"),
		};

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
		let po_pu = if let SpaceRef::PoPu(x) = self.p.r.get_space(self.id) {
			x
		} else {
			panic!("WRONG ID")
		};

		// 1. make ready my datum
		po_pu.ptr.stack_put(&datum);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_getters = po_pu.dropbox.recv();

		// 4. wait for getters to finish with my value
		for _ in 0..num_getters {
			po_pu.getters_sema.acquire();
		}

		// 5. return datum if nobody moved it
		let owned = po_pu.ptr.0.try_dec();
		if owned {
			Some(datum)
		} else {
			std::mem::forget(datum);
			None
		}
	}
}



/* ACT A "mem drain"
1. Memory putter
2. zero mem getters
*/


/* ACT B "mem swap"
1. Memory putter x
2. 1+ mem getters
3. x not getting
*/

/* ACT C "mem stay"
1. Memory putter x
2. 1+ mem getters
3. x is getting
*/
/* ACT D "port put"
1. Port putter
*/

macro_rules! action {
	($p:expr; @MEM_DRAIN@ $me_pu:expr => $( $po_ge:expr ),*) => {{

	}};
}

trait Proto: Sized {
	type Interface: Sized;
	fn instantiate() -> Self::Interface;	
}
struct MyProto;
impl Proto for MyProto {

	type Interface = (Putter<u32>, Putter<u32>, Getter<u32>);
	fn instantiate() -> Self::Interface {
		let mem_infos = vec![
			TypeMemInfo::new::<u32>(),
		];
		let po_pu_rng = 1..=2;
		let po_ge_rng = 3..=3;

		let (mem_data, me_pu) = build_buffer(mem_infos);
		let po_pu = po_pu_rng.map(|_| PoPuSpace::new()).collect();
		let po_ge = po_ge_rng.map(|_| PoGeSpace::new()).collect();

		let r = ProtoR { mem_data, me_pu, po_pu, po_ge };
		let rules = vec![
			Rule {
				guard: bitset!{0, 3},
				actions: |_p| {
					action![_p; @MEM_DRAIN@ 0 => 3];
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
	drop_fn: fn(*mut ()),
}
impl TypeMemInfo {
	pub fn new<T>() -> Self {
		let drop_fn: fn(*mut ()) = |ptr| unsafe {
			let ptr: &mut ManuallyDrop<T> =
				std::mem::transmute(ptr);
			ManuallyDrop::drop(ptr);
		};
		Self {
			size: std::mem::size_of::<T>(),
			align: std::mem::align_of::<T>(),
			drop_fn,
		}
	}
}

fn build_buffer<I>(infos: I) -> (Vec<u8>, Vec<MePuSpace>) where I: IntoIterator<Item=TypeMemInfo> {
	let mut capacity = 0;
	let mut offsets_n_drops = vec![];
	for info in infos.into_iter() {
		let rem = capacity % info.align;
		if rem > 0 {
			capacity += info.align - rem;
		}
		println!("@ {:?} for info {:?}", capacity, &info);
		offsets_n_drops.push((capacity, info.drop_fn));
		capacity += info.size;
	}
	println!("CAP IS {:?}", capacity);

	let mut buf: Vec<u8> = Vec::with_capacity(capacity);
	unsafe {
		buf.set_len(capacity);
	}
	let ptrs = offsets_n_drops.into_iter().map(|(offset, drop_fn)| unsafe {
		let p = buf.as_ptr().offset(offset as isize);
		let me_ptr = MePtr::new(p, false);
		MePuSpace::new(me_ptr, drop_fn)
	}).collect();
	(buf, ptrs)
}

#[test]
fn test_my_proto() {
	let x = MyProto::instantiate();
}