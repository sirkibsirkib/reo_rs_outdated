
////////// DEBUG DEBUG
#![allow(dead_code)]



use hashbrown::HashMap;
use std::any::TypeId;
use std::mem::ManuallyDrop;
use std::cell::UnsafeCell;
use parking_lot::Mutex;
use std::mem::transmute;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use crate::proto::PortId;
use std::marker::PhantomData;
use crate::bitset::BitSet;
use std_semaphore::Semaphore;
use std::sync::atomic::{AtomicBool, Ordering};

struct Ptr {
	ptr: UnsafeCell<*mut ()>,
	owned: AtomicBool,
}
impl Ptr {
	fn get_clone<T: Clone>(&self) -> T {
		let t: &T = unsafe {
			transmute(*self.ptr.get())
		};
		t.clone()
	}
	fn get_move_else_clone<T: Clone>(&self) -> T {
		self.get_move().unwrap_or_else(|| self.get_clone())
	}
	fn get_move<T>(&self) -> Option<T> {
		if self.try_dec() {
			let t: *const T = unsafe {
				transmute(*self.ptr.get())
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
			ptr: UnsafeCell::new(unsafe { transmute(storage) }),
			owned: initialized.into(),
		})
	}
}
struct PoPtr(Ptr);
impl PoPtr {
	fn stack_put<T>(&self, datum_ref: &T) {
		unsafe {
			*self.0.ptr.get() = transmute(datum_ref);
		}
		assert_eq!(false, self.0.owned.swap(true, Ordering::SeqCst));
	}
}

type DropFnPtr = fn(*mut ());


struct MePuSpace {
	ptr: MePtr,
	getters_left: AtomicUsize, // counts down. every getter decrements by 1. last getter must clean up
	type_id: TypeId,
}
impl MePuSpace {
	fn new(ptr: MePtr, type_id: TypeId) -> Self {
		Self {
			ptr,
			getters_left: 0.into(),
			type_id,
		}
	}
	fn getter_done(&self, r: &ProtoR) {
		let left = self.getters_left.fetch_sub(1, Ordering::SeqCst);
		assert!(left >= 1);
		let owned = self.ptr.0.owned.swap(false, Ordering::SeqCst);
		if owned && left == 1 {
			// I was the last. perform cleanup
			self.do_drop(r)
		}
	}
	fn do_drop(&self, r: &ProtoR) {
		let f = r.drop_fns.get(&self.type_id).expect("BAD TYPE for FN MAP");
		(f)(unsafe {
			*self.ptr.0.ptr.get()
		})
	}
	fn swap_mem_ptr(&mut self, other: &MePtr) {
		unsafe {
			std::mem::swap(
				*self.ptr.0.ptr.get(),
				*other.0.ptr.get(),
			)
		}

	}
	fn dup_mem_ptr(&mut self, original: &MePtr, tracking: &mut MemSlotTracking) {
		let original_p: *mut () = unsafe {
			*original.0.ptr.get()
		};
		let self_p: *mut () = unsafe {
			*self.ptr.0.ptr.get()
		};
		if self_p == original_p {
			return; // I've already go the same ptr as them
		}
		let o = tracking.mem_ptr_uses.get_mut(&original_p).expect("BAD OWNER");
		assert!(*o > 0);
		*o += 1;
		let a = tracking.mem_ptr_uses.get_mut(&self_p).expect("BAD ADOPTER");
		assert!(*a > 0);
		*a -= 1;
		if *a == 0 {
			tracking.mem_ptr_uses.remove(&self_p);
			tracking.mem_ptr_unused.get_mut(&self.type_id).expect("BAD").push(self_p);
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

struct MemSlotTracking {
	mem_ptr_uses: HashMap<*mut (), usize>,
	mem_ptr_unused: HashMap<TypeId, Vec<*mut ()>>,
}
struct ProtoW {
	ready: BitSet,
	rules: Vec<Rule>,
	mem_slot_tracking: MemSlotTracking,
}
impl ProtoW {
	fn enter(&mut self, r: &ProtoR, my_id: PortId) {
		self.ready.set(my_id);
		'outer: loop {
			'inner: for rule in self.rules.iter() {
				if self.ready.is_superset(&rule.guard) {
					// check guard
					(rule.actions)(r, &mut self.mem_slot_tracking);
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
	actions: fn(&ProtoR, &mut MemSlotTracking),
}

struct ProtoR {
	mem_data: Vec<u8>,
	me_pu: Vec<MePuSpace>,  // id range 0..#MePu
	po_pu: Vec<PoPuSpace>, // id range #MePu..(#MePu+#PoPu)
 	po_ge: Vec<PoGeSpace>,   // id range (#MePu+#PoPu)..(#MePu+#PoPu+#PoGe)
 	// me_ge doesn't need a space
 	drop_fns: HashMap<TypeId, DropFnPtr>,
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
	fn send(&self, msg: usize) {
		unsafe {
			*self.msg.get() = msg;
		}
		self.sema.release();
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
				me_pu.getter_done(&self.p.r);

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


/*
who cleans up?




*/

// ACTIONS
fn mem_to_nowhere(r: &ProtoR, me_pu: PortId) {
	let was_owned = r.me_pu[me_pu].ptr.0.owned.swap(false, Ordering::SeqCst);
	assert!(was_owned);
	r.me_pu[me_pu].do_drop(r);
}

fn mem_to_ports(r: &ProtoR, me_pu: PortId, po_ge: &[PortId]) {
	let new_getters_left = po_ge.len();
	if new_getters_left == 0 {
		return mem_to_nowhere(r, me_pu);
	}
	let old_getters_left = r.me_pu[me_pu].getters_left.swap(new_getters_left, Ordering::SeqCst);
	assert_eq!(0, old_getters_left);
	for p in po_ge {
		r.po_ge[*p].dropbox.send(me_pu);
	}
}

fn mem_to_mem_and_ports(r: &ProtoR, me_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
	// me_pu owned is TRUE
	// me_pu num_getters = 0
	let mut me_ge_iter = me_ge.iter().cloned();
	if let Some(first_me_ge) = me_ge_iter.next() {
		// 1. swap with first mem. this mem becomes the putter
		let real_putter = r.me_pu[me_pu].swap_mem_ptr(&r.me_pu[first_me_ge].ptr);
		// r.read

		for g in me_ge_iter {
			r.me_pu[g].dup_mem_ptr()
		}
	} else {
		return mem_to_ports(r, me_pu, po_ge);
	}
}


macro_rules! action {

	// mem to nowhere
	($r:expr, $w:expr; $me_pu:tt => | ) => {{
		// invariant: num_getters==0, owned=true
		// must change: {owned => false}
		let was_owned = $r.me_pu[$me_pu].ptr.0.owned.swap(false, Ordering::SeqCst);
		assert!(was_owned);
		$r.me_pu[$me_pu].do_drop();
	}};

	// mem to ports
	($r:expr, $w:expr; $me_pu:tt => $( $po_ge:tt ),+ | ) => {{
		// invariant: num_getters==0, owned=true
		// must change: {num_getters => #po_ge}
		let new_num_getters = [$($po_ge),+].len();
		let old_num_getters = $r.me_pu[$me_pu].num_getters.swap(num_getters, Ordering::SeqCst);
		assert_eq!(0, old_num_getters);
		$(
			$r.po_ge[$po_ge].send($me_pu);
		),+
	}};

	// mem to mem and ports
	($r:expr, $w:expr; $me_pu:tt => $( $po_ge:tt ),* | $me_ge0:tt, $( $me_ge:tt ),*) => {{
		// invariant: num_getters==0, owned=true
		// must swap mems
		// must raise mem GET bits
		let space0 = &$r.me_pu[$me_pu];
		let space1 = &$r.me_pu[$me_ge];
		assert_eq!(space0.type_id, space1.type_id);

		let me_pu = if $me_pu == $me_ge {
			// keep
			$me_pu
		} else {
			// swap
			$me_ge
		};
		$(
			$me_ge
	 	),*
	}};

	// port to mem and ports
	($p:expr, $w:expr; $me_pu:tt => $( $po_ge:tt ),* | $( $me_ge:tt ),*) => {{

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

		let (mem_data, me_pu, drop_fns, mem_ptr_uses) = build_buffer(mem_infos);
		let po_pu = po_pu_rng.map(|_| PoPuSpace::new()).collect();
		let po_ge = po_ge_rng.map(|_| PoGeSpace::new()).collect();


		let mem_slot_tracking = MemSlotTracking {
			mem_ptr_uses,
			mem_ptr_unused: drop_fns.keys().cloned().map(|type_id| (type_id, vec![])).collect(),
		};
		let r = ProtoR { mem_data, me_pu, po_pu, po_ge, drop_fns };
		let rules = vec![
			Rule {
				guard: bitset!{0, 3},
				actions: |_r, _mst| {
				},
			},
			Rule {
				guard: bitset!{1, 2, 4},
				actions: |_p, _mst| {

				},
			},
		];
		let w = Mutex::new(ProtoW {
			rules,
			ready: BitSet::default(),
			mem_slot_tracking,
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
	type_id: TypeId,
	drop_fn: fn(*mut ()),
}
impl TypeMemInfo {
	pub fn new<T: 'static>() -> Self {
		let drop_fn: fn(*mut ()) = |ptr| unsafe {
			let ptr: &mut ManuallyDrop<T> = transmute(ptr);
			ManuallyDrop::drop(ptr);
		};
		Self {
			size: std::mem::size_of::<T>(),
			align: std::mem::align_of::<T>(),
			type_id: TypeId::of::<T>(),
			drop_fn,
		}
	}
}

fn build_buffer<I>(infos: I) -> (Vec<u8>, Vec<MePuSpace>, HashMap<TypeId, DropFnPtr>, HashMap<*mut (), usize>)
where I: IntoIterator<Item=TypeMemInfo> {
	let mut capacity = 0;
	let mut offsets_n_typeids = vec![];
	let mut drop_fns = HashMap::default();
	for info in infos.into_iter() {
		let rem = capacity % info.align.max(1);
		if rem > 0 {
			capacity += info.align - rem;
		}
		println!("@ {:?} for info {:?}", capacity, &info);
		offsets_n_typeids.push((capacity, info.type_id));
		drop_fns.insert(info.type_id, info.drop_fn);
		capacity += info.size.max(1); // make pointers unique even with 0-byte data
	}
	drop_fns.shrink_to_fit();
	println!("CAP IS {:?}", capacity);

	let mut buf: Vec<u8> = Vec::with_capacity(capacity);
	unsafe {
		buf.set_len(capacity);
	}
	let mut mem_slot_uses = HashMap::default();
	let ptrs = offsets_n_typeids.into_iter().map(|(offset, type_id)| unsafe {
		let p = buf.as_ptr().offset(offset as isize);
		*mem_slot_uses.entry(transmute(p)).or_insert(0) += 1;
		let me_ptr = MePtr::new(p, false);
		MePuSpace::new(me_ptr, type_id)
	}).collect();
	(buf, ptrs, drop_fns, mem_slot_uses)
}

#[test]
fn test_my_proto() {
	let _x = MyProto::instantiate();
}