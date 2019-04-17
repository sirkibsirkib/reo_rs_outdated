
// ignore. used for statically specializing types (here, Port)
use std::marker::PhantomData;


pub trait Coupon {}

struct Port<T, C: Coupon> {
	phantom: PhantomData<(T,C)>,
}
impl<T, C: Coupon> Port<T,C> {
	pub fn new() -> Self {
		Self {
			phantom: PhantomData::default(),
		}
	}
	pub fn put(&mut self, datum: T, coupon: C) -> Done {
		//println!("put {:?} with coupon {:?}", datum, coupon);
		Done{ core: Core }
	} 
}


pub enum Uopts {
	A(CouA),
	None(Done),
}
pub enum Vopts {
	B(CouB),
}

pub struct CouA(Core); impl Coupon for CouA {}
pub struct CouB(Core); impl Coupon for CouB {}

pub struct U(Core);
pub struct V(Core);



struct Core;
pub struct Done {
	core: Core,
}
pub struct Env {
	port_a: Port<u32, CouA>,
	port_b: Port<u32, CouB>,
}
impl Env {
	fn advance_u(&mut self, u: U, handler: impl FnOnce(&mut Self, Uopts) -> Done) -> V {
		let _ = u;
		let opts = Uopts::A(CouA(Core));
		let _done = handler(self, opts);
		V(Core)
	}
	fn advance_v(&mut self, v: V, handler: impl FnOnce(&mut Self, Vopts) -> Done) -> U {
		let _ = v;
		let opts = Vopts::B(CouB(Core));
		let _done = handler(self, opts);
		U(Core)
	}
}

pub fn main() {
	let mut e = Env {
		port_a: Port::new(),
		port_b: Port::new(),
	};
	let mut u = U(Core);
	for _ in 0..4 {
		let v = e.advance_u(u, |e, opts| {
			match opts {
				Uopts::A(coup_a) => e.port_a.put(5, coup_a),
				Uopts::None(done) => done,
			}
		});
		u = e.advance_v(v, |e, opts| {
			match opts {
				Vopts::B(coup_b) => e.port_b.put(3, coup_b),
			}
		});
	}
}