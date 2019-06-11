
use crate::bitset::BitSet;
use crate::proto::{Getter, Putter};
use std::marker::PhantomData;
use std::{mem, fmt};
use crate::{LocId};

// for types that have NO SIZE and thus can be created without context
pub unsafe trait Token : Sized {
    unsafe fn fresh() -> Self {
        debug_assert!(mem::size_of::<Self>() == 0);
        mem::uninitialized()
    }
}
unsafe impl Token  for () {}

pub mod decimal {
    use super::*;

    pub trait Decimal: Token  {
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
            unsafe impl<T: Token > Token  for $d<T> {}
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
    pub fn get<S>(&mut self, coupon: Coupon<D, S>) -> (T, State<S>) {
        let _ = coupon;
        (self.inner.get(), unsafe { State::fresh() })
    }
}
impl<D: Decimal, T> Safe<D, Putter<T>> {
    pub fn put<S>(&mut self, coupon: Coupon<D, S>, datum: T) -> State<S> {
        let _ = coupon;
        self.inner.put(datum);
        unsafe { State::fresh() }
    }
}

/// A token structure which can be consumed in a Safe<Putter<D, _>>::put
/// or Safe<Getter<D, _>>::get invocation, being consumed in the process,
/// yielding a new state token, State<S>.
pub struct Coupon<D: Decimal, S> {
    phantom: PhantomData<(D, S)>,
}
unsafe impl<D: Decimal, S> Token for Coupon<D, S> {}
impl<D: Decimal, S> fmt::Debug for Coupon<D, S> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Coupon for port with N={}", D::N)
    }
}

////////////

/// A dynamically-created enum class. Contains the data which determines its Coupon
/// variant upon creation (at run-time). Can be inspected to determine
/// which variant is matched. This simulates "matching" an enum, allowing one
/// to differentiate behaviour at compile-time, based on the variant
/// (determined at run-time).
///
/// Eg: Discerned<()> represents an empty set (no variants) which can only be
///     trivially discarded.
/// Eg: Discerned<(Coupon<D,S>, ())> represents a singleton set which yields
///     Coupon<D,S> upon matching.
/// Eg: Discerned<(Coupon<A,B>, (Coupon<C,D>, ())) represents a two-element list
///     and upon inspection may match either Coupon<A,B> or Coupon<C,D>.
// pub struct Discerned<Q> {
//     rule_id: usize,
//     phantom: PhantomData<Q>,
// }


pub trait StateCheck {
    fn explains_state(against: &BitSet) -> bool;
}

pub trait Booly {
    const BOOL: Option<bool>; 
} 
impl Booly for F {
    const BOOL: Option<bool> = Some(false); 
} 
impl Booly for T {
    const BOOL: Option<bool> = Some(true); 
} 
impl Booly for X {
    const BOOL: Option<bool> = None; 
} 

impl<A,B> StateCheck for (A,B)
where A: Booly, B: Booly {
    fn explains_state(x: &BitSet) -> bool {
        (A::BOOL.filter(|&b| b != x.test(0))).is_none()
        &&
        (B::BOOL.filter(|&b| b != x.test(1))).is_none()
    }
}

pub struct Discerned2<'a, Q> {
    data: Option<(LocId, &'a BitSet)>, 
    phantom: PhantomData<Q>,
}
impl<Q> Discerned2<'_, Q> {
    pub fn trivial() -> Self {
        Self {
            data: None,
            phantom: PhantomData::default(),
        }
    }
}

// terminal 0
impl Discerned2<'_, ()> {
    pub fn match_nil(self) -> () {
        ()
    }
}
impl MayBranch for Discerned2<'_, ()> {
    const BRANCHING: bool = false;
}

// terminal 1
impl<R: Decimal, P: Decimal, S: StateCheck> Discerned2<'_, (Branch<R, P, S>, ())> {
    pub fn match_singleton(self) -> Coupon<P, State<S>> {
        Coupon {
            phantom: PhantomData::default(),
        }
    }
}
impl<R: Decimal, P: Decimal, S> MayBranch for Discerned2<'_, (Branch<R, P, S>, ())> {
    const BRANCHING: bool = false;
}

// chain 2+
impl<'a, R: Decimal, P: Decimal, S: StateCheck, N1, N2> Discerned2<'a, (Branch<R, P, S>, (N1, N2))> {
    pub fn match_head(self) -> Result<Coupon<P, State<S>>, Discerned2<'a, (N1, N2)>> {
        let d = self.data.as_ref().expect("SINGLETON UNWRAP");
        if P::N == d.0 && S::explains_state(d.1) {
            Ok(Coupon {
                phantom: PhantomData::default(),
            })
        } else {
            Err(Discerned2 {
                data: self.data,
                phantom: PhantomData::default(),
            })
        }
    }
}

impl<R: Decimal, P: Decimal, S, N1, N2> MayBranch for Discerned2<'_, (Branch<R, P, S>, (N1, N2))> {
    const BRANCHING: bool = true;
}



pub struct State<Q> {
    phantom: PhantomData<Q>,
}
unsafe impl<Q> Token for State<Q> {}

pub trait MayBranch {
    const BRANCHING: bool;
}

/// Represents en element of a Discerned list (variant of a simulated enum).
/// R: decimal which numbers the rule matched.
/// P: decimal which numbers the port involved.
/// S: generic arg Q of State<Q>, determining the resulting state.
pub struct Branch<R: Decimal, P: Decimal, S> {
    phantom: PhantomData<(R,P,S)>,
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
