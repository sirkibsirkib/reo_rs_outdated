
////////// DEBUG DEBUG
#![allow(dead_code)]

use crate::{PortId, RuleId};
use hashbrown::HashMap;
use std::{
	any::TypeId,
	mem::{transmute, ManuallyDrop},
	cell::UnsafeCell,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering, AtomicPtr},
	},
	marker::PhantomData,
};
use parking_lot::Mutex;
use crate::bitset::BitSet;
use std_semaphore::Semaphore;

type DropFnPtr = fn(*mut u8);
type CloneFnPtr = fn(*mut u8, *mut u8);

const FLAG_YOUR_MOVE: usize = (1 << 63);
const FLAG_OTH_EXIST: usize = (1 << 62);

struct MePuSpace {
	ptr: AtomicPtr<u8>, // acts as *mut T AND key to mem_slot_meta_map
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
	fn make_empty(&self, my_id: PortId, r: &ProtoR, w: &mut ProtoActive, do_drop: bool) {
		println!("::make_empty| id={} do_drop={}", my_id, do_drop);
		let ptr = self.get_ptr();
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
			w.ready.set(my_id + r.mem_get_id_start()); // GETTER ready
			w.free_mems.get_mut(tid).expect("??").push(ptr);
		}
	}
}

trait HasRacePtr {
	fn set_ptr(&self, ptr: *mut u8);
	fn get_ptr(&self) -> *mut u8;
}
impl HasRacePtr for PoPuSpace {
	fn set_ptr(&self, ptr: *mut u8) {
		self.ptr.store(ptr, Ordering::SeqCst);
	}
	fn get_ptr(&self) -> *mut u8 {
		self.ptr.load(Ordering::SeqCst)
	}
}
impl HasRacePtr for MePuSpace {
	fn set_ptr(&self, ptr: *mut u8) {
		self.ptr.store(ptr, Ordering::SeqCst);
	}
	fn get_ptr(&self) -> *mut u8 {
		self.ptr.load(Ordering::SeqCst)
	}
}

struct PoPuSpace {
	ptr: AtomicPtr<u8>,
	cloner_countdown: AtomicUsize,
	mover_sema: Semaphore,
	dropbox: MsgDropbox,
}
impl PoPuSpace {
	fn new() -> Self {
		Self {
			ptr: AtomicPtr::new(std::ptr::null_mut()),
			cloner_countdown: 0.into(),
			mover_sema: Semaphore::new(0),
			dropbox: MsgDropbox::new(),
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
		println!("::get_sig| (start)");
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
					me_pu_space.make_empty(putter_id, &a.r, &mut a.w.lock().active, true);
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
					po_pu_space.dropbox.send(1);
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
		println!("::get| start");
		let msg = self.dropbox.recv();
		let putter_id: PortId = msg & (!FLAG_YOUR_MOVE) & (!FLAG_OTH_EXIST);
		let i_move = (msg & FLAG_YOUR_MOVE) > 0;
		let conflict = msg & FLAG_OTH_EXIST > 0;

		println!("::get| putter_id={} i_move={} conflict={}", putter_id, i_move, conflict);

		// I requested move, so if it was denied SOMEONE must move
		assert!(i_move || conflict); 
		match a.r.get_space(putter_id) {
			SpaceRef::MePu(me_pu_space) => {
				println!("::get| mepu branch");
				let ptr: &T = unsafe {
					transmute(me_pu_space.get_ptr())
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
					me_pu_space.make_empty(putter_id, &a.r, &mut a.w.lock().active, false);
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
				println!("::get| popu branch");
				let ptr: &T = unsafe {
					transmute(po_pu_space.get_ptr())
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
					println!("::get| releasing popu");
					po_pu_space.dropbox.send(1);
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
	commitment: Option<Commitment>,
	ready_tentative: BitSet,
}
impl ProtoW {
	fn enter(&mut self, r: &ProtoR, my_id: PortId) {
		self.active.ready.set(my_id);
		if self.commitment.is_some() {
			// some rule is waiting for completion
			return;
		}
		let mut num_tenatives = 0;
		println!("enter with id={:?}. bitset now {:?}", my_id, &self.active.ready);
		'outer: loop {
			'inner: for (rule_id, rule) in self.rules.iter().enumerate() {
				if self.active.ready.is_superset(&rule.guard_ready) && (rule.guard_fn)(r) {
					// committing to this rule!

					// make all the guarded bits UNready at once.
					self.active.ready.difference_with(&rule.guard_ready);
					// TODO tentatives

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
						self.commitment = Some(Commitment {
							rule_id,
							awaiting: num_tenatives,
						});
						println!("committed to rid {}", rule_id);
						break 'inner;
					}
					// no tenatives! proceed

					println!("... firing {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard_ready);

					(rule.fire_fn)(Firer {
						r,
						w: &mut self.active,
					});
					println!("... FIRE COMPLETE {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard_ready);
					if !rule.guard_ready.test(my_id) {
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
		let comm: &mut Commitment = self.commitment.as_mut().expect("BUT IT MUST BE");
		assert_eq!(comm.rule_id, expecting_rule);
		self.ready_tentative.set_to(tent_it, false);
		comm.awaiting -= 1;
		if comm.awaiting > 0 {
			return; // someone else will finish up
		}
		let rule = &self.rules[comm.rule_id];
		self.commitment = None;
		(rule.fire_fn)(Firer {
			r,
			w: &mut self.active,
		});
	}
}

struct Rule {
	guard_ready: BitSet,
	guard_fn: fn(&ProtoR) -> bool,
	fire_fn: fn(Firer),
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
	fn mem_get_id_start(&self) -> usize {
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
	fn id_is_port(&self, id: PortId) -> bool {
		self.me_pu.len() <= id && id < self.mem_get_id_start()
	}
	fn get_space(&self, id: PortId) -> SpaceRef {
		use SpaceRef::*;
		let mpl = self.me_pu.len();
		let ppl = self.po_pu.len();
		self.me_pu.get(id).map(MePu)
		.or(self.po_pu.get(id - mpl).map(PoPu))
		.or(self.po_ge.get(id - mpl - ppl).map(PoGe))
		.unwrap_or(None)
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



#[derive(Debug, Copy, Clone)]
pub enum PortGroupError {
	EmptyGroup,
	MemId(PortId),
	SynchronousWithRule(RuleId),
}
#[derive(Default)]
pub struct PortGroupBuilder {
	core: Option<(PortId, Arc<ProtoAll>)>,
	members: BitSet,
}

pub struct PortGroup {
	p: Arc<ProtoAll>,
	leader: PortId,
	disambiguation: HashMap<RuleId, PortId>,
}
impl PortGroup {
	unsafe fn new(p: &Arc<ProtoAll>, port_set: &BitSet) -> Result<PortGroup, PortGroupError> {
		use PortGroupError::*;
		let mut w = p.w.lock();
		// 1. check that NO rule contains multiple ports in the set
		for (rule_id, rule) in w.rules.iter().enumerate() {
			if rule.guard_ready.iter_and(port_set).count() > 1 {
				return Err(SynchronousWithRule(rule_id))
			}
		}
		// 2. check no group id is associated with memory
		for id in port_set.iter_sparse() {
			if !p.r.id_is_port(id) {
				return Err(MemId(id))
			}
		}
		match port_set.iter_sparse().next() {
			Some(leader) => {
				let mut disambiguation = HashMap::new();
				// 2. change occurrences of any port IDs in the set to leader
				for (rule_id, rule) in w.rules.iter_mut().enumerate() {
					if let Some(specific_port) = rule.guard_ready.iter_and(port_set).next() {
						disambiguation.insert(rule_id, specific_port);
						rule.guard_ready.set_to(specific_port, false);
						rule.guard_ready.set(leader);
					}
				}
				Ok(PortGroup {
					p: p.clone(),
					leader,
					disambiguation,
				})
			},
			None => Err(EmptyGroup),
		}
	}
	pub fn ready_wait_determine_commit(&self) -> PortId {
		let space = self.p.r.get_space(self.leader);
		{
			let mut w = self.p.w.lock();
			w.ready_tentative.set(self.leader);
			w.active.ready.set(self.leader);
		}
		let rule_id = match space {
			SpaceRef::PoPu(po_pu_space) => po_pu_space.dropbox.recv(),
			SpaceRef::PoGe(po_ge_space) => po_ge_space.dropbox.recv(),
			_ => panic!("BAD ID"),
		};
		*self.disambiguation.get(&rule_id).expect("SHOULD BE OK")
	}
}
impl Drop for PortGroup {
	// TODO ensure you can't change leaders or something whacky
	fn drop(&mut self) {
		let mut w = self.p.w.lock();
		for (rule_id, rule) in w.rules.iter_mut().enumerate() {
			if let Some(&specific_port) = self.disambiguation.get(&rule_id) {
				if self.leader != specific_port {
					rule.guard_ready.set_to(self.leader, false);
					rule.guard_ready.set(specific_port);
				}
			}
		}
	}
}

// [MePu | PoPu | PoGe | MeGe]
struct ProtoAll {
	r: ProtoR,
	w: Mutex<ProtoW>,
}
impl ProtoAll {
	fn new(mem_infos: Vec<BuildMemInfo>, num_port_putters: usize, num_port_getters: usize, rules: Vec<Rule>) -> Self {
		let mem_get_id_start = mem_infos.len() + num_port_putters + num_port_getters;

		let (mem_data, me_pu, mem_type_info, free_mems, ready) = Self::build_buffer(mem_infos, mem_get_id_start);
		let po_pu = (0..num_port_putters).map(|_| PoPuSpace::new()).collect();
		let po_ge = (0..num_port_getters).map(|_| PoGeSpace::new()).collect();
		let r = ProtoR { mem_data, me_pu, po_pu, po_ge, mem_type_info };
		let w = Mutex::new(ProtoW {
			rules,
			active: ProtoActive {
				ready,
				free_mems,
				mem_refs: HashMap::default(),
			},
			commitment: None,
			ready_tentative: BitSet::default(),
		});
		ProtoAll {w, r}
	}
	fn build_buffer(infos: Vec<BuildMemInfo>, mem_get_id_start: usize) ->
	(
		Vec<u8>, // buffer
		Vec<MePuSpace>,
		HashMap<TypeId, MemTypeInfo>,
		HashMap<TypeId, Vec<*mut u8>>,
		BitSet,
	) {
		let mut capacity = 0;
		let mut offsets_n_typeids = vec![];
		let mut mem_type_info = HashMap::default();
		let mut ready = BitSet::default();
		let mut free_mems = HashMap::default();
		for (mem_id, info) in infos.into_iter().enumerate() {
			ready.set(mem_id + mem_get_id_start); // set GETTER
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

		// meta-offset used to ensure the start of the vec alsigns to 64-bits (covers all cases)
		// almost always unnecessary
		let mut buf: Vec<u8> = Vec::with_capacity(capacity + 8);
		let mut meta_offset: isize = 0;
		while (unsafe { buf.as_ptr().offset(meta_offset) }) as usize % 8 != 0 {
			meta_offset += 1;
		}
		unsafe {
			buf.set_len(capacity);
		}
		let ptrs = offsets_n_typeids.into_iter().map(|(offset, type_id)| unsafe {
			let ptr: *mut u8 = buf.as_mut_ptr().offset(offset as isize + meta_offset);
			free_mems.entry(type_id).or_insert(vec![]).push(ptr);
			MePuSpace::new(ptr, type_id)
		}).collect();
		(buf, ptrs, mem_type_info, free_mems, ready)
	}
}


unsafe impl<T: PortData> Send for Getter<T> {}
unsafe impl<T: PortData> Sync for Getter<T> {}
pub struct Getter<T: PortData> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	pub(crate) id: PortId,
}
impl<T: PortData> Getter<T> {
	pub fn get_signal(&mut self) {
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_move_intention(false);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.get_signal(&self.p);
	}
	pub fn get(&mut self) -> T {
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
pub struct Putter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	pub(crate) id: PortId,
}
impl<T> Putter<T> {
	pub fn put(&mut self, datum: T) -> Option<T> {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum & set owned to true
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let msg = po_pu.dropbox.recv();
		match msg {
			0 => Some(datum),
			1 => {
				std::mem::forget(datum);
				None
			},
			_ => panic!("putter got a bad msg"),
		}
	}
	pub fn put_lossy(&mut self, datum: T) {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum & set owned to true
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let msg = po_pu.dropbox.recv();
		std::mem::forget(datum);
		assert!(msg == 0 || msg == 1); // sanity check
	}
}


struct Firer<'a> {
	r: &'a ProtoR,
	w: &'a mut ProtoActive,
}
impl<'a> Firer<'a> {
	fn mem_to_nowhere(&mut self, me_pu: PortId) {
		let me_pu_space = self.r.get_me_pu(me_pu).expect("fewh");
		me_pu_space.make_empty(me_pu, self.r, self.w, true);
	}

	fn mem_to_ports(&mut self, me_pu: PortId, po_ge: &[PortId]) {
		let me_pu_space = self.r.get_me_pu(me_pu).expect("fewh");

		// 1. port getters have move-priority
		let port_mover_id = self.find_mover(po_ge);

		// 3. instruct port-getters. delegate clearing putters to them (unless 0 getters)
		match (port_mover_id, po_ge.len()) {
			(Some(m), 1) => {
				// ONLY mover case. mover will wake putter
				self.r.send_to_getter(m, me_pu | FLAG_YOUR_MOVE);
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
					self.r.send_to_getter(g, payload);
				}
			},
			(None, l) => {
				// no movers
				if l == 0 {
					// no cloners either
					me_pu_space.make_empty(me_pu, self.r, self.w, true);
				} else {
					me_pu_space.cloner_countdown.store(l, Ordering::SeqCst);
					for g in po_ge.iter().cloned() {
						self.r.send_to_getter(g, me_pu);
					}
				}
			},
		}
	}

	fn mem_to_mem_and_ports(&mut self, me_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
		println!("mem_to_mem_and_ports");
		let me_pu_space = self.r.get_me_pu(me_pu).expect("fewh");
		let tid = &me_pu_space.type_id;
		let src = me_pu_space.get_ptr();

		// 1. copy pointers to other memory cells
		// ASSUMES destinations have dangling pointers TODO checks
		for g in me_ge.iter().cloned() {
			let me_ge_space = self.r.get_me_pu(g).expect("gggg");
			assert_eq!(*tid, me_ge_space.type_id);
			me_ge_space.set_ptr(src);
			self.w.ready.set(g); // PUTTER is ready
		}
		// 2. increment memory pointer refs of me_pu
		let src_refs = self.w.mem_refs.get_mut(&src).expect("UNKNWN");
		*src_refs += me_ge.len();
		self.mem_to_ports(me_pu, po_ge);
	}


	fn find_mover(&self, getters: &[PortId]) -> Option<PortId> {
		getters.iter().filter(|id| {
			let po_ge = self.r.get_po_ge(**id).expect("bad id");
			po_ge.get_move_intention()
		}).next().or(getters.get(0)).cloned()
	}

	fn port_to_mem_and_ports(&mut self, po_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
		println!("port_to_mem_and_ports");

		assert!(po_pu < FLAG_OTH_EXIST && po_pu < FLAG_YOUR_MOVE);
		let po_pu_space = self.r.get_po_pu(po_pu).expect("ECH");

		// 1. port getters have move-priority
		let port_mover_id = self.find_mover(po_ge);
		println!("::port_to_mem_and_ports| port_mover_id={:?}", port_mover_id);

		// 2. populate memory cells if necessary
		let mut me_ge_iter = me_ge.iter().cloned();
		if let Some(first_me_ge) = me_ge_iter.next() {
			println!("::port_to_mem_and_ports| first_me_ge={:?}", first_me_ge);
			let first_me_ge_space = self.r.get_me_pu(first_me_ge).expect("wfew");
			self.w.ready.set(first_me_ge); // GETTER is ready
			let tid = &first_me_ge_space.type_id;
			let info = self.r.mem_type_info.get(tid).expect("unknown type");
			// 3. acquire a fresh ptr for this memcell
			// ASSUMES this memcell has a dangling ptr. TODO use Option<NonNull<_>> later for checking
			let fresh_ptr = self.w.free_mems.get_mut(tid).expect("HFEH").pop().expect("NO FREE PTRS, FAM");
			let mut ptr_refs = 1;
			first_me_ge_space.set_ptr(fresh_ptr);
			let src = po_pu_space.get_ptr();
			let dest = first_me_ge_space.get_ptr();
			if port_mover_id.is_some() {
				// mem clone!
				println!("::port_to_mem_and_ports| mem clone");
				(info.clone_fn)(src, dest);
			} else {
				// mem move!
				println!("::port_to_mem_and_ports| mem move");
				unsafe { std::ptr::copy(src, dest, info.bytes) };
			}
			// 4. copy pointers to other memory cells (if any)
			// ASSUMES all destinations have dangling pointers
			for g in me_ge_iter {
				println!("::port_to_mem_and_ports| mem_g={:?}", g);
				let me_ge_space = self.r.get_me_pu(g).expect("gggg");
				assert_eq!(*tid, me_ge_space.type_id);

				// 5. dec refs for existing ptr. free if refs are now 0
				me_ge_space.set_ptr(fresh_ptr);
				self.w.ready.set(g); // GETTER is ready
				ptr_refs += 1;
			}
			println!("::port_to_mem_and_ports| ptr_refs={}", ptr_refs);
			self.w.mem_refs.insert(fresh_ptr, ptr_refs);
		}

		// 2. instruct port-getters. delegate waking putter to them (unless 0 getters)

		// tell the putter the number of MOVERS BUT don't wake them up yet!
		match (port_mover_id, po_ge.len()) {
			(Some(m), 1) => {
				println!("::port_to_mem_and_ports| MOV={},L={}", m, 1);
				// ONLY mover case. mover will wake putter
				self.r.send_to_getter(m, po_pu | FLAG_YOUR_MOVE);
			},
			(Some(m), l) => {
				println!("::port_to_mem_and_ports| MOV={},L={}", m, l);
				// mover AND cloners case. mover will wake putter
				po_pu_space.cloner_countdown.store(l-1, Ordering::SeqCst);
				for g in po_ge.iter().cloned() {
					let payload = if g==m {
						po_pu | FLAG_OTH_EXIST | FLAG_YOUR_MOVE
					} else {
						po_pu | FLAG_OTH_EXIST
					};
					self.r.send_to_getter(g, payload);
				}
			},
			(None, l) => {
				println!("::port_to_mem_and_ports| MOV=.,L={}", l);
				// no movers
				if l == 0 {
					// no cloners either
					po_pu_space.dropbox.send(0);
				} else {
					po_pu_space.cloner_countdown.store(l, Ordering::SeqCst);
					for g in po_ge.iter().cloned() {
						self.r.send_to_getter(g, po_pu);
					}
				}
			},
		}
	}
}


pub trait PortData: Sized {
	fn clone_fn(_t: &Self) -> Self {
		panic!("Don't know how to clone this!")
	}
}
impl<T:Clone> PortData for T {
	fn clone_fn(t: &Self) -> Self {
		T::clone(t)
	}
} 

pub trait Proto: Sized {
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
				guard_ready: bitset!{0, 3},
				guard_fn: |_r| true,
				fire_fn: |mut _f| {
					_f.mem_to_ports(0, &[3]);
				},
			},
			Rule {
				guard_ready: bitset!{1, 2, 3, 4},
				guard_fn: |_r| true,
				fire_fn: |mut _f| {
					_f.port_to_mem_and_ports(1, &[], &[3]);
					_f.port_to_mem_and_ports(2, &[0], &[]);
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
				println!("GOT {:?} | {:?}", g3.get(), g3.get());
			}
		});
	}).expect("WENT OK");
}