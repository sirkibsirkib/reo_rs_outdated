
use crate::proto::PortCommon;
use std::sync::Arc;
use std::mem;
use crate::proto::{Proto, Id};
use std::marker::PhantomData;
use crate::proto::{Putter, Getter};

pub trait Decimal: Token + Default {}
pub trait Token: Sized {}
trait NoData: Token {
	fn fresh() -> Self;
}

#[derive(Default)]
pub struct D0<T>(PhantomData<T>);
#[derive(Default)]
pub struct D1<T>(PhantomData<T>);
#[derive(Default)]
pub struct D2<T>(PhantomData<T>);
#[derive(Default)]
pub struct D3<T>(PhantomData<T>);
#[derive(Default)]
pub struct D4<T>(PhantomData<T>);
#[derive(Default)]
pub struct D5<T>(PhantomData<T>);
#[derive(Default)]
pub struct D6<T>(PhantomData<T>);
#[derive(Default)]
pub struct D7<T>(PhantomData<T>);
#[derive(Default)]
pub struct D8<T>(PhantomData<T>);
#[derive(Default)]
pub struct D9<T>(PhantomData<T>);

// convenience to make the last digit less ugly
pub type E0 = D0<()>;
pub type E1 = D1<()>;
pub type E2 = D2<()>;
pub type E3 = D3<()>;
pub type E4 = D4<()>;
pub type E5 = D5<()>;
pub type E6 = D6<()>;
pub type E7 = D7<()>;
pub type E8 = D8<()>;
pub type E9 = D9<()>;

pub struct Safe<D:Decimal, T> {
	port_ids: Arc<Vec<Id>>,
	inner: T,
	d: D,
}
impl<D:Decimal, T> Safe<D,T> {
	pub fn new(inner: T, port_ids: Arc<Vec<Id>>) -> Self {
		Self {
			port_ids,
			inner,
			d: Default::default(),
		}
	}
}

pub trait ProtoCommunicator<P: Proto> {
	// assumes: 
	// 1. output.len() >= 1
	// 2. output[0] is always the same
	fn id_iter(&self) -> &[Id];
	fn get_commo(&self) -> &PortCommon<P>;
	fn register_group(&mut self) {
		/* panics if:
		1. ports overlap with already-registered
		2. proto not in initial state
		*/
		unimplemented!()
	}
	fn ready_and_wait(&mut self) -> usize {
		// 1. acquire group id
		// 2. send proto a message that entire group is ready
		// 3. wait for proto response
		// 4. interpret the proto's response
		unimplemented!()
	}
}

impl<D:Decimal, T> Safe<D, Getter<T>> {
	pub fn get<R:Token>(&self, coupon: Coupon<D,R>) -> (T,R) {
        (self.inner.get(), R::fresh())
    }
}
impl<D:Decimal, T> Safe<D, Putter<T>> {
	pub fn put<R:Token>(&self, coupon: Coupon<D,R>, datum: T) -> R {
		self.inner.put(datum);
		R::fresh()
    }
}


pub struct Coupon<D: Decimal, R: Token> {
	phantom: PhantomData<(D,R)>,
}

impl<T: Token> NoData for T {
	fn fresh() -> Self {
		debug_assert!(mem::size_of::<Self>() == 0);
		unsafe {
			mem::uninitialized()
		}
	}
}

pub struct T;
pub struct F;
pub struct X;
pub trait Tern: Token {}
impl Token for T {}
impl Token for F {}
impl Token for X {}

pub struct Neg<T: Var> {
    phantom: PhantomData<T>,
}
impl<T: Var> Token for Neg<T> {}

pub trait Nand {}
impl Nand for F {}
impl Nand for Neg<T> {}

// only left is NAND
impl<A:Nand> Nand for (A, T)      {}
impl<A:Nand> Nand for (A, Neg<F>) {}
impl<A:Nand> Nand for (A, X)      {}
impl<A:Nand> Nand for (A, Neg<X>) {}
// only right is NAND
impl<A:Nand> Nand for (T,      A) {}
impl<A:Nand> Nand for (Neg<F>, A) {}
impl<A:Nand> Nand for (X,      A) {}
impl<A:Nand> Nand for (Neg<X>, A) {}
// both sides are NAND
impl<A:Nand, B:Nand> Nand for (A, B) {}


pub trait Var {}
impl Var for T {}
impl Var for F {}
impl Var for X {}


pub trait Transition: Sized {
	fn new(proto_rule_id: usize) -> Self;
} 

pub trait Advance<P: Proto>: Sized {
    type Opts: Transition;
    fn advance<C, F, R>(self, communicator: C, handler: F) -> R
    where
    	C: ProtoCommunicator<P>,
        F: FnOnce(Self::Opts) -> R,
    {
    	let choice: Self::Opts = match mem::size_of::<Self::Opts>() {
    		0 => unsafe { mem::uninitialized() },
    		_ => {
    			unimplemented!()
    		},
    	};
    	handler(choice)
    }
}
