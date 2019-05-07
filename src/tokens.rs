
use hashbrown::HashSet;
use std::sync::Arc;
use std::mem;
use std::marker::PhantomData;
use crate::proto::{Putter, Getter, ProtoCommon, Proto, Id, RuleId};

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

// proto-type specific 
pub struct ProtoHandle<P: Proto> {
	leader: Id,
	common: Arc<ProtoCommon<P>>,
}
impl<P: Proto> ProtoHandle<P> {
	// fn new<I>(proto_lock: &mut ProtoLock<P>, port_ids: HashSet<Id>) -> Self {
	// 	// let common = proto_lock.proto_common.clone();
	// 	// let leader = proto_lock.register_group(port_ids).expect("WAH");
	// 	// Self {
	// 	// 	leader, common,
	// 	// }
	// 	unimplemented!()
	// }
	fn ready_wait_determine<T: Transition<P>>(&self) -> T {
		// let rid: RuleId = self.common.group_ready_wait(self.leader).expect("HUEY");
		// T::new(rid)
		unimplemented!()
	}
}

impl<D:Decimal, T> Safe<D, Getter<T>> {
	pub fn get<R:Token>(&self, coupon: Coupon<D,R>) -> (T,R) {
		let _ = coupon; 
        (self.inner.get(), R::fresh())
    }
}
impl<D:Decimal, T> Safe<D, Putter<T>> {
	pub fn put<R:Token>(&self, coupon: Coupon<D,R>, datum: T) -> R {
		let _ = coupon;
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


pub trait Transition<P: Proto>: Sized {
	fn new(proto_rule_id: RuleId) -> Self;
} 

pub trait Advance<P: Proto>: Sized {
    type Opts: Transition<P>;
    fn advance<F, R>(self, p_handle: ProtoHandle<P>, handler: F) -> R
    where
        F: FnOnce(Self::Opts) -> R,
    {
    	let choice: Self::Opts = match mem::size_of::<Self::Opts>() {
    		0 => unsafe { mem::uninitialized() },
    		_ => p_handle.ready_wait_determine(),
    	};
    	handler(choice)
    }
}
