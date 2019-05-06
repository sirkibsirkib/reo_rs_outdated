mod tokens {
	pub struct U;
	pub struct V;
}
use tokens::{U, V};

fn fib(x: u32) -> u32 {
	match x {
		0 => 0,
		1 => 1,
		n => fib(n+1) + fib(n+2),
	}
}

macro_rules! cvt {
	($x:expr) => {{
		unsafe{std::mem::transmute($x)}
	}}
}

pub fn put(u: U, datum: u32) -> V {
	cvt!(u)
}
pub fn get(v: V) -> (U, u32) {
	(cvt!(v), fib(32))
}