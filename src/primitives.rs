

use std::sync::atomic::{AtomicBool, Ordering};
use crossbeam::{Sender, Receiver};
use hashbrown::HashSet;
use hashbrown::HashMap;
use std_semaphore::Semaphore;
// use parking_lot::Mutex;
use std::{mem, fmt};
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
	pub const READY: u8 = 0b00000001;
	pub const UNBLOCKED: u8 = 0b00000010;
	pub const MOVED: u8 = 0b00000100;

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
		Self::from(len, std::iter::empty())
	}
	pub fn from<I>(len: usize, it: I) -> Self where I: IntoIterator<Item=usize> {
		let len = if len % 8 == 0 {
			len / 8
		} else {
			(len / 8) + 1
		};
		let me = Self {
			data: std::iter::repeat(0).take(len).collect(),
		};
		for i in it {
			me.set_byte(i, Self::READY);
		}
		me
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
	getter_count: usize,
}

impl fmt::Debug for PutterSpace {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		Ok(())
	}
}
struct PutterSpace {
	sema: Semaphore,
	not_yet_moved: AtomicBool,
	ptr: *const (),
}

struct Prot {
	rules: Vec<Rule>,
	ready: ByteSet,
	putter_spaces: HashMap<usize, PutterSpace>,
	msgs: HashMap<usize, (Semaphore, UnsafeCell<usize>)>,
}
impl Prot {
	fn send_msg(&self, id: usize, msg: usize) {
		let v = self.msgs.get(&id).expect("EE");
		let v1 = v.1.get();
		unsafe { *v1 = msg };
		v.0.release() // += 1
	}
	fn get_msg(&self, id: usize) -> usize {
		let v = self.msgs.get(&id).expect("QQ");
		v.0.acquire(); // -= 1
		let v1 = v.1.get();
		unsafe { *v1 }
	}
	pub fn put(&self, id: usize, datum: u32) -> Option<u32> {
		self.ready.set_byte(id, ByteSet::READY);

		let space = self.putter_spaces.get(&id).expect("NOT A PUTTER?");
		space.not_yet_moved.store(true, Ordering::Relaxed);

		self.enter();

		let awaiting_number = self.get_msg(id);
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

	pub fn get_signal(&self, id: usize) {
		self.ready.set_byte(id, ByteSet::READY);
		self.enter();
		let putter = self.get_msg(id);
		let space = self.putter_spaces.get(&putter).expect("NOT A PUTTER?");
		space.sema.release(); // += 1
	}

	pub fn get(&self, id: usize) -> u32 {
		self.ready.set_byte(id, ByteSet::READY);

		self.enter();
		let putter = self.get_msg(id);
		let space = self.putter_spaces.get(&putter).expect("NOT A PUTTER?");
		let do_move = space.not_yet_moved.swap(false, Ordering::Relaxed);

		let value = if do_move {
			5
		} else {
			5.clone()
		};
		space.sema.release(); // += 1
		value
	}
	fn enter(&self) {
		loop {
			for (i, r) in self.rules.iter().enumerate() {
				if self.ready.is_superset(&r.guard) {
					println!("RULE {} SAT!", i);
					for &getter in r.getters.iter() {
						self.send_msg(getter, r.putter);
					}
					self.send_msg(r.putter, r.getter_count);
				}
			}
		}
	}
}
struct ProtBuilder {
	max_id_promised: usize,
	max_id_encountered: usize,
	is_putter: HashMap<usize, bool>,
	rules: Vec<Rule>,
}
impl ProtBuilder {
	pub fn new(max_id: usize) -> Self {
		Self {
			max_id_promised: max_id,
			max_id_encountered: 0,
			is_putter: Default::default(),
			rules: Default::default(),
		}
	}
	pub fn add_rule<I>(mut self, putter: usize, getters: I) -> Self
	where I: IntoIterator<Item=usize> {
		let mut guard = ByteSet::with_len(self.max_id_promised + 1);

		// getters
		let mut getters_vec: Vec<_> = getters.into_iter().collect();
		getters_vec.sort();
		getters_vec.dedup();
		for &getter in getters_vec.iter() {
			if *self.is_putter.entry(getter).or_insert(false) {
				panic!("WAS PUTTER BEFORE");
			}
			self.max_id_encountered = self.max_id_encountered.max(getter);
			guard.set_byte(getter, ByteSet::READY | ByteSet::UNBLOCKED);
		}
		let getter_count = getters_vec.len();

		// putter
		if ! *self.is_putter.entry(putter).or_insert(true) {
			panic!("WAS GETTER BEFORE");
		}
		self.max_id_encountered = self.max_id_encountered.max(putter);
		guard.set_byte(putter, ByteSet::READY | ByteSet::UNBLOCKED);

		guard = guard.minimize();
		let r = Rule {
			guard, putter, getters: getters_vec, getter_count,
		};
		self.rules.push(r);
		self
	}
	pub fn build(self) -> Prot {
		Prot {
			putter_spaces: self.is_putter.iter()
			.filter(|(_, p)| **p)
			.map(|(&id, _)| {
				let space = PutterSpace {
					sema: Semaphore::new(0),
					not_yet_moved: false.into(),
					ptr: std::ptr::null(),
				};
				(id, space)
			}).collect(),
			msgs: self.is_putter.into_iter().map(|(id,_)|
				(id, (Semaphore::new(0), 0.into()))
			).collect(),
			rules: self.rules,
			ready: ByteSet::with_len(self.max_id_encountered),
		}
	}
}

#[test]
pub fn prot_test() {
	let prot = 
	ProtBuilder::new(10)
	.add_rule(0, [1].iter().cloned())
	.add_rule(0, [2].iter().cloned())
	.build();

  //   crossbeam::scope(|s| {
  //   	s.spawn(|_| {
		// 	for i in 0..20 {
		// 		unsafe {
		// 			x.get().write(i);
		// 		}
		// 		milli_sleep![330];
		// 	}
		// });
  //   	s.spawn(|_| {
		// 	for _ in 0..20 {
		// 		println!("{:?}", unsafe { x.get().read() });
		// 		milli_sleep![330];
		// 	}
		// });
  //   }).expect("EY");
}