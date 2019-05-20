
////////// DEBUG DEBUG
#![allow(dead_code)]

use core::ops::Range;
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

// contains all the information a memcell needs such that it can erase the type
#[derive(Debug, Clone, Copy)]
struct MemTypeInfo {
	type_id: TypeId,
	drop_fn: DropFnPtr,
	clone_fn: CloneFnPtr,
	bytes: usize,
	align: usize,
}
impl MemTypeInfo {
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
			bytes: std::mem::size_of::<T>(),
			type_id: TypeId::of::<T>(),
			drop_fn,
			clone_fn,
			align: std::mem::align_of::<T>(),
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
	fn make_empty(&self, my_id: PortId, r: &ProtoR, w: &mut ProtoActive, do_drop: bool) {
		// println!("::make_empty| id={} do_drop={}", my_id, do_drop);
		let ptr = self.get_ptr();
		let src_refs = w.mem_refs.get_mut(&ptr).expect("UNKNWN");
		let tid = &self.type_info.type_id;
		*src_refs -= 1;
		if *src_refs == 0 {
			// contents need to be dropped! ptr needs to be made free
			w.mem_refs.remove(&ptr).expect("hhh");
			if do_drop {
				(self.type_info.drop_fn)(ptr);
			}
			w.ready.set(r.mem_getter_id(my_id)); // GETTER ready
			w.free_mems.get_mut(tid).expect("??").push(ptr);
		}
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
	move_intention: UnsafeCell<bool>,
}
impl PoGeSpace {
	fn new() -> Self {
		Self {
			dropbox: MsgDropbox::new(),
			move_intention: false.into(),
		}
	}
	fn set_move_intention(&self, move_intention: bool) {
		unsafe {
			*self.move_intention.get() = move_intention
		}
	}
	fn get_move_intention(&self) -> bool {
		unsafe {
			*self.move_intention.get()
		}
	}
	fn get_signal(&self, a: &ProtoAll) {
		// println!("::get_sig| (start)");
		let msg = self.dropbox.recv();
		let putter_id: PortId = msg & (!FLAG_YOUR_MOVE) & (!FLAG_OTH_EXIST);
		let i_move = (msg & FLAG_YOUR_MOVE) > 0;
		let conflict = msg & FLAG_OTH_EXIST > 0;

		// it's possible I am assigned "move duty" even if I didn't want it.
		// move then means "drop"
		debug_assert!(i_move || conflict); 
		match a.r.get_space(putter_id) {
			SpaceRef::Memo(memo_space) => {
				if i_move {
					if conflict {
						// 1. must wait for cloners to finish
						memo_space.mover_sema.acquire();
					}
					{
						let mut w = a.w.lock();
						// 2. release putter. DO DROP
						memo_space.make_empty(putter_id, &a.r, &mut w.active, true);
						// 3. notify state waiters
						let ProtoW { ref active, ref mut awaiting_states, .. } = &mut w as &mut ProtoW;
						ProtoW::notify_state_waiters(&active.ready, awaiting_states, &a.r);
					}
				} else {
					let was = memo_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						memo_space.mover_sema.release();
					}
				}
			},
			SpaceRef::PoPu(po_pu_space) => {
				if i_move {
					if conflict {
						// must wait for cloners to finish
						po_pu_space.mover_sema.acquire();
					}
					// 3. release putter. let them know NOBODY moved (do drop)
					po_pu_space.dropbox.send(0);
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
		// println!("::get| start");
		let msg = self.dropbox.recv();
		let putter_id: PortId = msg & (!FLAG_YOUR_MOVE) & (!FLAG_OTH_EXIST);
		let i_move = (msg & FLAG_YOUR_MOVE) > 0;
		let conflict = msg & FLAG_OTH_EXIST > 0;

		// println!("::get| putter_id={} i_move={} conflict={}", putter_id, i_move, conflict);

		// I requested move, so if it was denied SOMEONE must move
		debug_assert!(i_move || conflict); 
		match a.r.get_space(putter_id) {
			SpaceRef::Memo(memo_space) => {
				// println!("::get| mepu branch");
				let ptr: &T = unsafe {
					transmute(memo_space.get_ptr())
				};
				if i_move {
					if conflict {
						// must wait for cloners to finish
						memo_space.mover_sema.acquire();
					}
					let datum = unsafe {
						std::ptr::read(ptr)
					};
					// 3. release putter. DON'T DROP
					{
						let mut w = a.w.lock();
						// 2. release putter. DO NOT drop
						memo_space.make_empty(putter_id, &a.r, &mut w.active, false);
						// 3. notify state waiters
						let ProtoW { ref active, ref mut awaiting_states, .. } = &mut w as &mut ProtoW;
						ProtoW::notify_state_waiters(&active.ready, awaiting_states, &a.r);
					}
					datum
				} else {
					let datum = T::clone_fn(ptr);
					let was = memo_space.cloner_countdown.fetch_sub(1, Ordering::SeqCst);
					if was == 1 {
						// I was last! release mover (who MUST exist)
						memo_space.mover_sema.release();
					}
					datum
				}
			},
			SpaceRef::PoPu(po_pu_space) => {
				// println!("::get| popu branch");
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
					// 3. release putter. let them know someone DID move (don't drop)
					// println!("::get| releasing popu");
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
	whom: PortId,
}
struct ProtoW {
	rules: Vec<Rule>,
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
					_ => panic!("bad state-waiter PortId!"),
				};
				false
			} else {
				true
			};
			retain
		})
	}
	fn enter(&mut self, r: &ProtoR, my_id: PortId) {
		self.active.ready.set(my_id);
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
	fn enter_committed(&mut self, r: &ProtoR, tent_it: PortId, expecting_rule: usize) {
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



struct ProtoR {
	mem_data: Vec<u8>,
	po_pu: Vec<PoPuSpace>, // id range 0..#PoPu
 	po_ge: Vec<PoGeSpace>, // id range #PoPu..(#PoPu + #PoGe)
	me_pu: Vec<MemoSpace>, // id range (#PoPu + #PoGe)..(#PoPu + #PoGe + #Memo)
 	// me_ge doesn't need a space
 	// mem_type_info: HashMap<TypeId, MemTypeInfo>,
}
impl ProtoR {
	fn send_to_getter(&self, id: PortId, msg: usize) {
		self.get_po_ge(id).expect("NOPOGE").dropbox.send(msg)
	}
	#[inline]
	fn mem_getter_id(&self, id: PortId) -> PortId {
		id + self.me_pu.len()
	}
	fn get_po_pu(&self, id: PortId) -> Option<&PoPuSpace> {
		self.po_pu.get(id)
	}
	fn get_po_ge(&self, id: PortId) -> Option<&PoGeSpace> {
		self.po_ge.get(id - self.po_pu.len())
	}
	fn get_me_pu(&self, id: PortId) -> Option<&MemoSpace> {
		self.me_pu.get(id - self.po_pu.len() - self.po_ge.len())
	}
	fn id_is_port(&self, id: PortId) -> bool {
		id < (self.po_pu.len() + self.po_ge.len())
	}
	fn get_space(&self, id: PortId) -> SpaceRef {
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
	fn recv(&self) -> usize {
		self.r.recv().unwrap()
	}
	#[inline]
	fn send(&self, msg: usize) {
		self.s.try_send(msg).expect("Msgbox was full!")
	}
	fn send_nothing(&self) {
		self.send(!0)
	}
	fn recv_nothing(&self) {
		debug_assert_eq!(self.recv(), !0);
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

// [Memo | PoPu | PoGe | MeGe]
struct ProtoAll {
	r: ProtoR,
	w: Mutex<ProtoW>,
}
impl ProtoAll {
	fn new(mem_infos: Vec<MemTypeInfo>, num_port_putters: usize, num_port_getters: usize, rules: Vec<Rule>) -> Self {
		let mem_get_id_start = mem_infos.len() + num_port_putters + num_port_getters;
		// let po_pu_rng = 0..num_port_putters;
		// let po_ge_rng = num_port_putters..(num_port_putters + num_port_getters);
		// let end = num_port_putters + num_port_getters + mem_infos.len();
		// let mem_rng = (num_port_putters + num_port_getters)..end;
		// let rules = abstract_actions.iter().map(|action_list| {
		// 	prepare_rule(action_list, po_pu_rng.clone(), po_ge_rng.clone(), mem_rng.clone()).expect("BAD ACTION")
		// }).collect();

		let (mem_data, me_pu, free_mems, ready) = Self::build_buffer(mem_infos, mem_get_id_start);
		let po_pu = (0..num_port_putters).map(|_| PoPuSpace::new()).collect();
		let po_ge = (0..num_port_getters).map(|_| PoGeSpace::new()).collect();
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
	fn build_buffer(infos: Vec<MemTypeInfo>, mem_get_id_start: usize) ->
	(
		Vec<u8>, // buffer
		Vec<MemoSpace>,
		HashMap<TypeId, Vec<*mut u8>>,
		BitSet,
	) {
		let mut capacity = 0;
		let mut offsets_n_typeids = vec![];
		let mut mem_type_info: HashMap<TypeId, Arc<MemTypeInfo>> = HashMap::default();
		let mut ready = BitSet::default();
		let mut free_mems = HashMap::default();
		for (mem_id, info) in infos.into_iter().enumerate() {
			ready.set(mem_id + mem_get_id_start); // set GETTER
			let rem = capacity % info.align.max(1);
			if rem > 0 {
				capacity += info.align - rem;
			}
			// println!("@ {:?} for info {:?}", capacity, &info);
			offsets_n_typeids.push((capacity, info.type_id));
			mem_type_info.entry(info.type_id).or_insert_with(|| Arc::new(info));
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

		// 1. make ready my datum
		unsafe {
			po_pu.set_ptr(transmute(&datum));	
		}

		// 2. enter, participate in protocol
		self.p.w.lock().enter(&self.p.r, self.id);

		// 3. wait for my value to be consumed
		let num_movers_msg = po_pu.dropbox.recv();
		println!("putter got msg={}", num_movers_msg);
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


pub struct Firer<'a> {
	r: &'a ProtoR,
	w: &'a mut ProtoActive,
}
impl<'a> Firer<'a> {
	pub fn mem_to_nowhere(&mut self, me_pu: PortId) {
		let memo_space = self.r.get_me_pu(me_pu).expect("fewh");
		memo_space.make_empty(me_pu, self.r, self.w, true);
	}

	pub fn mem_to_ports(&mut self, me_pu: PortId, po_ge: &[PortId]) {
		let memo_space = self.r.get_me_pu(me_pu).expect("fewh");

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
				memo_space.cloner_countdown.store(l-1, Ordering::SeqCst);
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
					memo_space.make_empty(me_pu, self.r, self.w, true);
				} else {
					memo_space.cloner_countdown.store(l, Ordering::SeqCst);
					for g in po_ge.iter().cloned() {
						self.r.send_to_getter(g, me_pu);
					}
				}
			},
		}
	}

	pub fn mem_to_mem_and_ports(&mut self, me_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
		// println!("mem_to_mem_and_ports");
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


	fn find_mover(&self, getters: &[PortId]) -> Option<PortId> {
		getters.iter().filter(|id| {
			let po_ge = self.r.get_po_ge(**id).expect("bad id");
			po_ge.get_move_intention()
		}).next().or(getters.get(0)).cloned()
	}

	pub fn port_to_mem_and_ports(&mut self, po_pu: PortId, me_ge: &[PortId], po_ge: &[PortId]) {
		// println!("port_to_mem_and_ports");

		debug_assert!(po_pu < FLAG_OTH_EXIST && po_pu < FLAG_YOUR_MOVE);
		let po_pu_space = self.r.get_po_pu(po_pu).expect("ECH");

		// 1. port getters have move-priority
		let port_mover_id = self.find_mover(po_ge);
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
			if port_mover_id.is_some() {
				// mem clone!
				// println!("::port_to_mem_and_ports| mem clone");
				(info.clone_fn)(src, dest);
			} else {
				// mem move!
				// println!("::port_to_mem_and_ports| mem move");
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
		match (port_mover_id, po_ge.len()) {
			(Some(m), 1) => {
				// println!("::port_to_mem_and_ports| MOV={},L={}", m, 1);
				// ONLY mover case. mover will wake putter
				self.r.send_to_getter(m, po_pu | FLAG_YOUR_MOVE);
			},
			(Some(m), l) => {
				// println!("::port_to_mem_and_ports| MOV={},L={}", m, l);
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
				// println!("::port_to_mem_and_ports| MOV=.,L={}", l);
				// no movers
				if l == 0 {
					// no cloners either
					let mem_cloners = if me_ge.is_empty() {0} else {1};
					po_pu_space.dropbox.send(mem_cloners);
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


struct Rule {
	guard_ready: BitSet,
	guard_fn: fn(&ProtoR) -> bool,
	fire_fn: fn(Firer),
}
impl Rule {
	fn fire(&self, f: Firer) {
		(self.fire_fn)(f)
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

fn in_rng(x: &Range<usize>, y: usize) -> bool {
	x.start <= y && y < x.end
}


struct MyProto<T: PortData>(PhantomData<T>);
impl<T: 'static +  PortData> Proto for MyProto<T> {
	type Interface = (Putter<T>, Putter<T>, Getter<T>);
	fn instantiate() -> Self::Interface {
		let num_port_putters = 2; // 0..=1
		let num_port_getters = 1; // 2..=2
		let mem_infos = vec![ // 3..=3  (3..=4 bits)
			MemTypeInfo::new::<T>(),
		];
		let rules = vec![
			Rule {
				guard_ready: bitset!{0,1,2,4},
				guard_fn: |_r| true,
				fire_fn: |mut _f| {
					_f.port_to_mem_and_ports(0, &[], &[2]);
					_f.port_to_mem_and_ports(1, &[3], &[]);
				},
			},
			Rule { // m3 -> g2
				guard_ready: bitset!{2, 3},
				guard_fn: |_r| true,
				fire_fn: |mut _f| {
					_f.mem_to_ports(3, &[2]);
				},
			},
		];

		let p = Arc::new(ProtoAll::new(mem_infos, num_port_putters, num_port_getters, rules));
		(
			Putter {p: p.clone(), id: 0, phantom: Default::default() },
			Putter {p: p.clone(), id: 1, phantom: Default::default() },
			Getter {p: p.clone(), id: 2, phantom: Default::default() },
			// 3 => m1 // putter
			// 4 => m1^ // getter
		)
	}
}


// let abstract_rules: &[&[AbstractAction]] = &[
// 	&[ // rule 0: {p0 -> g2, p1 -> m3}
// 		AbstractAction::new(0, &[2], &[]),
// 		AbstractAction::new(1, &[3], &[]),
// 	],
// 	&[ // rule 1: {m3 -> g2}
// 		AbstractAction::new(3, &[], &[2]),
// 	],
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
	let (mut p1, mut p2, mut g3) = MyProto::instantiate();
	crossbeam::scope(|s| {
		s.spawn(move |_| {
			for i in 0..5 {
				p1.put(Box::new(TestDatum(i)));
			}
		});

		s.spawn(move |_| {
			for i in 0..5 {
				p2.put(Box::new(TestDatum(i + 10)));
			}
		});

		s.spawn(move |_| {
			for _ in 0..5 {
				println!("GOT {:?} | {:?}", g3.get(), g3.get());
				// milli_sleep!(2000);
			}
		});
	}).expect("WENT OK");
}


// assume for now that the bitset has the right shape.
pub struct ProtoState {
	data: BitSet,
}