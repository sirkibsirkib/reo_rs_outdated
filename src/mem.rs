
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
use std::marker::PhantomData;
use crate::bitset::BitSet;
use std_semaphore::Semaphore;
use std::sync::atomic::{AtomicBool, Ordering};


type PortId = usize;

struct Ptr {
	ptr: UnsafeCell<*mut u8>,
	owned: AtomicBool,
}
impl Ptr {
	fn get_clone<T>(&self, clone_fn: fn(&T)->T) -> T {
		let t: &T = unsafe {
			transmute(*self.ptr.get())
		};
		clone_fn(t)
	}
	fn get_move_else_clone<T>(&self, clone_fn: fn(&T)->T) -> T {
		self.get_move().unwrap_or_else(|| self.get_clone(clone_fn))
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

type DropFnPtr = fn(*mut u8);


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
		let f = r.mem_type_info.get(&self.type_id).expect("BAD TYPE for FN MAP").drop_fn;
		(f)(unsafe {
			*self.ptr.0.ptr.get()
		})
	}
	fn swap_mem_ptr(&self, other: &MePtr) {
		unsafe {
			std::mem::swap(
				&mut *self.ptr.0.ptr.get(),
				&mut *other.0.ptr.get(),
			)
		}

	}
	fn raw_move_port_ptr(&self, ptr: &PoPtr, mem_type_info: &HashMap<TypeId, MemTypeInfo>, tracking: &mut MemSlotTracking) {
		let bytes = mem_type_info.get(&self.type_id).expect("HEYYAY").bytes;
		unsafe {
			let mut raw_dest = *self.ptr.0.ptr.get();
			// 1. ensure the dest ptr has exactly ONE user
			let current_uses = tracking.mem_ptr_uses.get_mut(&raw_dest).expect("HHHF");
			assert!(*current_uses >= 1);
			if *current_uses > 1 {
				// 2. stop using this current ptr
				*current_uses -= 1;
				// 3. get a new (currently unused) pointer, mark it as having 1 user
				let fresh_raw_ptr = tracking.mem_ptr_unused.get_mut(&self.type_id).expect("BLAR").pop().expect("HG");
				tracking.mem_ptr_uses.insert(fresh_raw_ptr, 1);
				*self.ptr.0.ptr.get() = fresh_raw_ptr;
				raw_dest = fresh_raw_ptr;
			}
			let raw_src = *ptr.0.ptr.get();
			// 4. copy value from the stack to where the raw mem_ptr points
			std::ptr::copy(raw_src, raw_dest, bytes);
		}
	}
	fn dup_mem_ptr(&self, original: &MePtr, tracking: &mut MemSlotTracking) {
		let original_p: *mut u8 = unsafe {
			*original.0.ptr.get()
		};
		let self_p: *mut u8 = unsafe {
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
	mem_ptr_uses: HashMap<*mut u8, usize>,
	mem_ptr_unused: HashMap<TypeId, Vec<*mut u8>>,
}

struct ProtoActive {
	ready: BitSet,
	mem_tracking: MemSlotTracking,
}

struct Commitment {
	rule_id: usize,
	awaiting: usize,
}
struct ProtoW {
	rules: Vec<Rule>,
	active: ProtoActive,
	committed_rule: Option<Commitment>,
	ready_tentative: BitSet,
}
impl ProtoW {
	fn enter(&mut self, r: &ProtoR, my_id: PortId) {
		self.active.ready.set(my_id);
		if self.committed_rule.is_some() {
			// some rule is waiting for completion
			return;
		}
		let mut num_tenatives = 0;
		println!("enter with id={:?}. bitset now {:?}", my_id, &self.active.ready);
		'outer: loop {
			'inner: for (rule_id, rule) in self.rules.iter().enumerate() {
				if self.active.ready.is_superset(&rule.guard) {
					// committing to this rule!
					self.active.ready.difference_with(&rule.guard);
					for id in self.active.ready.iter_and(&self.ready_tentative) {
						num_tenatives += 1;
						match r.get_space(id) {
							SpaceRef::PoPu(po_pu) => po_pu.dropbox.send(rule_id),
							SpaceRef::PoGe(po_ge) => po_ge.dropbox.send(rule_id),
							_ => panic!("bad tentative!"),
						} 
					}
					// tenative ports! must wait for them to resolve
					if num_tenatives > 0 {
						self.committed_rule = Some(Commitment {
							rule_id,
							awaiting: num_tenatives,
						});
						println!("committed to rid {}", rule_id);
						break 'inner;
					}
					// no tenatives! proceed

					println!("... firing {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard);

					(rule.actions)(r, &mut self.active);
					println!("... FIRE COMPLETE {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard);
					if !rule.guard.test(my_id) {
						// job done!
						break 'inner;
					} else {
						continue 'outer;
					}
				}
			}
			// none matched
			println!("... exiting");
			return
		}
	}
	fn enter_committed(&mut self, r: &ProtoR, tent_it: PortId, expecting_rule: usize) {
		let comm = self.committed_rule.as_mut().expect("BUT IT MUST BE");
		assert_eq!(comm.rule_id, expecting_rule);
		self.ready_tentative.set_to(tent_it, false);
		comm.awaiting -= 1;
		if comm.awaiting > 0 {
			return; // someone else will finish up
		}
		let rule = &self.rules[comm.rule_id];
		self.committed_rule = None;
		(rule.actions)(r, &mut self.active);
	}
}

struct Rule {
	guard: BitSet,
	actions: fn(&ProtoR, &mut ProtoActive),
}

struct MemTypeInfo {
	drop_fn: DropFnPtr,
	bytes: usize,
}

struct ProtoR {
	mem_data: Vec<u8>,
	me_pu: Vec<MePuSpace>,  // id range 0..#MePu
	po_pu: Vec<PoPuSpace>, // id range #MePu..(#MePu+#PoPu)
 	po_ge: Vec<PoGeSpace>,   // id range (#MePu+#PoPu)..(#MePu+#PoPu+#PoGe)
 	// me_ge doesn't need a space
 	mem_type_info: HashMap<TypeId, MemTypeInfo>,
}
impl ProtoR {
	fn mem_id_gap(&self) -> usize {
		self.me_pu.len() + self.po_pu.len() + self.po_ge.len()
	}
	fn get_me_pu(&self, id: PortId) -> Option<&MePuSpace> {
		self.me_pu.get(id)
	}
	fn get_po_pu(&self, id: PortId) -> Option<&PoPuSpace> {
		self.po_pu.get(id - self.me_pu.len())
	}
	fn get_po_ge(&self, id: PortId) -> Option<&PoGeSpace> {
		self.po_ge.get(id - self.me_pu.len() - self.po_pu.len())
	}
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
impl ProtoAll {
	fn new(mem_infos: Vec<TypeMemInfo>, num_port_putters: usize, num_port_getters: usize, rules: Vec<Rule>) -> Self {
		let mem_id_gap = mem_infos.len() + num_port_putters + num_port_getters;

		let (mem_data, me_pu, mem_type_info, mem_ptr_uses, ready) = Self::build_buffer(mem_infos, mem_id_gap);
		let po_pu = (0..num_port_putters).map(|_| PoPuSpace::new()).collect();
		let po_ge = (0..num_port_getters).map(|_| PoGeSpace::new()).collect();

		let mem_tracking = MemSlotTracking {
			mem_ptr_uses,
			mem_ptr_unused: mem_type_info.keys().cloned().map(|type_id| (type_id, vec![])).collect(),
		};
		let r = ProtoR { mem_data, me_pu, po_pu, po_ge, mem_type_info };
		let w = Mutex::new(ProtoW {
			rules,
			active: ProtoActive {
				ready,
				mem_tracking,
			},
			committed_rule: None,
			ready_tentative: BitSet::default(),
		});
		ProtoAll {w, r}
	}

	fn build_buffer(infos: Vec<TypeMemInfo>, mem_id_gap: usize) -> (Vec<u8>, Vec<MePuSpace>, HashMap<TypeId, MemTypeInfo>, HashMap<*mut u8, usize>, BitSet) {
		let mut capacity = 0;
		let mut offsets_n_typeids = vec![];
		let mut mem_type_info = HashMap::default();
		let mut ready = BitSet::default();
		for (mem_id, info) in infos.into_iter().enumerate() {
			ready.set(mem_id + mem_id_gap);
			let rem = capacity % info.align.max(1);
			if rem > 0 {
				capacity += info.align - rem;
			}
			println!("@ {:?} for info {:?}", capacity, &info);
			offsets_n_typeids.push((capacity, info.type_id));
			mem_type_info.insert(info.type_id, MemTypeInfo {
				drop_fn: info.drop_fn,
				bytes: info.size,
			});
			capacity += info.size.max(1); // make pointers unique even with 0-byte data
		}
		mem_type_info.shrink_to_fit();
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
		(buf, ptrs, mem_type_info, mem_slot_uses, ready)
	}
}


unsafe impl<T: PortData> Send for Getter<T> {}
unsafe impl<T: PortData> Sync for Getter<T> {}
struct Getter<T: PortData> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	id: PortId,
}
impl<T: PortData> Getter<T> {
	fn get_signal(&mut self) {
		// 1. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 2. wait for a putter
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		let putter_id = po_ge.dropbox.recv();

		// 3. release putter
		match self.p.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu) => me_pu.getter_done(&self.p.r),
			SpaceRef::PoPu(po_pu) => po_pu.getters_sema.release(),
			_ => panic!("bad putter!"),
		}
	}
	fn get(&mut self) -> T {

		// 1. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 2. wait for a putter
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		let putter_id = po_ge.dropbox.recv();

		let datum = match self.p.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu) => {
				// 3. get datum
				let datum = me_pu.ptr.0.get_move_else_clone(T::clone_fn);

				// 4. perform cleanup if last getter
				me_pu.getter_done(&self.p.r);

				datum
			},
			SpaceRef::PoPu(po_pu) => {
				// 3. get datum
				let datum = po_pu.ptr.0.get_move_else_clone(T::clone_fn);

				// 4. release port putter
				po_pu.getters_sema.release();

				datum
			},
			_ => panic!("bad putter!"),
		};

		// 5. return datum
		datum
	}
}


unsafe impl<T> Send for Putter<T> {}
unsafe impl<T> Sync for Putter<T> {}
struct Putter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	id: PortId,
}
impl<T> Putter<T> {
	fn put(&mut self, datum: T) -> Option<T> {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum & set owned to true
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

// ACTIONS
fn mem_to_nowhere(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId) {
	println!("mem_to_nowhere");
	let was_owned = r.me_pu[me_pu].ptr.0.owned.swap(false, Ordering::SeqCst);
	assert!(was_owned);
	let mem_id_gap = r.mem_id_gap();
	w.ready.set(me_pu+mem_id_gap);
	r.me_pu[me_pu].do_drop(r);
}

fn mem_to_ports(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId, po_ge: &[PortId]) {
	println!("mem_to_ports");
	let new_getters_left = po_ge.len();
	if new_getters_left == 0 {
		return mem_to_nowhere(r, w, me_pu);
	}
	let mem_id_gap = r.mem_id_gap();
	w.ready.set(me_pu+mem_id_gap);
	let old_getters_left = r.me_pu[me_pu].getters_left.swap(new_getters_left, Ordering::SeqCst);
	assert_eq!(0, old_getters_left);
	for p in po_ge {
		r.get_po_ge(*p).expect("POGEO").dropbox.send(me_pu);
	}
}

fn mem_to_mem_and_ports(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
	println!("mem_to_mem_and_ports");
	// me_pu owned is TRUE
	// me_pu num_getters = 0
	let mem_id_gap = r.mem_id_gap();
	let mut me_ge_iter = me_ge.iter().cloned();
	let me_pu = match me_ge_iter.next() {
		None => return mem_to_ports(r, w, me_pu, po_ge),
		Some(first_me_ge) if first_me_ge == me_pu => {
			// STAY case
			// 1. pe_pu becomes full (again. was put down by `enter`)
			w.ready.set(me_pu); // putter UP
			me_pu
		},
		Some(first_me_ge) => {
			// SWAP case
			// 1. me_pu becomes EMPTY
			w.ready.set(me_pu+mem_id_gap); // getter UP

			// 2. first_me_ge becomes EMPTY also (it acts as putter, remember?)
			w.ready.set(first_me_ge+mem_id_gap); // putter UP
			first_me_ge
		},
	};
	let me_pu_space = &r.me_pu[me_pu];
	for g in me_ge_iter {
		// 3. getter becomes FULL
		let g_space = &r.me_pu[g];
		g_space.dup_mem_ptr(&me_pu_space.ptr, &mut w.mem_tracking);
		w.ready.set(g); // putter UP
		let was_owned = g_space.ptr.0.owned.swap(true, Ordering::SeqCst);
		assert!(!was_owned);
	}
	mem_to_ports(r, w, me_pu, po_ge);
}

fn port_to_mem_and_ports(r: &ProtoR, w: &mut ProtoActive, po_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
	println!("port_to_mem_and_ports");
	let mut me_ge_iter = me_ge.iter().cloned();
	let po_pu_space = r.get_po_pu(po_pu).expect("ECH");
	if let Some(first_me_ge) = me_ge_iter.next() {
		// 1+ memory getters. does not appear owned to port-getters
		let first_me_ge_space = r.get_me_pu(first_me_ge).expect("ECH2");
		// 1. move the value into the first mem's slot
		first_me_ge_space.raw_move_port_ptr(&po_pu_space.ptr, &r.mem_type_info, &mut w.mem_tracking);
		let was_owned = po_pu_space.ptr.0.owned.swap(false, Ordering::SeqCst);
		assert!(was_owned);

		// 2. fill the first memcell
		w.ready.set(first_me_ge); // putter UP
		let was_owned = first_me_ge_space.ptr.0.owned.swap(true, Ordering::SeqCst);
		assert!(!was_owned);

		for g in me_ge_iter {
			// 2. getter becomes FULL
			let g_space = &r.me_pu[g];
			g_space.dup_mem_ptr(&first_me_ge_space.ptr, &mut w.mem_tracking);
			w.ready.set(g); // putter UP
			let was_owned = g_space.ptr.0.owned.swap(true, Ordering::SeqCst);
			assert!(!was_owned);
		}
	} else {
		// no memory getters. stays owned
		let was_owned = po_pu_space.ptr.0.owned.swap(true, Ordering::SeqCst);
		assert!(was_owned);
	}
	// 3. allow port getters to get
	let num_port_getters = po_ge.len();
	for p in po_ge {
		r.get_po_ge(*p).expect("POGEO").dropbox.send(po_pu);
	}
	// 4. tell the port putter how many to expect
	po_pu_space.dropbox.send(num_port_getters);
}


trait PortData: Sized {
	fn clone_fn(_t: &Self) -> Self {
		panic!("Don't know how to clone this!")
	}
}
impl<T:Clone> PortData for T {
	fn clone_fn(t: &Self) -> Self {
		T::clone(t)
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
		let mem_infos = vec![
			TypeMemInfo::new::<u32>(),
		];
		let num_port_putters = 2;
		let num_port_getters = 1;
		let rules = vec![
			Rule {
				guard: bitset!{0, 3},
				actions: |_r, _w| {
					mem_to_ports(_r, _w, 0, &[3]);
				},
			},
			Rule {
				guard: bitset!{1, 2, 3, 4},
				actions: |_r, _w| {
					port_to_mem_and_ports(_r, _w, 1, &[], &[3]);
					port_to_mem_and_ports(_r, _w, 2, &[0], &[]);
				},
			},
		];
		let p = Arc::new(ProtoAll::new(mem_infos, num_port_putters, num_port_getters, rules));
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
	drop_fn: fn(*mut u8),
}
impl TypeMemInfo {
	pub fn new<T: 'static>() -> Self {
		let drop_fn: fn(*mut u8) = |ptr| unsafe {
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


#[test]
fn test_my_proto() {
	let (mut p1, mut p2, mut g3) = MyProto::instantiate();
	crossbeam::scope(|s| {
		s.spawn(move |_| {
			for i in 0..5 {
				p1.put(i);
			}
		});

		s.spawn(move |_| {
			for i in 0..5 {
				p2.put(i + 10);
			}
		});

		s.spawn(move |_| {
			for _ in 0..5 {
				println!("GOT {:?} | {:?}", g3.get(), g3.get_signal());
			}
		});
	}).expect("WENT OK");
}