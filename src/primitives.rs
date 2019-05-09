

use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicBool, Ordering};


use hashbrown::HashMap;
use std_semaphore::Semaphore;
// use parking_lot::Mutex;
use std::{fmt};
use std::cell::UnsafeCell;
use itertools::izip;

mod idc {
	// use hashbrown::HashSet;
	// use hashbrown::HashMap;
	// // use parking_lot::Mutex;
	// use std::{mem::{self,ManuallyDrop}, fmt};
	// use std::cell::UnsafeCell;
	// use itertools::izip;


	// #[derive(Debug, Default)]
	// pub struct RaceCell<T: ?Sized> {
	// 	inner: UnsafeCell<T>,
	// }
	// impl<T: ?Sized> RaceCell<T> {
	// 	pub unsafe fn get(&self) -> &mut T {
	// 		&mut *self.inner.get()
	// 	}
	// }

	// unsafe impl<T: ?Sized> Send for RaceCell<T> {}
	// unsafe impl<T: ?Sized> Sync for RaceCell<T> {}

	// #[derive(Debug)]
	// pub struct ManualOption<T: ?Sized> {
	//     pub occupied: bool,
	//     datum: ManuallyDrop<T>,
	// }
	// impl<T: Sized> Default for ManualOption<T> {
	// 	fn default() -> Self {
	// 		Self {
	// 			occupied: false,
	// 			datum: ManuallyDrop::new(unsafe {mem::uninitialized()})
	// 		}
	// 	}
	// }
	// impl<T: Sized> ManualOption<T> {
	// 	pub unsafe fn write(&mut self, datum: T) {
	// 		mem::forget(mem::replace(&mut self.datum as &mut T, datum))
	// 	}
	// 	pub unsafe fn read(&mut self) -> T {
	// 		let mut ret: T = mem::uninitialized();
	// 		let dest = &mut ret as *mut T;
	// 		let src = &self.datum as &T as *const T;
	// 		std::ptr::copy(src, dest, 1);
	// 		ret
	// 	}
	// }
	// impl<T: ?Sized> Drop for ManualOption<T> {
	// 	fn drop(&mut self) {
	// 		if self.occupied {
	// 			unsafe {
	// 				ManuallyDrop::drop(&mut self.datum);
	// 			}
	// 		}
	// 	}
	// }


	// #[test]
	// pub fn store_test() {
	// 	let x: RaceCell<ManualOption<_>> = Default::default();
	//     crossbeam::scope(|s| {
	//     	s.spawn(|_| {
	// 			for i in 0..20 {
	// 				unsafe {
	// 					x.get().write(i);
	// 				}
	// 				milli_sleep![330];
	// 			}
	// 		});
	//     	s.spawn(|_| {
	// 			for _ in 0..20 {
	// 				println!("{:?}", unsafe { x.get().read() });
	// 				milli_sleep![330];
	// 			}
	// 		});
	//     }).expect("EY");
	// }
}

// // return all indices where a >= b > 0 
// struct SatIter<'a,'b> {
// 	a: &'a [u64],
// 	b: &'b [u64],
// 	maj: usize,
// 	min: usize,
// }
// impl<'a,'b> Iterator for SatIter<'a,'b> {
// 	type Item = usize;
// 	fn next(&mut self) -> Option<usize> {
// 		loop {
// 			let chunk = match [self.a.get(self.maj), self.b.get(self.maj)] {
// 				[Some(x), Some(y)] if (a & !b) > 0 => 
// 				[Some(x), Some(y)] => a & b,
// 				_ => return None,
// 			};
// 			if chunk == 0 {
// 				min = 0;
// 				maj += 1;
// 				continue;
// 			}
// 			let mask = 0xff << self.min;
// 			let masked = mask & chunk;

// 		}
// 	}
// }



struct NonNullIter<'a> {
	a: &'a ByteSet,
	maj: usize,
	min: u8, // object invariant: in [0,8)
}
impl<'a> Iterator for NonNullIter<'a> {
	type Item = usize;
	fn next(&mut self) -> Option<usize> {
		while let Some(chunk) = self.a.data.get(self.maj) {
			let mask: u64 = 0xff << self.min;
			if self.min < 7 {
				self.min += 1;
			} else {
				self.min = 0;
				self.maj += 1;
			}
			let was_set = (mask & *chunk) != 0;
			if was_set {
				let current_idx = (self.maj*8) + self.min as usize;
				return Some(current_idx - 1)
			}
		}
		None
	}
}
impl fmt::Debug for ByteSet {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		for &chunk in self.data.iter().rev() {
			write!(f, "{:016x}.", chunk)?;
		}
		Ok(())
	}
}
struct ByteSet {
	// length is object invariant
	data: Vec<u64>,
}
impl ByteSet {
	pub const PUTTY: u8 = 0b00000001;
	pub const GETTY: u8 = 0b00000010;

	pub fn minimize(mut self) -> Self {
		while self.data.last() == Some(&0) {
			self.data.pop();
		}
		self.data.shrink_to_fit();
		self
	}
	pub fn len_bytes(&self) -> usize {
		self.data.len() * 8
	}
	pub fn with_len(len: usize) -> Self {
		let len = if len % 8 == 0 {
			len / 8
		} else {
			(len / 8) + 1
		};
		Self {
			data: std::iter::repeat(0).take(len).collect(),
		}
	}
	pub fn is_superset(&self, other: &Self) -> bool {
		// 1. overlapping bits
		for (&a, &b) in izip!(&self.data, &other.data) {
			if b & !a != 0 {
				return false;
			}
		}
		if other.data.len() > self.data.len() {
			for &b in &other.data[self.data.len()..] {
				if b != 0 {
					return false;
				}
			}
		}
		true
	}
	pub fn atomic_drain_byte(&self, byte_id: usize, extract: u8) -> u8 {
		let slice_u64 = &self.data[..];
		let slice_u8: &[AtomicU8] = unsafe {
			slice_u64.align_to::<AtomicU8>().1
		};
		let cell = slice_u8.get(byte_id).expect("OUT OF BOUNDS");
		cell.fetch_and(!extract, Ordering::Relaxed)
	}
	pub fn get_byte(&self, byte_id: usize) -> u8 {
		let slice_u64 = &self.data[..];
		let slice_u8: &[UnsafeCell<u8>] = unsafe {
			slice_u64.align_to::<UnsafeCell<u8>>().1
		};
		let cell = slice_u8.get(byte_id).expect("OUT OF BOUNDS");
		unsafe {
			*cell.get()
		}
	}
	pub fn set_byte(&self, byte_id: usize, value: u8) {
		let slice_u64 = &self.data[..];
		let slice_u8: &[UnsafeCell<u8>] = unsafe {
			slice_u64.align_to::<UnsafeCell<u8>>().1
		};
		let cell = slice_u8.get(byte_id).expect("OUT OF BOUNDS");
		unsafe {
			*cell.get() = value;
		}
	}
	pub fn iter_non_null(&self) -> impl Iterator<Item=usize> + '_ {
		NonNullIter {
			a: self,
			maj: 0,
			min: 0,
		}
	}
}


impl fmt::Debug for Rule {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{} => [", self.putter)?;
		for &g in self.getters.iter() {
			write!(f, "{}, ", g)?;
		}
		write!(f, "] guard: {:?}", self.guard)
	}
}
struct Rule {
	guard: ByteSet,
	putter: usize,
	getters: Vec<usize>, // sorted, deduplicated
	mem_getters: Vec<usize>, // sorted, deduplicated
	getter_count: usize,
	rule_type: RuleType,
}

impl fmt::Debug for PutterSpace {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		Ok(())
	}
}

unsafe impl Send for MsgDropbox {}
unsafe impl Sync for MsgDropbox {}
struct MsgDropbox {
	sema: Semaphore,
	msg: UnsafeCell<usize>,
}
impl Default for MsgDropbox {
	fn default() -> Self {
		Self {
			sema: Semaphore::new(0),
			msg: 0.into(),
		}
	}
}
impl MsgDropbox {
	fn send(&self, msg: usize) {
		unsafe { *self.msg.get() = msg }
		self.sema.release(); // += 1
	}
	fn recv(&self) -> usize {
		self.sema.acquire(); // -= 1
		unsafe { *self.msg.get() }
	}
}

unsafe impl Send for Ptr {}
unsafe impl Sync for Ptr {}
struct Ptr(UnsafeCell<*const ()>);
impl Default for Ptr {
	fn default() -> Self {
		Self(UnsafeCell::new(std::ptr::null()))
	}
}
impl Ptr {
	fn write<T: Sized>(&self, datum: &T) {
		unsafe {
			*self.0.get() = std::mem::transmute(datum)
		};
	}
	fn read_moving<T: Sized>(&self) -> T {
		unsafe {
			let x: *const T = std::mem::transmute(*self.0.get());
			x.read()
		}
	} 
	fn read_cloning<T: Sized + Clone>(&self) -> T {
		unsafe {
			let x: &T = std::mem::transmute(*self.0.get());
			x.clone()
		}
	} 
}

struct PutterSpace {
	sema: Semaphore,
	not_yet_moved: AtomicBool,
	ptr: Ptr,
}

struct Prot {
	rules: Vec<Rule>,
	ready: ByteSet,
	putter_spaces: HashMap<usize, PutterSpace>,
	msg_dropboxes: HashMap<usize, MsgDropbox>,
}

impl Prot {
	const MAX_RULE_LOOPS: usize = 10;
	pub fn put(&self, id: usize, datum: u32) -> Option<u32> {


		let space = self.putter_spaces.get(&id).expect("NOT A PUTTER?");
		space.ptr.write(&datum);
		space.not_yet_moved.store(true, Ordering::Relaxed);
		self.ready.set_byte(id, ByteSet::PUTTY);

		println!("{:?} entering", id);
		self.enter();
		println!("{:?} back", id);

		let awaiting_number = self.msg_dropboxes.get(&id).expect("WAH").recv();
		for _ in 0..awaiting_number {
			space.sema.acquire(); // -= 1
		}
		let unmoved = space.not_yet_moved.load(Ordering::Relaxed);
		if unmoved {
			Some(datum)
		} else {
			std::mem::forget(datum);
			None
		}
	}

	// pub fn get_signal(&self, id: usize) {
	// 	self.ready.set_byte(id, ByteSet::FULLY);
	// 	self.enter();
	// 	let putter = self.msg_dropboxes.get(&id).expect("WAH").recv();
	// 	let space = self.putter_spaces.get(&putter).expect("NOT A PUTTER?");
	// 	space.sema.release(); // += 1
	// }

	pub fn get(&self, id: usize) -> u32 {
		self.ready.set_byte(id, ByteSet::GETTY);

		println!("{:?} entering", id);
		self.enter();
		println!("{:?} back", id);
		let putter = self.msg_dropboxes.get(&id).expect("WAH").recv();
		let space = self.putter_spaces.get(&putter).expect("NOT A PUTTER?");
		let do_move = space.not_yet_moved.swap(false, Ordering::Relaxed);

		let value = if do_move {
			space.ptr.read_moving()
		} else {
			space.ptr.read_cloning()
		};
		space.sema.release(); // += 1
		value
	}
	fn enter(&self) {
		'reps: loop {
			'rules: for (i, rule) in self.rules.iter().enumerate() {
				if self.ready.is_superset(&rule.guard) {
					println!("RULE {} LOOKS SAT!", i);
					let got = self.ready.atomic_drain_byte(rule.putter, ByteSet::PUTTY);
					//putter reset
					if got & ByteSet::PUTTY != ByteSet::PUTTY {
						println!("nvm. RULE {} NOT locked :(", i);
						continue 'rules;
					}
					self.fire(rule);
					continue 'reps;
				}
			}
			break 'reps;
		}
	}

	fn fire(&self, rule: &Rule) {
		for &getter in rule.getters.iter() {
			self.ready.set_byte(getter, 0x00);
			self.msg_dropboxes.get(&getter).expect("WAH").send(rule.putter);
		}
		self.msg_dropboxes.get(&rule.putter).expect("WAH").send(rule.getter_count);
		// let space = self.putter_spaces.get(&rule.putter).expect("WAH");
		// if !rule.rule_type.move_possible() {
		// 	space.not_yet_moved.swap(false, Ordering::Relaxed);
		// }

		// for &mem_getter in rule.mem_getters.iter() {
		// 	if mem_getter != rule.putter {
		// 		self.ready.set_byte(mem_getter, ByteSet::PUTTY);
		// 	}
		// }
		// for &getter in rule.getters.iter() {
		// 	self.ready.set_byte(getter, 0x00);
		// 	//getter reset
		// 	self.msg_dropboxes.get(&getter).expect("WAH").send(rule.putter);
		// }
		// if rule.rule_type.drains_putter() {
		// 	self.ready.set_byte(mem_getter, ByteSet::GETTY);
		// }
		// if rule.rule_type == RuleType::MoveFromMem {
		// 	// act as mem putter
		// 	let awaiting_number = rule.getter_count;
		// 	for _ in 0..awaiting_number {
		// 		space.sema.acquire(); // -= 1
		// 	}
		// 	let unmoved = space.not_yet_moved.load(Ordering::Relaxed);
		// 	if unmoved {
		// 		Some(datum)s
		// 	} else {
		// 		std::mem::forget(datum);
		// 		None
		// 	}
		// }		
	}
}
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum IdType {
	Pu, Ge, Mem
}
impl IdType {
	fn needs_space(self) -> bool {
		use IdType::*;
		match self {
			Pu | Mem => true,
			Ge => false,
		}
	}
}
struct ProtBuilder {
	max_id_promised: usize,
	max_id_encountered: usize,
	id_type: HashMap<usize, IdType>,
	rules: Vec<Rule>,
}
impl ProtBuilder {
	pub fn new(max_id: usize) -> Self {
		Self {
			max_id_promised: max_id,
			max_id_encountered: 0,
			id_type: Default::default(),
			rules: Default::default(),
		}
	}
	fn type_clash(&mut self, id: usize, id_type: IdType) -> bool {
		*self.id_type.entry(id).or_insert(id_type) != id_type
	}
	pub fn add_rule<I,J>(mut self, is_mem: bool, putter: usize, getters: I, mem_getters: J) -> Self
	where I: IntoIterator<Item=usize>, J: IntoIterator<Item=usize> {
		let mut guard = ByteSet::with_len(self.max_id_promised + 1);


		// getters
		let mut getters_vec: Vec<_> = getters.into_iter().collect();
		getters_vec.sort();
		getters_vec.dedup();
		for &getter in getters_vec.iter() {
			if self.type_clash(getter, IdType::Ge) {
				panic!("Not Ge");
			}
			self.max_id_encountered = self.max_id_encountered.max(getter);
			guard.set_byte(getter, ByteSet::GETTY);
		}

		// mem getters
		let mut mem_getters_vec: Vec<_> = mem_getters.into_iter().collect();
		mem_getters_vec.sort();
		mem_getters_vec.dedup();
		for &getter in mem_getters_vec.iter() {
			if self.type_clash(getter, IdType::Mem) {
				panic!("Not Mem");
			}
			self.max_id_encountered = self.max_id_encountered.max(getter);
			guard.set_byte(getter, ByteSet::GETTY);
		}

		let getter_count = getters_vec.len() + mem_getters_vec.len();

		// putter
		let putter_type = match is_mem {
			true => IdType::Mem,
			false => IdType::Pu,
		};
		if self.type_clash(putter, putter_type) {
			panic!("Not right putter type");
		}
		self.max_id_encountered = self.max_id_encountered.max(putter);
		guard.set_byte(putter, ByteSet::PUTTY);

		guard = guard.minimize();
		let rule_type = if is_mem {
			if mem_getters_vec.contains(&putter) {
				RuleType::CloneFromMem
			} else {
				RuleType::MoveFromMem
			}
		} else {
			RuleType::FromPort
		};
		let r = Rule {
			rule_type,
			mem_getters: mem_getters_vec,
			guard, putter, getters: getters_vec, getter_count,
		};
		self.rules.push(r);
		self
	}
	pub fn build(self) -> Prot {
		Prot {
			putter_spaces: self.id_type.iter()
			.filter(|(_, id_type)| id_type.needs_space())
			.map(|(&id, _)| {
				let space = PutterSpace {
					sema: Semaphore::new(0),
					not_yet_moved: false.into(),
					ptr: Default::default(),
				};
				(id, space)
			}).collect(),
			msg_dropboxes: self.id_type.into_iter().map(|(id,_)|
				(id, MsgDropbox::default())
			).collect(),
			rules: self.rules,
			ready: ByteSet::with_len(self.max_id_encountered),
		}
	}
}


macro_rules! arrit {
	($($el:expr),*) => {{
		[$($el),*].iter().cloned()
	}}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum RuleType {
	CloneFromMem,
	MoveFromMem,
	FromPort,
}
impl RuleType {
	fn drains_putter(self) -> bool {
		!self.move_possible()
	}
	fn move_possible(self) -> bool {
		use RuleType::*;
		match self {
			CloneFromMem => true,
			MoveFromMem | FromPort => false,
		}
	}
	fn mem_putter(self) -> bool {
		use RuleType::*;
		match self {
			CloneFromMem | MoveFromMem=> true,
			FromPort => false,
		}
	}
}

#[test]
pub fn prot_test() {
	let prot = 
	ProtBuilder::new(10)
	.add_rule(false, 0, arrit![1], arrit![])
	.add_rule(false, 0, arrit![2], arrit![])
	.build();

    crossbeam::scope(|s| {
    	s.spawn(|_| {
    		for i in 0..10 {
    			prot.put(0, i);
    			milli_sleep![1000];
    		}
		});
    	s.spawn(|_| {
    		for _ in 0..10 {
    			println!("got {}", prot.get(1));
    			milli_sleep![1000];
    		}
		});
    }).expect("EY");
}

/*
PutterSpace has a PTR
if this space belongs to a PORTPUTTER, the ptr is to the putter's stack
if the space belongs to a MEMPUTTER, the ptr is to a Box<T>

M1 => {M2, M3}

after a transfer, getters have a Ptr to the real datum, not the datum itself*




*/

#[test]
pub fn park_test() {
	let a = std::thread::current();
	let b = std::thread::spawn(move || {
		for _ in 0..10 {
			milli_sleep![1000];
		}
	});
	let now = std::time::Instant::now();
	for _ in 0..3 {
		b.thread().unpark();
	}
	println!("took {:?}", now.elapsed());
	b.join();
}