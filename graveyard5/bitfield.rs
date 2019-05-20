


////////// DEBUG DEBUG
#![allow(dead_code)]

use std::{fmt};


pub trait Fieldlike: Sized {
    fn cap(&self) -> usize;
    fn chunk(&self, id: usize) -> u64;
    fn true_iter(&self) -> TrueIter<Self> {
    	let first = if self.cap() > 0 {
    		Some(self.chunk(0))
    	} else {
    		None
    	};
    	TrueIter {
    		field: self,
    		chunk: first,
    		maj: 0,
    		min: 0,
    	}
    }
}


pub struct RepField(u64);
impl Fieldlike for RepField {
    fn cap(&self) -> usize {
    	if self.0 == 0 {
    		0
    	} else {
    		std::usize::MAX
    	}
    }
    fn chunk(&self, _id: usize) -> u64 {self.0}
}

pub struct Field {
    cap: usize,
    data: Vec<u64>,
}
impl Field {
	fn from<I>(cap: usize, it: I) -> Self where I: IntoIterator<Item=usize> {
		let mut me = Self::new(cap);
		for i in it.into_iter() {
			let _ = me.set_to(i, true);
		}
		me
	}
	fn set_to(&mut self, idx: usize, val: bool) -> Result<(),()> {
		if idx >= self.cap {
			return Err(())
		}
		let maj = idx / 64;
		let mask = 1 << (idx%64);
		let chunk = unsafe {
			self.data.get_unchecked_mut(maj)
		};
		if val {
			*chunk |= mask
		} else {
			*chunk &= !mask
		}
		Ok(())
	}
	fn new(cap: usize) -> Self {
		let chunks = (cap / 64) + if cap%64==0 {0} else {1};
		Self {
			data: (0..chunks).map(|_| 0x0).collect(),
			cap,
		}
	}
}

pub struct TrueIter<'a, A: Fieldlike> {
	field: &'a A,
	chunk: Option<u64>,
	maj: usize,
	min: usize,
}
impl<'a, A: Fieldlike> Iterator for TrueIter<'a,A> {
	type Item = usize;
	fn next(&mut self) -> Option<Self::Item> {
		loop {
			let chunk = match self.chunk {
				Some(chunk) => chunk,
				None => {
					let idx = self.maj*64 + self.min;
					if idx >= self.field.cap() {
						return None
					}
					self.field.chunk(self.maj)
				},
			};
			let mask = 1 << self.min;
			if chunk & mask > 0 {
				return Some(self.maj*64 + self.min)
			}
			if self.min == 64 {
				self.min = 0;
				self.maj += 1;
			}
		}
	}
}
impl fmt::Debug for Field {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		do_debug(self, f)
	}
}

fn do_debug<F: Fieldlike>(fieldlike: &F, f: &mut fmt::Formatter) -> fmt::Result {
	let cap = fieldlike.cap();
	let chunks = (cap / 64) + if cap%64==0 {0} else {1};
	for i in 0..chunks {
		write!(f, "{:064b} ", fieldlike.chunk(i))?;
	}
	Ok(())
}


impl Fieldlike for Field {
    fn cap(&self) -> usize { self.cap }
    fn chunk(&self, id: usize) -> u64 { self.data[id] }
}

pub struct Or<'a, 'b, A: Fieldlike, B: Fieldlike>(&'a A, &'b B);
impl<'a, 'b, A: Fieldlike, B: Fieldlike> Fieldlike for Or<'a, 'b, A, B> {
    fn cap(&self) -> usize { self.0.cap().max(self.1.cap()) }
    fn chunk(&self, id: usize) -> u64 {
    	let [c0, c1] = [self.0.cap(), self.1.cap()];
    	let ch0 = (c0 / 64) + if c0%64==0 {0} else {1};
    	let ch1 = (c1 / 64) + if c1%64==0 {0} else {1};
    	match [ch0 > id, ch1 > 1] {
    		[false, false] => panic!("OUT OF BOUNDS"),
    		[true, false] => self.0.chunk(id),
    		[false, true] => self.1.chunk(id),
    		[true, true] => self.0.chunk(id) & self.1.chunk(id),
    	}
    }
}
impl<'a, 'b, A: Fieldlike, B: Fieldlike> fmt::Debug for Or<'a, 'b, A, B> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		do_debug(self, f)
	}
}

pub struct And<'a, 'b, A: Fieldlike, B: Fieldlike>(&'a A, &'b B);
impl<'a, 'b, A: Fieldlike, B: Fieldlike> Fieldlike for And<'a, 'b, A, B> {
    fn cap(&self) -> usize { self.0.cap().min(self.1.cap()) }
    fn chunk(&self, id: usize) -> u64 { self.0.chunk(id) & self.1.chunk(id) }
}
impl<'a, 'b, A: Fieldlike, B: Fieldlike> fmt::Debug for And<'a, 'b, A, B> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		do_debug(self, f)
	}
}

pub struct Not<'a, A: Fieldlike>(&'a A);
impl<'a, A: Fieldlike> Fieldlike for Not<'a, A> {
    fn cap(&self) -> usize { self.0.cap() }
    fn chunk(&self, id: usize) -> u64 { !self.0.chunk(id) }
}
impl<'a, A: Fieldlike> fmt::Debug for Not<'a, A> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		do_debug(self, f)
	}
}


#[test]
fn fieldy() {
	let a = Field::from(10, [0,4, 23].iter().cloned());
	let b = Field::from(10, [1,2].iter().cloned());
	let c = RepField(0b11111111110);
	println!("{:?}", And(&Or(&a,&b), &c));
}
