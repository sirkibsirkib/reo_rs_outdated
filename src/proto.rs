
////////// DEBUG DEBUG
#![allow(dead_code)]

use std::time::Duration;
use std::ptr::NonNull;
use core::ops::Range;
use crate::{LocId, RuleId};
use hashbrown::{HashMap};
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

type GuardFn = fn(&ProtoR) -> bool;

// an untyped CloneFn pointer. Null variant represents an undefined function
// which will cause explicit panic if execute() is invoked.
// UNSAFE if the type pointed to does not match the type used to instantiate the ptr.
#[derive(Debug, Copy, Clone)]
struct CloneFn(Option<NonNull<fn(*mut u8, *mut u8)>>);
impl CloneFn {
	fn new_defined<T>() -> Self {
		let clos: fn(*mut u8, *mut u8) = |src, dest| unsafe {
			let datum = T::maybe_clone(transmute(src));
			let dest: &mut T = transmute(dest);
			*dest = datum;
		};
		let opt_nn = NonNull::new( unsafe { transmute(clos) });
		debug_assert!(opt_nn.is_some());
		CloneFn(opt_nn)
	}
	fn new_undefined() -> Self {
		CloneFn(None)
	}
	/// safe ONLY IF:
	///  - src is &T to initialized memory
	///  - dst is &mut T to uninitialized memory
	///  - T matches the type provided when creating this CloneFn in `new_defined`
	#[inline]
	unsafe fn execute(self, src: *mut u8, dst: *mut u8) {
		if let Some(x) = self.0 {
			(*x.as_ptr())(src, dst);
		} else {
			panic!("proto attempted to clone an unclonable type!");
		}
	}
}

// an untyped DropFn pointer. Null variant represents a trivial drop Fn (no behavior).
// new() automatically handles types with trivial drop functions
// UNSAFE if the type pointed to does not match the type used to instantiate the ptr.
#[derive(Debug, Copy, Clone)]
struct DropFn(Option<NonNull<fn(*mut u8)>>);
impl DropFn {
	fn new<T>() -> Self {
		if std::mem::needs_drop::<T>() {
			let clos: fn(*mut u8) = |ptr| unsafe {
	            let ptr: &mut ManuallyDrop<T> = transmute(ptr);
	            ManuallyDrop::drop(ptr);
	        };
	        DropFn(NonNull::new( unsafe { transmute(clos) }))
		} else {
			DropFn(None)
		}
	}
	/// safe ONLY IF the given pointer is of the type this DropFn was created for.
	#[inline]
	unsafe fn execute(self, on: *mut u8) {
		if let Some(x) = self.0 {
			(*x.as_ptr())(on);
		} else {
			// None variant represents a drop with no effect
		}
	}
}

// facilitates memcell type erasure. this structure contains all the information
// necessary to specialize CLONE, DROP and MOVE procedures on the type-erased ptrs
#[derive(Debug, Clone, Copy)]
pub struct MemTypeInfo {
	type_id: TypeId,
	drop_fn: DropFn,
	clone_fn: CloneFn,
	is_copy: bool,
	bytes: usize,
	align: usize,
}
impl MemTypeInfo {
	pub fn new<T: 'static>() -> Self {
		// always true: clone_fn.is_none() || !is_copy
		Self {
			bytes: std::mem::size_of::<T>(),
			type_id: TypeId::of::<T>(),
			drop_fn: DropFn::new::<T>(),
			clone_fn: CloneFn::new_defined::<T>(),
			align: std::mem::align_of::<T>(),
			is_copy: <T as MaybeCopy>::IS_COPY,
		}
	}
}

/// tracks the datum associated with this memory cell (if any).
struct MemoSpace {
	ptr: AtomicPtr<u8>, // acts as *mut T AND key to mem_slot_meta_map
	cloner_countdown: AtomicUsize,
	mover_sema: Semaphore,
	type_info: Arc<MemTypeInfo>,
}
impl MemoSpace {
	fn new(ptr: *mut u8, type_info: Arc<MemTypeInfo>) -> Self {
		Self {
			ptr: ptr.into(),
			cloner_countdown: 0.into(),
			mover_sema: Semaphore::new(0),
			type_info,
		}
	}
	fn make_empty(&self, my_id: LocId, r: &ProtoR, w: &mut ProtoActive, do_drop: bool) {
		// println!("::make_empty| id={} do_drop={}", my_id, do_drop);
		let ptr = self.get_ptr();
		let src_refs = w.mem_refs.get_mut(&ptr).expect("UNKNWN");
		let tid = &self.type_info.type_id;
		*src_refs -= 1;
		if *src_refs == 0 {
			// contents need to be dropped! ptr needs to be made free
			w.mem_refs.remove(&ptr).expect("hhh");
			if do_drop {
				unsafe { self.type_info.drop_fn.execute(ptr) }
			}
			w.free_mems.get_mut(tid).expect("??").push(ptr);
		}
		println!("MEMCELL BECAME EMPTY. SET");
		w.ready.set(r.mem_getter_id(my_id)); // GETTER ready
	}
}

// trait to cut down boilerplate in the rest of the lib
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
impl HasRacePtr for MemoSpace {
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
	want_data: UnsafeCell<bool>,
}
impl PoGeSpace {
	fn new() -> Self {
		Self {
			dropbox: MsgDropbox::new(),
			want_data: false.into(),
		}
	}
	fn set_want_data(&self, want_data: bool) {
		unsafe {
			*self.want_data.get() = want_data
		}
	}
	fn get_want_data(&self) -> bool {
		unsafe {
			*self.want_data.get()
		}
	}
	#[inline]
	fn participate_get<T>(&self, a: &ProtoAll, msg: usize) -> T {
		let (case, putter_id) = DataGetCase::parse_msg(msg); 
		println!("... GOT MSG");
		match a.r.get_space(putter_id) {
			SpaceRef::Memo(memo_space) => {
				let ptr: &T = unsafe {
					transmute(memo_space.get_ptr())
				};
				if T::IS_COPY {
					// everyone acts as the mover
					let datum = unsafe { std::ptr::read(ptr) };
					// cloner_countdown always initialized to #getters. 
					let was = memo_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						println!("WILL TRY LOCK...");
						let mut w = a.w.lock();
						println!("GOT LOCK!");
						// 2. release putter. DO NOT drop
						memo_space.make_empty(putter_id, &a.r, &mut w.active, false);
						// 3. notify state waiters
						let ProtoW { ref active, ref mut awaiting_states, .. } = &mut w as &mut ProtoW;
						ProtoW::notify_state_waiters(&active.ready, awaiting_states, &a.r);
					}
					datum
				} else {
					// n-1 cloners. last one releases the mover. `case` tells us if that's us.
					if case.i_move() {
						if case.mover_must_wait() {
							memo_space.mover_sema.acquire();
						}
						let datum = unsafe { std::ptr::read(ptr) };
						let mut w = a.w.lock();
						// 2. release putter. DO NOT drop
						memo_space.make_empty(putter_id, &a.r, &mut w.active, false);
						// 3. notify state waiters
						let ProtoW { ref active, ref mut awaiting_states, .. } = &mut w as &mut ProtoW;
						ProtoW::notify_state_waiters(&active.ready, awaiting_states, &a.r);
						datum
					} else {
						let datum = T::maybe_clone(ptr);
						let was = memo_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
						// cloner_countdown always initialized to #getters. mover isnt participating here
						if was == 2 {
							// I was last! release mover (who MUST exist)
							memo_space.mover_sema.release();
						}
						datum
					}
				}
			},
			SpaceRef::PoPu(po_pu_space) => {
				let ptr: &T = unsafe {
					transmute(po_pu_space.get_ptr())
				};
				if T::IS_COPY {
					let datum = unsafe { std::ptr::read(ptr) };
					let was = po_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						println!("... i am last. notifying putter");
						po_pu_space.dropbox.send(1);
					}
					datum
				} else {
					if case.i_move() {
						if case.mover_must_wait() {
							po_pu_space.mover_sema.acquire();
						}
						let datum = unsafe { std::ptr::read(ptr) };
						po_pu_space.dropbox.send(1);
						datum
					} else {
						let datum = T::maybe_clone(ptr);
						let was = po_pu_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
						// offset by 1 because the mover isnt participating
						if was == 2 && case.mover_must_wait() {
							// I was last! release mover (who MUST exist)
							po_pu_space.mover_sema.release();
						}
						datum
					}
				}
			},
			_ => panic!("bad putter!"),
		}
	}
	#[inline]
	fn participate_get_timeout<T>(&self, a: &ProtoAll, timeout: Duration, my_id: LocId) -> Option<T> {
		println!("getting ... ");
		let msg = match self.dropbox.recv_timeout(timeout) {
			Some(msg) => msg,
			None => {
				if a.w.lock().active.ready.set_to(my_id, false) {
					// managed reverse my readiness
					return None
				} else {
					// readiness has already been consumed
					println!("too late");
					self.dropbox.recv()
				}
			}
		};
		Some(self.participate_get(a, msg))
	}
}

enum SpaceRef<'a> {
	Memo(&'a MemoSpace),
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

struct StateWaiter {
	state: BitSet,
	whom: LocId,
}
struct ProtoW {
	rules: Vec<Rule2>,
	active: ProtoActive,
	commitment: Option<Commitment>,
	ready_tentative: BitSet,
	awaiting_states: Vec<StateWaiter>
}
impl ProtoW {
	fn notify_state_waiters(ready: &BitSet, awaiting_states: &mut Vec<StateWaiter>, r: &ProtoR) {
		awaiting_states.retain(|awaiting_state| {
			let retain = if ready.is_superset(&awaiting_state.state) {
				match r.get_space(awaiting_state.whom) {
					SpaceRef::PoPu(space) => space.dropbox.send_nothing(),
					SpaceRef::PoGe(space) => space.dropbox.send_nothing(),
					_ => panic!("bad state-waiter LocId!"),
				};
				false
			} else {
				true
			};
			retain
		})
	}
	fn enter(&mut self, r: &ProtoR, my_id: LocId) {
		println!("ENTER WITH GOAL {}", my_id);
		self.active.ready.set(my_id);
		println!("READINESS IS {:?}", &self.active.ready);
		if self.commitment.is_some() {
			// some rule is waiting for completion
			return;
		}
		let mut num_tenatives = 0;
		// println!("enter with id={:?}. bitset now {:?}", my_id, &self.active.ready);
		'outer: loop {
			'inner: for (rule_id, rule) in self.rules.iter().enumerate() {
				if self.active.ready.is_superset(&rule.guard_ready) && (rule.guard_fn)(r) {
					// committing to this rule!
					println!("FIRING {}", rule_id);

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
						// println!("committed to rid {}", rule_id);
						break 'inner;
					}
					// no tenatives! proceed

					// println!("... firing {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard_ready);

					rule.fire(Firer {
						r,
						w: &mut self.active,
					});
					Self::notify_state_waiters(&self.active.ready, &mut self.awaiting_states, r);
					// println!("... FIRE COMPLETE {:?}. READY: {:?} GUARD {:?}", rule_id, &self.active.ready, &rule.guard_ready);
					if !rule.guard_ready.test(my_id) {
						// job done!
						break 'inner;
					} else {
						continue 'outer;
					}
				}
			}
			// none matched
			// println!("... exiting");
			return
		}
	}
	fn enter_committed(&mut self, r: &ProtoR, tent_it: LocId, expecting_rule: usize) {
		let comm: &mut Commitment = self.commitment.as_mut().expect("BUT IT MUST BE");
		debug_assert_eq!(comm.rule_id, expecting_rule);
		self.ready_tentative.set_to(tent_it, false);
		comm.awaiting -= 1;
		if comm.awaiting > 0 {
			return; // someone else will finish up
		}
		let rule = &self.rules[comm.rule_id];
		self.commitment = None;
		rule.fire(Firer {
			r,
			w: &mut self.active,
		});
	}
}



pub struct ProtoR {
	mem_data: Vec<u8>,
	po_pu: Vec<PoPuSpace>, // id range 0..#PoPu
 	po_ge: Vec<PoGeSpace>, // id range #PoPu..(#PoPu + #PoGe)
	me_pu: Vec<MemoSpace>, // id range (#PoPu + #PoGe)..(#PoPu + #PoGe + #Memo)
 	// me_ge doesn't need a space
 	// mem_type_info: HashMap<TypeId, MemTypeInfo>,
}
impl ProtoR {
	fn send_to_getter(&self, id: LocId, msg: usize) {
		self.get_po_ge(id).expect("NOPOGE").dropbox.send(msg)
	}
	#[inline]
	fn mem_getter_id(&self, id: LocId) -> LocId {
		id + self.me_pu.len()
	}
	fn get_po_pu(&self, id: LocId) -> Option<&PoPuSpace> {
		self.po_pu.get(id)
	}
	fn get_po_ge(&self, id: LocId) -> Option<&PoGeSpace> {
		self.po_ge.get(id - self.po_pu.len())
	}
	fn get_me_pu(&self, id: LocId) -> Option<&MemoSpace> {
		self.me_pu.get(id - self.po_pu.len() - self.po_ge.len())
	}
	fn loc_is_port(&self, id: LocId) -> bool {
		id < (self.po_pu.len() + self.po_ge.len())
	}
	fn get_space(&self, id: LocId) -> SpaceRef {
		use SpaceRef::*;
		let ppl = self.po_pu.len();
		let pgl = self.po_ge.len();
		self.po_pu.get(id).map(PoPu)
		.or(self.po_ge.get(id - ppl).map(PoGe))
		.or(self.me_pu.get(id - ppl - pgl).map(Memo))
		.unwrap_or(None)
	}
}

struct MsgDropbox {
	s: crossbeam::Sender<usize>,
	r: crossbeam::Receiver<usize>,
}
impl MsgDropbox {
	fn new() -> Self {
		let (s, r) = crossbeam::channel::bounded(1);
		Self { s, r }
	}

	#[inline]
	fn recv_timeout(&self, timeout: Duration) -> Option<usize> {
		self.r.recv_timeout(timeout).ok()
	}
	#[inline]
	fn recv(&self) -> usize {
		let msg = self.r.recv().unwrap();
		println!("MSG {:b} rcvd!", msg);
		msg
	}
	#[inline]
	fn send(&self, msg: usize) {
		println!("MSG {:b} sent!", msg);
		self.s.try_send(msg).expect("Msgbox was full!")
	}
	fn send_nothing(&self) {
		self.send(!0)
	}
	fn recv_nothing(&self) {
		let got = self.recv();
		debug_assert_eq!(got, !0);
	}
}



#[derive(Debug, Copy, Clone)]
pub enum PortGroupError {
	EmptyGroup,
	MemId(LocId),
	SynchronousWithRule(RuleId),
}
#[derive(Default)]
pub struct PortGroupBuilder {
	core: Option<(LocId, Arc<ProtoAll>)>,
	members: BitSet,
}

pub struct PortGroup {
	p: Arc<ProtoAll>,
	leader: LocId,
	disambiguation: HashMap<RuleId, LocId>,
}
impl PortGroup {

	/// block until the protocol is in this state
	unsafe fn await_state(&self, state_pred: BitSet) {
		// TODO check the given state pred is OK? maybe unnecessary since function is internal
		{
			let w = self.p.w.lock();
			if w.active.ready.is_superset(&state_pred) {
				return; // already in the desired state
			}
		} // release lock
		let space = self.p.r.get_space(self.leader);
		match space {
			SpaceRef::PoPu(po_pu_space) => po_pu_space.dropbox.recv_nothing(),
			SpaceRef::PoGe(po_ge_space) => po_ge_space.dropbox.recv_nothing(),
			_ => panic!("BAD ID"),
		}
		// I received a notification that the state is ready!
	}
	unsafe fn new(p: &Arc<ProtoAll>, id_set: &BitSet) -> Result<PortGroup, PortGroupError> {
		use PortGroupError::*;
		let mut w = p.w.lock();
		// 1. check all loc_ids correspond with ports (not memory cells)
		for id in id_set.iter_sparse() {
			if !p.r.loc_is_port(id) {
				return Err(MemId(id))
			}
		}
		// 1. check that NO rule contains multiple ports in the set
		for (rule_id, rule) in w.rules.iter().enumerate() {
			if rule.guard_ready.iter_and(id_set).count() > 1 {
				return Err(SynchronousWithRule(rule_id))
			}
		}
		match id_set.iter_sparse().next() {
			Some(leader) => {
				let mut disambiguation = HashMap::new();
				// 2. change occurrences of any port IDs in the set to leader
				for (rule_id, rule) in w.rules.iter_mut().enumerate() {
					if let Some(specific_port) = rule.guard_ready.iter_and(id_set).next() {
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
	pub fn ready_wait_determine_commit(&self) -> LocId {
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

// Ready layout:  [PoPu|PoGe|MePu|MeGe] 
// Spaces layout: [PoPu|PoGe|Memo]

struct ProtoAll {
	r: ProtoR,
	w: Mutex<ProtoW>,
}
impl ProtoAll {
	fn new(proto_def: &ProtoDef) -> Self {
		let rules = Self::build_rules(proto_def).expect("OH NO");
		let (mem_data, me_pu, free_mems, ready) = Self::build_buffer(proto_def);
		let po_pu = (0..proto_def.num_port_putters).map(|_| PoPuSpace::new()).collect();
		let po_ge = (0..proto_def.num_port_getters).map(|_| PoGeSpace::new()).collect();
		let r = ProtoR { mem_data, me_pu, po_pu, po_ge };
		let w = Mutex::new(ProtoW {
			rules,
			active: ProtoActive {
				ready,
				free_mems,
				mem_refs: HashMap::default(),
			},
			commitment: None,
			ready_tentative: BitSet::default(),
			awaiting_states: vec![],
		});
		ProtoAll {w, r}
	}
	fn build_buffer(proto_def: &ProtoDef) ->
	(
		Vec<u8>, // buffer
		Vec<MemoSpace>,
		HashMap<TypeId, Vec<*mut u8>>,
		BitSet,
	) {
		let mem_get_id_start = proto_def.mem_infos.len() + proto_def.num_port_putters + proto_def.num_port_getters;
		let mut capacity = 0;
		let mut offsets_n_typeids = vec![];
		let mut mem_type_info: HashMap<TypeId, Arc<MemTypeInfo>> = HashMap::default();
		let mut ready = BitSet::default();
		let mut free_mems = HashMap::default();
		for (mem_id, info) in proto_def.mem_infos.iter().enumerate() {
			ready.set(mem_id + mem_get_id_start); // set GETTER
			let rem = capacity % info.align.max(1);
			if rem > 0 {
				capacity += info.align - rem;
			}
			// println!("@ {:?} for info {:?}", capacity, &info);
			offsets_n_typeids.push((capacity, info.type_id));
			mem_type_info.entry(info.type_id).or_insert_with(|| Arc::new(*info));
			capacity += info.bytes.max(1); // make pointers unique even with 0-byte data
		}
		// println!("CAP IS {:?}", capacity);

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
			let type_info = mem_type_info.get(&type_id).expect("Missed a type").clone();
			MemoSpace::new(ptr, type_info)
		}).collect();
		(buf, ptrs, free_mems, ready)
	}
	fn build_rules(proto_def: &ProtoDef) -> Result<Vec<Rule2>, BuildRuleError> {
		use BuildRuleError::*;
		let mut rules = vec![];
		for (_rule_id, rule_def) in proto_def.rule_defs.iter().enumerate() {
			let mut guard_ready = BitSet::default();
			let mut actions = vec![];
			// let mut seen = HashSet::<LocId>::default();
			for action_def in rule_def.actions.iter() {
				let mut mg = vec![];
				let mut pg = vec![];
				let p = action_def.putter;
				if let Some(g) = proto_def.mem_getter_id(p) {
					if guard_ready.test(g) {
						// mem is getter in one action and putter in another
						return Err(SynchronousFiring {loc_id: p})
					}
				}
				for &g in action_def.getters.iter() {
					if proto_def.loc_is_po_ge(g) {
						pg.push(g);
						if guard_ready.set_to(g, true) {
							return Err(SynchronousFiring {loc_id: g})
						}
					} else if proto_def.loc_is_mem(g) {
						mg.push(g);
						if guard_ready.set_to(proto_def.mem_getter_id(g).expect("BAD MEM ID"), true) {
							return Err(SynchronousFiring {loc_id: g})
						}
					} else {
						return Err(LocCannotGet { loc_id: g })
					}
				}
				if guard_ready.set_to(p, true) {
					return Err(SynchronousFiring {loc_id: p})
				}
				// seen.insert(action_def.putter);
				if proto_def.loc_is_po_pu(p) {
					actions.push(Action::PortPut { putter: p, mg, pg});
				} else if proto_def.loc_is_mem(p) {
					actions.push(Action::MemPut { putter: p, mg, pg});
				} else {
					return Err(LocCannotPut { loc_id: p })
				}
			}
			rules.push(Rule2 {
				guard_ready,
				guard_fn: rule_def.guard_fn.clone(),
				actions,
			});
		}
		Ok(rules)
	}
}
#[derive(Debug, Copy, Clone)]
enum BuildRuleError {
	SynchronousFiring { loc_id: LocId },
	LocCannotGet { loc_id: LocId },
	LocCannotPut { loc_id: LocId },
}

#[derive(derive_new::new)]
pub struct ActionDef {
	pub putter: LocId,
	pub getters: &'static [LocId],
}
#[derive(derive_new::new)]
pub struct RuleDef {
	pub guard_fn: Arc<dyn Fn(&ProtoR) -> bool>,
	pub actions: Vec<ActionDef>,
}
unsafe impl Send for RuleDef {}
unsafe impl Sync for RuleDef {}

pub struct ProtoDef {
	pub mem_infos: Vec<MemTypeInfo>,
	pub num_port_putters: usize,
	pub num_port_getters: usize,
	pub rule_defs: Vec<RuleDef>,
}

impl ProtoDef {
	fn mem_getter_id(&self, id: LocId) -> Option<LocId> {
		if self.loc_is_mem(id) {
			Some(id + self.mem_infos.len())
		} else {
			None
		}
	}
	fn loc_is_po_pu(&self, id: LocId) -> bool {
		id < self.num_port_putters
	}
	fn loc_can_put(&self, id: LocId) -> bool {
		self.loc_is_po_pu(id) || self.loc_is_mem(id)
	}
	fn loc_can_get(&self, id: LocId) -> bool {
		self.loc_is_po_ge(id) || self.loc_is_mem(id)
	}
	fn loc_is_po_ge(&self, id: LocId) -> bool {
		let r = self.num_port_putters + self.num_port_getters;
		self.num_port_putters <= id && id < r
	}
	fn loc_is_mem(&self, id: LocId) -> bool {
		let l = self.num_port_putters + self.num_port_getters;
		let r = self.num_port_putters + self.num_port_getters + self.mem_infos.len();
		l <= id && id < r
	}
}

// more protected
struct Rule2 {
	guard_ready: BitSet,
	guard_fn: Arc<dyn Fn(&ProtoR) -> bool>,
	actions: Vec<Action>,
}
impl Rule2 {
	fn fire(&self, mut f: Firer) {
		for a in self.actions.iter() {
			match a {
				Action::PortPut { putter, mg, pg } => f.port_to_locs(*putter, mg, pg),
				Action::MemPut { putter, mg, pg } => f.mem_to_locs(*putter, mg, pg),
			}
		}
	}
}
enum Action {
	PortPut { putter: LocId, mg: Vec<LocId>, pg: Vec<LocId> },
	MemPut { putter: LocId, mg: Vec<LocId>, pg: Vec<LocId> },
}


unsafe impl<T> Send for Getter<T> {}
unsafe impl<T> Sync for Getter<T> {}
pub struct Getter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	pub(crate) id: LocId,
}
impl<T> Getter<T> {
	pub fn get_signal(&mut self) {
		println!("siggy... id={}", self.id);
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_want_data(false);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.dropbox.recv_nothing();
		println!("SIGGY RETURNING");
	}
	pub fn get_timeout(&mut self, timeout: Duration) -> Option<T> {
		println!("dataey... id={}", self.id);
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_want_data(true);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.participate_get_timeout(&self.p, timeout, self.id)
	}
	pub fn get(&mut self) -> T {
		println!("dataey... id={}", self.id);
		// 1. set move intention
		let po_ge = self.p.r.get_po_ge(self.id).expect("HEYa");
		po_ge.set_want_data(true);

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. participate
		po_ge.participate_get(&self.p, po_ge.dropbox.recv())
	}
}


unsafe impl<T> Send for Putter<T> {}
unsafe impl<T> Sync for Putter<T> {}
pub struct Putter<T> {
	p: Arc<ProtoAll>,
	phantom: PhantomData<T>,
	pub(crate) id: LocId,
}
impl<T> Putter<T> {
	pub fn put_timeout_lossy(&mut self, datum: T, timeout: Duration) {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_movers_msg = match po_pu.dropbox.recv_timeout(timeout) {
			Some(msg) => msg,
			None => {
				if self.p.w.lock().active.ready.set_to(self.id, false) {
					// succeeded
					drop(datum);
					return;
				} else {
					// too late
					po_pu.dropbox.recv()
				}
			}
		};
		match num_movers_msg {
			0 => drop(datum),
			1 => std::mem::forget(datum),
			_ => panic!("putter got a bad `num_movers_msg`"),
		}
	}

	pub fn put_timeout(&mut self, datum: T, timeout: Duration) -> Option<T> {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_movers_msg = match po_pu.dropbox.recv_timeout(timeout) {
			Some(msg) => msg,
			None => {
				if self.p.w.lock().active.ready.set_to(self.id, false) {
					// succeeded
					return Some(datum)
				} else {
					// too late
					po_pu.dropbox.recv()
				}
			}
		};
		// println!("putter got msg={}", num_movers_msg);
		match num_movers_msg {
			0 => Some(datum),
			1 => {
				std::mem::forget(datum);
				None
			},
			_ => panic!("putter got a bad `num_movers_msg`"),
		}
	}
	pub fn put(&mut self, datum: T) -> Option<T> {
		let po_pu = self.p.r.get_po_pu(self.id).expect("HEYa");

		// 1. make ready my datum
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_movers_msg = po_pu.dropbox.recv();
		// println!("putter got msg={}", num_movers_msg);
		match num_movers_msg {
			0 => Some(datum),
			1 => {
				std::mem::forget(datum);
				None
			},
			_ => panic!("putter got a bad `num_movers_msg`"),
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
		let num_movers_msg = po_pu.dropbox.recv();
		match num_movers_msg {
			0 => drop(datum),
			1 => std::mem::forget(datum),
			_ => panic!("putter got a bad `num_movers_msg`"),
		}
	}
}

#[derive(Debug, Copy, Clone)]
enum DataGetCase {
	CloneSignalMover,
	AwaitThenMove,
	MoveImmediately,
}
impl DataGetCase {
	fn i_move(self) -> bool {
		use DataGetCase::*;
		match self {
			CloneSignalMover => false,
			AwaitThenMove |
			MoveImmediately => true,
		}
	}
	fn mover_must_wait(self) -> bool {
		use DataGetCase::*;
		match self {
			CloneSignalMover |
			AwaitThenMove => true,
			MoveImmediately => false,
		}
	}
	fn parse_msg(msg: usize) -> (Self, LocId) {
		println!("... GOT {:b}", msg);
		use DataGetCase::*;
		let mask = 0b11 << 62;
		let case = match (msg & mask) >> 62 {
			0b00 => CloneSignalMover,
			0b01 => AwaitThenMove,
			0b10 => MoveImmediately,
			0b11 => panic!("undefined case"),
			_ => unreachable!(),
		};
		(case, msg & !mask)
	}
	fn include_in_msg(self, msg: usize) -> usize {
		use DataGetCase::*;
		assert_eq!(msg & (0b11 << 62), 0);
		let x = match self {
			CloneSignalMover => 0b00,
			AwaitThenMove => 0b01,
			MoveImmediately => 0b10,
		};
		msg | (x << 62)
	}
}

pub struct Firer<'a> {
	r: &'a ProtoR,
	w: &'a mut ProtoActive,
}
impl<'a> Firer<'a> {
	pub fn mem_to_nowhere(&mut self, me_pu: LocId) {
		let memo_space = self.r.get_me_pu(me_pu).expect("fewh");
		memo_space.make_empty(me_pu, self.r, self.w, true);
	}

	// release signal-getters and count getters
	fn release_sig_getters_count_getters(&self, getters: &[LocId]) -> usize {
		let mut count = 0;
		for &g in getters {
			let po_ge = self.r.get_po_ge(g).expect("bad id");
			if po_ge.get_want_data() {
				println!("ID={} wants some data", g);
				count += 1;
			} else {
				println!("SENDING NOTHING TO {}", g);
				po_ge.dropbox.send_nothing()
			}
		}
		count
	}

	pub fn mem_to_ports(&mut self, me_pu: LocId, po_ge: &[LocId]) {
		println!("mem2ports");
		let memo_space = self.r.get_me_pu(me_pu).expect("fewh");

		// 1. port getters have move-priority
		let data_getters_count = self.release_sig_getters_count_getters(po_ge);

		// 3. instruct port-getters. delegate clearing putters to them (unless 0 getters)
		match data_getters_count {
			0 => {
				// cleanup my damn self
				println!("no movers!");
				memo_space.make_empty(me_pu, self.r, self.w, true);
			},
			1 => {
				// solo mover
				memo_space.cloner_countdown.store(1, Ordering::SeqCst);
				let mut i = po_ge.iter().filter(|&&g| self.r.get_po_ge(g).unwrap().get_want_data());
				let mover = *i.next().unwrap();
				println!("mover is {}", mover);
				assert_eq!(None, i.next());
				let msg = DataGetCase::MoveImmediately.include_in_msg(me_pu);
				self.r.send_to_getter(mover, msg);
			},
			n => {
				// n-1 cloners and 1 mover 
				println!("n= {}", n);
				memo_space.cloner_countdown.store(n, Ordering::SeqCst);
				for (i, &g) in po_ge.iter().filter(|&&g| self.r.get_po_ge(g).unwrap().get_want_data()).enumerate() {
					let msg = if i == 0 {
						// I choose you to be the mover!
						DataGetCase::AwaitThenMove
					} else {
						DataGetCase::CloneSignalMover
					}.include_in_msg(me_pu);
					self.r.send_to_getter(g, msg);
				} 
			}
		}
	}

	pub fn mem_to_locs(&mut self, me_pu: LocId, me_ge: &[LocId], po_ge: &[LocId]) {
		println!("mem_to_mem_and_ports");
		let memo_space = self.r.get_me_pu(me_pu).expect("fewh");
		let tid = &memo_space.type_info.type_id;
		let src = memo_space.get_ptr();

		// 1. copy pointers to other memory cells
		// ASSUMES destinations have dangling pointers TODO checks
		for g in me_ge.iter().cloned() {
			let me_ge_space = self.r.get_me_pu(g).expect("gggg");
			debug_assert_eq!(*tid, me_ge_space.type_info.type_id);
			me_ge_space.set_ptr(src);
			self.w.ready.set(g); // PUTTER is ready
		}
		// 2. increment memory pointer refs of me_pu
		let src_refs = self.w.mem_refs.get_mut(&src).expect("UNKNWN");
		*src_refs += me_ge.len();
		self.mem_to_ports(me_pu, po_ge);
	}

	pub fn port_to_locs(&mut self, po_pu: LocId, me_ge: &[LocId], po_ge: &[LocId]) {
		println!("port_to_mem_and_ports");
		let po_pu_space = self.r.get_po_pu(po_pu).expect("ECH");

		// 1. port getters have move-priority
		let data_getters_count = self.release_sig_getters_count_getters(po_ge);
		// println!("::port_to_mem_and_ports| port_mover_id={:?}", port_mover_id);

		// 2. populate memory cells if necessary
		let mut me_ge_iter = me_ge.iter().cloned();
		if let Some(first_me_ge) = me_ge_iter.next() {
			// println!("::port_to_mem_and_ports| first_me_ge={:?}", first_me_ge);
			let first_me_ge_space = self.r.get_me_pu(first_me_ge).expect("wfew");
			self.w.ready.set(first_me_ge); // GETTER is ready
			let tid = &first_me_ge_space.type_info.type_id;
			let info = &first_me_ge_space.type_info;
			// 3. acquire a fresh ptr for this memcell
			// ASSUMES this memcell has a dangling ptr. TODO use Option<NonNull<_>> later for checking
			let fresh_ptr = self.w.free_mems.get_mut(tid).expect("HFEH").pop().expect("NO FREE PTRS, FAM");
			let mut ptr_refs = 1;
			first_me_ge_space.set_ptr(fresh_ptr);
			let src = po_pu_space.get_ptr();
			let dest = first_me_ge_space.get_ptr();
			if data_getters_count > 0 {
				// mem clone!
				unsafe { info.clone_fn.execute(src, dest) }
			} else {
				// mem move!
				unsafe { std::ptr::copy(src, dest, info.bytes) };
			}
			// 4. copy pointers to other memory cells (if any)
			// ASSUMES all destinations have dangling pointers
			for g in me_ge_iter {
				// println!("::port_to_mem_and_ports| mem_g={:?}", g);
				let me_ge_space = self.r.get_me_pu(g).expect("gggg");
				debug_assert_eq!(*tid, me_ge_space.type_info.type_id);

				// 5. dec refs for existing ptr. free if refs are now 0
				me_ge_space.set_ptr(fresh_ptr);
				self.w.ready.set(g); // GETTER is ready
				ptr_refs += 1;
			}
			// println!("::port_to_mem_and_ports| ptr_refs={}", ptr_refs);
			self.w.mem_refs.insert(fresh_ptr, ptr_refs);
		}

		// 2. instruct port-getters. delegate waking putter to them (unless 0 getters)

		// tell the putter the number of MOVERS BUT don't wake them up yet!
		match data_getters_count {
			0 => {
				// cleanup my damn self
				println!("no movers!");
				let mem_movers = if me_ge.is_empty() {0} else {1};
				po_pu_space.dropbox.send(mem_movers);
			},
			1 => {
				// solo mover
				po_pu_space.cloner_countdown.store(1, Ordering::SeqCst);
				let mut i = po_ge.iter().filter(|&&g| self.r.get_po_ge(g).unwrap().get_want_data());
				let mover = *i.next().unwrap();
				println!("mover= {}", mover);
				assert_eq!(None, i.next());
				let msg = DataGetCase::MoveImmediately.include_in_msg(po_pu);
				self.r.send_to_getter(mover, msg);
			},
			n => {
				println!("n= {}", n);
				// n-1 cloners and 1 mover 
				po_pu_space.cloner_countdown.store(n, Ordering::SeqCst);
				for (i, &g) in po_ge.iter().filter(|&&g| self.r.get_po_ge(g).unwrap().get_want_data()).enumerate() {
					let msg = if i == 0 {
						// I choose you to be the mover!
						DataGetCase::AwaitThenMove
					} else {
						DataGetCase::CloneSignalMover
					}.include_in_msg(po_pu);
					self.r.send_to_getter(g, msg);
				} 
			}
		}
	}
}

// struct Rule {
// 	guard_ready: BitSet,
// 	guard_fn: GuardFn,
// 	fire_fn: fn(Firer),
// }
// impl Rule {
// 	fn fire(&self, f: Firer) {
// 		(self.fire_fn)(f)
// 	}
// }


// pub trait PortData: Sized + 'static {
// 	fn clone_fn(_t: &Self) -> Self {
// 		panic!("Don't know how to clone this!")
// 	}
// }
// impl<T: Clone + 'static> PortData for T {
// 	fn clone_fn(t: &Self) -> Self {
// 		T::clone(t)
// 	}
// } 

pub trait Proto: Sized {
	type Interface: Sized;
	fn proto_def() -> ProtoDef;
	fn instantiate() -> Self::Interface;
}

fn in_rng(x: &Range<usize>, y: usize) -> bool {
	x.start <= y && y < x.end
}

macro_rules! new_rule_def {
	($guard_clos:expr ;    $( $p:tt => $(  $g:tt ),*   );* ) => {{
		RuleDef {
			guard_fn: Arc::new($guard_clos),
			actions: vec![ 
				$(ActionDef {
					putter: $p,
					getters: &[$($g),*],
				}),*
			],
		}
	}}
}

// lazy_static::lazy_static! {
//     static ref MY_PROTO_DEF: ProtoDef = ProtoDef {
// 		mem_infos: vec![
// 			MemTypeInfo::new::<T0>(),
// 		],
// 		num_port_putters: 2,
// 		num_port_getters: 1,
// 		rule_defs: vec![
// 			new_rule_def![|_r| true; 0=>2; 1=>3],
// 			new_rule_def![|_r| true; 3=>2],
// 		],
// 	};
// }

struct MyProto<T0>(PhantomData<T0>);
impl<T0: 'static> Proto for MyProto<T0> {

	type Interface = (Putter<T0>, Putter<T0>, Getter<T0>);
	fn proto_def() -> ProtoDef {
		ProtoDef {
			mem_infos: vec![
				MemTypeInfo::new::<T0>(),
			],
			num_port_putters: 2,
			num_port_getters: 1,
			rule_defs: vec![
				new_rule_def![|_r| true; 0=>2; 1=>3],
				new_rule_def![|_r| true; 3=>2],
			],
		}
	}
	fn instantiate() -> Self::Interface {
		let p = Arc::new(ProtoAll::new(&Self::proto_def()));
		(
			Putter {p: p.clone(), id: 0, phantom: Default::default() },
			Putter {p: p.clone(), id: 1, phantom: Default::default() },
			Getter {p: p.clone(), id: 2, phantom: Default::default() },
			// 3 => m1 // putter
			// 4 => m1^ // getter
		)
	}
}

// These are the UNSAFE representations of these
// let rules = vec![
// 	Rule {
// 		guard_ready: bitset!{0,1,2,4},
// 		guard_fn: |_r| true,
// 		fire_fn: |mut _f| {
// 			_f.port_to_locs(0, &[], &[2]);
// 			_f.port_to_locs(1, &[3], &[]);
// 		},
// 	},
// 	Rule { // m3 -> g2
// 		guard_ready: bitset!{2, 3},
// 		guard_fn: |_r| true,
// 		fire_fn: |mut _f| {
// 			_f.mem_to_ports(3, &[2]);
// 		},
// 	},
// ];

#[derive(Debug)]
struct TestDatum(u32);
impl Clone for TestDatum {
	fn clone(&self) -> Self {
		println!("I AM BEING CLONED :3 (contents={})", self.0);
		TestDatum(self.0)
	}
}
impl Drop for TestDatum {
	fn drop(&mut self) {
		println!("I AM BEING DROPPED :O (contents={})", self.0);
	}
}

#[test]
fn test_my_proto() {
	println!("drop is needed? ={:?}", std::mem::needs_drop::<TestDatum>());
	let (mut p1, mut p2, mut g3) = MyProto::instantiate();
	crossbeam::scope(|s| {
		s.spawn(move |_| {
			for i in 0..5 {
				p1.put([i;32]);
			}
		});

		s.spawn(move |_| {
			for i in 0..7 {
				let r = p2.put_timeout_lossy([i;32], Duration::from_millis(1900));
				println!("r={:?}", r);
			}
		});

		s.spawn(move |_| {
			for _ in 0..6 {
				// g3.get_signal(); g3.get_signal();

				let got = g3.get_timeout(Duration::from_millis(2000));
				println!("============================. GOT:{:?}", got);
				// milli_sleep!(3000);
				let got = g3.get_timeout(Duration::from_millis(2000));
				println!("============================. GOT:{:?}", got);
				// milli_sleep!(3000);
			}
		});
	}).expect("WENT OK");
}


// assume for now that the bitset has the right shape.
pub struct ProtoState {
	data: BitSet,
}

trait MaybeClone {
	fn maybe_clone(&self) -> Self; 
}
impl<T> MaybeClone for T {
	default fn maybe_clone(&self) -> Self {
		panic!("type isn't clonable!")
	}
}

impl<T: Clone> MaybeClone for T {
	fn maybe_clone(&self) -> Self {
		self.clone()
	}
}


trait MaybeCopy {
	const IS_COPY: bool; 
}
impl<T> MaybeCopy for T {
	default const IS_COPY: bool = false;
}

impl<T: Copy> MaybeCopy for T {
	const IS_COPY: bool = true;
}