use crate::proto::groups::PortGroup;
use crate::proto::{traits::Proto, Getter, Putter};
use std::marker::PhantomData;
use std::mem;

use crate::{LocId, RuleId};


// for types that have NO SIZE and thus can be created without context
pub unsafe trait NilBytes: Sized {
    unsafe fn fresh() -> Self {
        debug_assert!(mem::size_of::<Self>() == 0);
        mem::uninitialized()
    }
}
unsafe impl NilBytes for () {}

pub mod decimal {
    use super::*;

    pub trait Decimal: NilBytes {
        const N: usize;
    }

    macro_rules! def_decimal {
        ($d:tt, $e:tt, $n:tt) => {
            pub struct $d<T>(PhantomData<T>);
            impl<T: Decimal> Decimal for $d<T> {
                const N: usize = <T as Decimal>::N + $n;
            }
            impl Decimal for $d<()> {
                const N: usize = $n;
            }
            unsafe impl<T: NilBytes> NilBytes for $d<T> {}
            pub type $e = $d<()>;
        };
    }
    def_decimal![D0, E0, 0];
    def_decimal![D1, E1, 1];
    def_decimal![D2, E2, 2];
    def_decimal![D3, E3, 3];
    def_decimal![D4, E4, 4];
    def_decimal![D5, E5, 5];
    def_decimal![D6, E6, 6];
    def_decimal![D7, E7, 7];
    def_decimal![D8, E8, 8];
    def_decimal![D9, E9, 9];
}
use decimal::*;

pub struct Safe<D: Decimal, T> {
    original_id: LocId,
    inner: T,
    phantom: PhantomData<D>,
}

impl<T: 'static> Getter<T> {
    pub unsafe fn safe_wrap<D: Decimal>(mut self, leader: LocId) -> Safe<D, Self> {
        let original_id = self.id;
        self.id = leader;
        Safe {
            inner: self,
            phantom: PhantomData::default(),
            original_id,
        }
    }
}
impl<T: 'static> Putter<T> {
    pub unsafe fn safe_wrap<D: Decimal>(mut self, leader: LocId) -> Safe<D, Self> {
        let original_id = self.id;
        self.id = leader;
        Safe {
            inner: self,
            phantom: PhantomData::default(),
            original_id,
        }
    }
}

impl<D: Decimal, T> Safe<D, Getter<T>> {
    pub fn get<R: NilBytes>(&mut self, coupon: Coupon<D, R>) -> (T, R) {
        let _ = coupon;
        (self.inner.get(), unsafe { R::fresh() })
    }
}
impl<D: Decimal, T> Safe<D, Putter<T>> {
    pub fn put<R: NilBytes>(&mut self, coupon: Coupon<D, R>, datum: T) -> R {
        let _ = coupon;
        self.inner.put(datum);
        unsafe { R::fresh() }
    }
}



pub struct Coupon<D: Decimal, R: NilBytes> {
    phantom: PhantomData<(D, R)>,
}
unsafe impl<D: Decimal, R: NilBytes> NilBytes for Coupon<D, R> {}

////////////


pub struct Discerned<Q> {
    rule_id: usize,
    phantom: PhantomData<Q>,
}

pub struct State<Q> {
    phantom: PhantomData<Q>,
}
unsafe impl<Q> NilBytes for State<Q> {}

pub trait MayBranch {
    const BRANCHING: bool;
}

// terminal 0
impl Discerned<()> {
    pub fn match_nil(self) -> () {
        ()
    }
}
impl MayBranch for Discerned<()> {
    const BRANCHING: bool = false;
}
// terminal 1
impl<D: Decimal, S> Discerned<(Branch<D,S>,())> {
    pub fn match_singleton(self) -> Branch<D,S> {
        assert_eq!(self.rule_id, D::N);
        Branch {
            phantom: PhantomData::default(),
        }
    }
}
impl<D: Decimal, S> MayBranch for Discerned<(Branch<D,S>,())> {
    const BRANCHING: bool = false;
}

// chain 2+
impl<D: Decimal, S, N1, N2> Discerned<(Branch<D,S>,(N1, N2))> {
    pub fn match_head(self) -> Result<Branch<D,S>, Discerned<(N1,N2)>> {
        if D::N == self.rule_id {
            Ok(Branch {
                phantom: PhantomData::default(),
            })
        } else {
            Err(Discerned {
                rule_id: self.rule_id,
                phantom: PhantomData::default(),
            })
        }
    }
}
impl<D: Decimal, S, N1, N2> MayBranch for Discerned<(Branch<D,S>,(N1, N2))> {
    const BRANCHING: bool = true;
}



pub struct Branch<D: Decimal, S> {
    phantom: PhantomData<(D,S)>,
}
impl<D: Decimal, S> Branch<D,S> {
    pub fn print(self) {
        println!("Branch with N={:?}", D::N);
    }
}

#[macro_export]
macro_rules! match_list {
    ($d:expr; ) => {{
        $d.finalize()
    }};
    ($d:expr; $e:ident => $b:expr $(,)*) => {{
        let $e = $d.match_singleton();
        $b
    }};
    ($d:expr; $e:ident => $b:expr, $($en:ident => $bn:expr),+ $(,)*) => {{
        match $d.match_head() {
            Ok($e) => $b,
            Err(__d) => match_list!(__d; $($en => $bn),+),
        }
    }};
}

pub struct T;
pub struct F;
pub struct X;