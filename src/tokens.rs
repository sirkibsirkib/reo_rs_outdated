use crate::proto::{Getter, PortGroup, PortId, Proto, Putter, RuleId, TryClone};
use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;

pub trait Decimal: Token {}
pub trait Token: Sized {} 
impl Token for () {}
trait NoData: Token {
    fn fresh() -> Self;
}

macro_rules! def_decimal {
    ($d:tt, $e:tt) => {
        pub struct $d<T>(PhantomData<T>);
        impl<T: Token> Decimal for $d<T> {}
        impl<T: Token> Token for $d<T> {}
        pub type $e = $d<()>;
    }
}
def_decimal![D0, E0];
def_decimal![D1, E1];
def_decimal![D2, E2];
def_decimal![D3, E3];
def_decimal![D4, E4];
def_decimal![D5, E5];
def_decimal![D6, E6];
def_decimal![D7, E7];
def_decimal![D8, E8];
def_decimal![D9, E9];

pub struct Safe<D: Decimal, T> {
    port_ids: Arc<Vec<PortId>>,
    inner: T,
    d: D,
}
impl<D: Decimal, T> Safe<D, T> {
    pub fn new(inner: T, port_ids: Arc<Vec<PortId>>) -> Self {
        Self {
            port_ids,
            inner,
            d: Default::default(),
        }
    }
}

impl<D: Decimal, T: TryClone, P: Proto> Safe<D, Getter<T, P>> {
    pub fn get<R: Token>(&self, coupon: Coupon<D, R>) -> (T, R) {
        let _ = coupon;
        (self.inner.get(), R::fresh())
    }
}
impl<D: Decimal, T: TryClone, P: Proto> Safe<D, Putter<T, P>> {
    pub fn put<R: Token>(&self, coupon: Coupon<D, R>, datum: T) -> R {
        let _ = coupon;
        self.inner.put(datum);
        R::fresh()
    }
}

pub struct Coupon<D: Decimal, R: Token> {
    phantom: PhantomData<(D, R)>,
}

impl<T: Token> NoData for T {
    fn fresh() -> Self {
        debug_assert!(mem::size_of::<Self>() == 0);
        unsafe { mem::uninitialized() }
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
impl<A: Nand> Nand for (A, T) {}
impl<A: Nand> Nand for (A, Neg<F>) {}
impl<A: Nand> Nand for (A, X) {}
impl<A: Nand> Nand for (A, Neg<X>) {}
// only right is NAND
impl<A: Nand> Nand for (T, A) {}
impl<A: Nand> Nand for (Neg<F>, A) {}
impl<A: Nand> Nand for (X, A) {}
impl<A: Nand> Nand for (Neg<X>, A) {}
// both sides are NAND
impl<A: Nand, B: Nand> Nand for (A, B) {}

pub trait Var {}
impl Var for T {}
impl Var for F {}
impl Var for X {}

pub trait Transition<P: Proto>: Sized {
    fn from_rule_id(proto_rule_id: RuleId) -> Self;
}

pub trait Advance<P: Proto>: Sized {
    type Opts: Transition<P>;
    fn advance<F, R>(self, port_group: PortGroup<P>, handler: F) -> R
    where
        F: FnOnce(Self::Opts) -> R,
    {
        let choice: Self::Opts = match mem::size_of::<Self::Opts>() {
            0 => unsafe { mem::uninitialized() },
            _ => port_group.ready_wait_determine(),
        };
        handler(choice)
    }
}
