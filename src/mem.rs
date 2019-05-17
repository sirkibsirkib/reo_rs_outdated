
////////// DEBUG DEBUG
#![allow(dead_code)]

use hashbrown::HashMap;
use std::any::TypeId;
use std::mem::ManuallyDrop;
use std::cell::UnsafeCell;
use parking_lot::Mutex;
use std::mem::transmute;
use std::sync::Arc;
use std::marker::PhantomData;
use crate::bitset::BitSet;
use std_semaphore::Semaphore;
use std::sync::atomic::{AtomicUsize, Ordering};

type PortId = usize;
type UntypedPtr = *mut u8;
type DropFnPtr = fn(*mut u8);
type CloneFnPtr = fn(*mut u8, *mut u8);

const FLAG_YOUR_MOVE: usize = (1 << 63);
const FLAG_OTH_EXIST: usize = (1 << 62);

struct MePuSpace {
	ptr: UnsafeCell<*mut u8>, // acts as *mut T AND key to mem_slot_meta_map
	cloner_countdown: AtomicUsize,
	mover_sema: Semaphore,
	type_id: TypeId,
}
impl MePuSpace {
	fn new(ptr: *mut u8, type_id: TypeId) -> Self {
		Self {
			ptr: ptr.into(),
			cloner_countdown: 0.into(),
			mover_sema: Semaphore::new(0),
			type_id,
		}
	}
	fn make_empty(&self, r: &ProtoR, w: &mut ProtoActive, do_drop: bool) {
		let ptr = unsafe {
			*self.ptr.get()
		};
		let src_refs = w.mem_refs.get_mut(&ptr).expect("UNKNWN");
		let tid = &self.type_id;
		*src_refs -= 1;
		if *src_refs == 0 {
			// contents need to be dropped! ptr needs to be made free
			let info = r.mem_type_info.get(tid).expect("unknown type 2");
			w.mem_refs.remove(&ptr).expect("hhh");
			if do_drop {
				(info.drop_fn)(ptr);
			}
			w.free_mems.get_mut(tid).expect("??").push(ptr);
		}
	}
}

struct PoPuSpace {
	ptr: UnsafeCell<*mut u8>,
	cloner_countdown: AtomicUsize,
	mover_sema: Semaphore,
	done_dropbox: MsgDropbox,
}
impl PoPuSpace {
	fn new() -> Self {
		Self {
			ptr: UnsafeCell::new(std::ptr::null_mut()),
			cloner_countdown: 0.into(),
			mover_sema: Semaphore::new(0),
			done_dropbox: MsgDropbox::new(),
		}
	}
}

struct PoGeSpace {
	dropbox: MsgDropbox, // used only by this guy to recv messages
	move_intention: UnsafeCell<bool>,
}
impl PoGeSpace {
	fn new() -> Self {
		Self {
			dropbox: MsgDropbox::new(),
			move_intention: false.into(),
		}
	}
	fn get_move_intention(&self) -> bool {
		unsafe {
			*self.move_intention.get()
		}
	}
	fn set_move_intention(&self, move_intention: bool) {
		unsafe {
			*self.move_intention.get() = move_intention
		}
	}
	fn get_signal(&self, a: &ProtoAll) {
		let msg = self.dropbox.recv();
		let putter_id: PortId = msg & (!FLAG_YOUR_MOVE) & (!FLAG_OTH_EXIST);
		let i_move = (msg & FLAG_YOUR_MOVE) > 0;
		let conflict = msg & FLAG_OTH_EXIST > 0;

		// it's possible I am assigned "move duty" even if I didn't want it.
		// move then means "drop"
		assert!(i_move || conflict); 
		match a.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu_space) => {
				if i_move {
					if conflict {
						// 1. must wait for cloners to finish
						me_pu_space.mover_sema.acquire();
					}
					// 2. release putter
					me_pu_space.make_empty(&a.r, &mut a.w.lock().active, true);
				} else {
					let was = me_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						me_pu_space.mover_sema.release();
					}
				}
			},
			SpaceRef::PoPu(po_pu_space) => {
				if i_move {
					if conflict {
						// must wait for cloners to finish
						po_pu_space.mover_sema.acquire();
					}
					// 3. release putter. DO drop
					po_pu_space.done_dropbox.send(1);
				} else {
					let was = po_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						po_pu_space.mover_sema.release();
					}
				}
			},
			_ => panic!("bad putter!"),
		}
	}
	fn get<T: PortData>(&self, a: &ProtoAll) -> T {
		let msg = self.dropbox.recv();
		let putter_id: PortId = msg & (!FLAG_YOUR_MOVE) & (!FLAG_OTH_EXIST);
		let i_move = (msg & FLAG_YOUR_MOVE) > 0;
		let conflict = msg & FLAG_OTH_EXIST > 0;

		// I requested move, so if it was denied SOMEONE must move
		assert!(i_move || conflict); 
		match a.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu_space) => {
				let ptr: &T = unsafe {
					transmute(*me_pu_space.ptr.get())
				};
				if i_move {
					if conflict {
						// must wait for cloners to finish
						me_pu_space.mover_sema.acquire();
					}
					let datum = unsafe {
						std::ptr::read(ptr)
					};
					// 3. release putter. DON'T DROP
					me_pu_space.make_empty(&a.r, &mut a.w.lock().active, false);
					datum
				} else {
					let datum = T::clone_fn(ptr);
					let was = me_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						me_pu_space.mover_sema.release();
					}
					datum
				}
			},
			SpaceRef::PoPu(po_pu_space) => {
				let ptr: &T = unsafe {
					transmute(*po_pu_space.ptr.get())
				};
				if i_move {
					if conflict {
						// must wait for cloners to finish
						po_pu_space.mover_sema.acquire();
					}
					let datum = unsafe {
						std::ptr::read(ptr)
					};
					// 3. release putter
					po_pu_space.done_dropbox.send(1);
					datum
				} else {
					let datum = T::clone_fn(ptr);
					let was = po_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						po_pu_space.mover_sema.release();
					}
					datum
				}
			},
			_ => panic!("bad putter!"),
		}
	}
}

enum SpaceRef<'a> {
	MePu(&'a MePuSpace),
	PoPu(&'a PoPuSpace),
	PoGe(&'a PoGeSpace),
	None,
}

struct ProtoActive {
	ready: BitSet,
 	free_mems: HashMap<TypeId, Vec<*mut u8>>,
 	mem_refs: HashMap<*mut u8, usize>,
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
		// if self.committed_rule.is_some() {
		// 	// some rule is waiting for completion
		// 	return;
		// }
		// let mut num_tenatives = 0;
		println!("enter with id={:?}. bitset now {:?}", my_id, &self.active.ready);
		'outer: loop {
			'inner: for (rule_id, rule) in self.rules.iter().enumerate() {
				if self.active.ready.is_superset(&rule.guard) {
					// committing to this rule!
					self.active.ready.difference_with(&rule.guard);
					// TODO tentatives

					// for id in self.active.ready.iter_and(&self.ready_tentative) {
					// 	num_tenatives += 1;
					// 	match r.get_space(id) {
					// 		SpaceRef::PoPu(po_pu) => po_pu.dropbox.send(rule_id),
					// 		SpaceRef::PoGe(po_ge) => po_ge.dropbox.send(rule_id),
					// 		_ => panic!("bad tentative!"),
					// 	} 
					// }
					// // tenative ports! must wait for them to resolve
					// if num_tenatives > 0 {
					// 	self.committed_rule = Some(Commitment {
					// 		rule_id,
					// 		awaiting: num_tenatives,
					// 	});
					// 	println!("committed to rid {}", rule_id);
					// 	break 'inner;
					// }
					// // no tenatives! proceed

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
	clone_fn: CloneFnPtr,
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
	fn send_to_getter(&self, id: PortId, msg: usize) {
		self.get_po_ge(id).expect("NOPOGE").dropbox.send(msg)
	}
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
	#[inline]
	fn recv(&self) -> usize {
		self.sema.acquire();
		unsafe { 
			*self.msg.get()
		}
	}
	#[inline]
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
	fn new(mem_infos: Vec<BuildMemInfo>, num_port_putters: usize, num_port_getters: usize, rules: Vec<Rule>) -> Self {
		let mem_id_gap = mem_infos.len() + num_port_putters + num_port_getters;

		let (mem_data, me_pu, mem_type_info, mem_refs, ready) = Self::build_buffer(mem_infos, mem_id_gap);
		let po_pu = (0..num_port_putters).map(|_| PoPuSpace::new()).collect();
		let po_ge = (0..num_port_getters).map(|_| PoGeSpace::new()).collect();
		let free_mems = mem_type_info.keys().map(|&type_id| {
			(type_id, vec![])
		}).collect();
		let r = ProtoR { mem_data, me_pu, po_pu, po_ge, mem_type_info };
		let w = Mutex::new(ProtoW {
			rules,
			active: ProtoActive {
				ready,
				free_mems,
				mem_refs,
			},
			committed_rule: None,
			ready_tentative: BitSet::default(),
		});
		ProtoAll {w, r}
	}

	fn build_buffer(infos: Vec<BuildMemInfo>, mem_id_gap: usize) ->
	(
		Vec<u8>, // buffer
		Vec<MePuSpace>,
		HashMap<TypeId, MemTypeInfo>,
		HashMap<*mut u8, usize>,
		BitSet,
	) {
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
			mem_type_info.entry(info.type_id).or_insert(MemTypeInfo {
				drop_fn: info.drop_fn,
				clone_fn: info.clone_fn,
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
		let mut mem_refs = HashMap::default();
		let ptrs = offsets_n_typeids.into_iter().map(|(offset, type_id)| unsafe {
			let ptr: *mut u8 = buf.as_mut_ptr().offset(offset as isize);
			mem_refs.insert(ptr, 1);
			MePuSpace::new(ptr, type_id)
		}).collect();
		(buf, ptrs, mem_type_info, mem_refs, ready)
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
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_move_intention(false);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.get_signal(&self.p);
	}
	fn get(&mut self) -> T {
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_move_intention(true);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.get(&self.p)
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
		unsafe {
			*po_pu.ptr.get() = transmute(&datum);	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let msg = po_pu.done_dropbox.recv();
		match msg {
			0 => Some(datum),
			1 => {
				std::mem::forget(datum);
				None
			},
			_ => panic!("putter got a bad msg"),
		}
	}
	fn put_lossy(&mut self, datum: T) {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum & set owned to true
		unsafe {
			*po_pu.ptr.get() = transmute(&datum);	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let msg = po_pu.done_dropbox.recv();
		std::mem::forget(datum);
		assert!(msg == 0 || msg == 1); // sanity check
	}
}

// ACTIONS
fn mem_to_nowhere(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId) {
	let me_pu_space = r.get_me_pu(me_pu).expect("fewh");
	me_pu_space.make_empty(r, w, true);
}

fn mem_to_ports(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId, po_ge: &[PortId]) {
	let me_pu_space = r.get_me_pu(me_pu).expect("fewh");

	// 1. port getters have move-priority
	let port_mover_id = find_mover(po_ge, r);

	// 3. instruct port-getters. delegate clearing putters to them (unless 0 getters)
	match (port_mover_id, po_ge.len()) {
		(Some(m), 1) => {
			// ONLY mover case. mover will wake putter
			r.send_to_getter(m, me_pu | FLAG_YOUR_MOVE);
		},
		(Some(m), l) => {
			// mover AND cloners case. mover will wake putter
			me_pu_space.cloner_countdown.store(l-1, Ordering::SeqCst);
			for g in po_ge.iter().cloned() {
				let payload = if g==m {
					me_pu | FLAG_OTH_EXIST | FLAG_YOUR_MOVE
				} else {
					me_pu | FLAG_OTH_EXIST
				};
				r.send_to_getter(g, payload);
			}
		},
		(None, l) => {
			// no movers
			if l == 0 {
				// no cloners either
				me_pu_space.make_empty(r, w, true);
			} else {
				me_pu_space.cloner_countdown.store(l, Ordering::SeqCst);
				for g in po_ge.iter().cloned() {
					r.send_to_getter(g, me_pu);
				}
			}
		},
	}
}

fn mem_to_mem_and_ports(r: &ProtoR, w: &mut ProtoActive, me_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
	println!("mem_to_mem_and_ports");
	let me_pu_space = r.get_me_pu(me_pu).expect("fewh");
	let tid = &me_pu_space.type_id;
	let src = unsafe {
		*me_pu_space.ptr.get()
	};

	// 1. copy pointers to other memory cells
	// ASSUMES destinations have dangling pointers TODO checks
	for g in me_ge.iter().cloned() {
		let me_ge_space = r.get_me_pu(g).expect("gggg");
		assert_eq!(*tid, me_ge_space.type_id);
		unsafe {
			*me_ge_space.ptr.get() = src;
		}
	}
	// 2. increment memory pointer refs of me_pu
	let src_refs = w.mem_refs.get_mut(&src).expect("UNKNWN");
	*src_refs += me_ge.len();
	mem_to_ports(r, w, me_pu, po_ge);
}


fn find_mover(getters: &[PortId], r: &ProtoR) -> Option<PortId> {
	getters.iter().filter(|id| {
		let po_ge = r.get_po_ge(**id).expect("bad id");
		po_ge.get_move_intention()
	}).next().or(getters.get(0)).cloned()
}

fn port_to_mem_and_ports(r: &ProtoR, w: &mut ProtoActive, po_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
	println!("port_to_mem_and_ports");

	assert!(po_pu < FLAG_OTH_EXIST && po_pu < FLAG_YOUR_MOVE);
	let po_pu_space = r.get_po_pu(po_pu).expect("ECH");

	// 1. port getters have move-priority
	let port_mover_id = find_mover(po_ge, r);

	// 2. populate memory cells if necessary
	let mut me_ge_iter = me_ge.iter().cloned();
	if let Some(first_me_ge) = me_ge_iter.next() {
		let first_me_ge_space = r.get_me_pu(first_me_ge).expect("wfew");
		let tid = &first_me_ge_space.type_id;
		let info = r.mem_type_info.get(tid).expect("unknown type");
		// 3. acquire a fresh ptr for this memcell
		// ASSUMES this memcell has a dangling ptr. TODO use Option<NonNull<_>> later for checking
		let fresh_ptr = w.free_mems.get_mut(tid).expect("HFEH").pop().expect("NO FREE PTRS, FAM");
		let mut ptr_refs = 1;
		unsafe {
			*first_me_ge_space.ptr.get() = fresh_ptr;
			let src = *po_pu_space.ptr.get();
			let dest = *first_me_ge_space.ptr.get();
			if port_mover_id.is_some() {
				// mem clone!
				(info.clone_fn)(src, dest);
			} else {
				// mem move!
				std::ptr::copy(src, dest, info.bytes);
			}
		}
		// 4. copy pointers to other memory cells (if any)
		// ASSUMES all destinations have dangling pointers
		for g in me_ge_iter {
			let me_ge_space = r.get_me_pu(g).expect("gggg");
			assert_eq!(*tid, me_ge_space.type_id);

			// 5. dec refs for existing ptr. free if refs are now 0
			unsafe {
				*me_ge_space.ptr.get() = fresh_ptr;
			}
			ptr_refs += 1;
		}
		w.mem_refs.insert(fresh_ptr, ptr_refs);
	}

	// 2. instruct port-getters. delegate waking putter to them (unless 0 getters)

	// tell the putter the number of MOVERS BUT don't wake them up yet!
	match (port_mover_id, po_ge.len()) {
		(Some(m), 1) => {
			// ONLY mover case. mover will wake putter
			r.send_to_getter(m, po_pu | FLAG_YOUR_MOVE);
		},
		(Some(m), l) => {
			// mover AND cloners case. mover will wake putter
			po_pu_space.cloner_countdown.store(l-1, Ordering::SeqCst);
			for g in po_ge.iter().cloned() {
				let payload = if g==m {
					po_pu | FLAG_OTH_EXIST | FLAG_YOUR_MOVE
				} else {
					po_pu | FLAG_OTH_EXIST
				};
				r.send_to_getter(g, payload);
			}
		},
		(None, l) => {
			// no movers
			if l == 0 {
				// no cloners either
				po_pu_space.done_dropbox.send(0);
			} else {
				po_pu_space.cloner_countdown.store(l, Ordering::SeqCst);
				for g in po_ge.iter().cloned() {
					r.send_to_getter(g, po_pu);
				}
			}
		},
	}
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
			BuildMemInfo::new::<u32>(),
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
struct BuildMemInfo {
	size: usize,
	align: usize,
	type_id: TypeId,
	drop_fn: DropFnPtr,
	clone_fn: CloneFnPtr,
}
impl BuildMemInfo {
	pub fn new<T: 'static + PortData>() -> Self {
		let drop_fn: DropFnPtr = |ptr| unsafe {
			let ptr: &mut ManuallyDrop<T> = transmute(ptr);
			ManuallyDrop::drop(ptr);
		};
		let clone_fn: CloneFnPtr = |src, dest| unsafe {
			let datum = T::clone_fn(transmute(src));
			let dest: &mut T = transmute(dest);
			*dest = datum;
		};
		Self {
			size: std::mem::size_of::<T>(),
			align: std::mem::align_of::<T>(),
			type_id: TypeId::of::<T>(),
			drop_fn,
			clone_fn,
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


// struct StateSet {
// 	vals: BitSet, // 0 for False, 1 for True
// 	mask: BitSet, // 1 for X (overrides val)
// }
// impl StateSet {
// 	fn satisfied(&self, state: &BitSet) -> bool {
// 		unimplemented!()
// 	}
// }


// TODO
// 1. port groups: creation, destruction and interaction
// 2. (runtime) stateset as (mask: BitSet, vals: BitSet)
// 3. imagine the token api jazz

// struct PortGroup {
// 	leader: PortId,
// }
// impl PortGroup {

// }

// // TODO instead of a hashmap for typeids, rather use Rc