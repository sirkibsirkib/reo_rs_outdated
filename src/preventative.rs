/* IDEA:
The user is only allowed to hook up an atomic component if its implementation
is a function with the correct signature.
Here: fn atomic(mut env: Env, mut u: State<U>) -> !;
interpreted:
- given environment object (ports struct) "Env"
- given start state "U"
- which never returns

The user can only PUT and GET using the port objects inside Env, but they will
soon find out that the Put operation requires as input a "Coupon"-type token,
and returns a Receipt-type token. Now what?

Env, in addition to containing ports as fields, implements one "advance" function
per state. Eg: "advance_u". This function takes as input the U token and returns
the next state (in this case, V). BUT this function requires the USER to define 
a function that maps a given COUPON into a RECEIPT.
*/
use std::marker::PhantomData;

/*
Coupon<E,P> is a token consumed by port P to return Receipt<E> where E is
the environment of P.
*/
pub struct Coupon<E,P,N> {
    phantom: PhantomData<(E,P,N)>,
}
impl<E,P,N> Coupon<E,P,N> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
}


/*
Receipt<E> is provided by some port with label P when provided Coupon<E,P>.
*/
// pub struct Receipt<E> {
//     phantom: PhantomData<E>,
// }
// impl<E> Receipt<E> {
//     pub(crate) fn fresh() -> Self {
//         Self { phantom: PhantomData::default() }
//     }
// }


pub struct State<E,S> {
    phantom: PhantomData<(E,S)>,
}
impl<E,S> State<E,S> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
}

mod nums {
    pub struct N0;
    pub struct N1;
    pub struct N2;
    pub struct N3;
    pub struct N4;
    pub struct N5;
    pub struct N6;
    pub struct N7;
}
use nums::*;


pub trait RawPort<D> {
    fn put(&mut self, datum: D);
}

struct Port<E,P,D> {
    raw_port: Box<dyn RawPort<D>>,
    phantom: PhantomData<(E,P,D)>,
}
impl<E,P,D> Port<E,P,D> {
    fn new(raw_port: Box<dyn RawPort<D>>) -> Self {
        Self {
            phantom: PhantomData::default(), raw_port,
        }
    }
    pub fn put<N>(&mut self, datum: D, coupon: Coupon<E,P,N>) -> State<E,N> {
        let _ = datum;
        let _ = coupon;
        State::fresh()
    }
}

// mod opts {
//     use super::{Coupon, nums::*};
//     pub enum S0i<E> {
//         Port0(Coupon<E, N0>, State<E, >),
//         Port1(Receipt<E>),
//     }
//     pub enum S1<E> {
//         Port1(Coupon<E, N1>),
//     }
// }


pub enum Opts0 {
    P0S1(Coupon<Env, N0, N1>),
    P1S0(Coupon<Env, N1, N0>),
}

pub enum Opts1 {
    P1S0(Coupon<Env, N1, N0>),
}

// State "U" may require the atomic to do either {A, nothing}
// this structure gives you a coupon for precisely one action
// pub type Vopts<E> = opts::S1<E>;

pub struct Env {
    port0: Port<Self, N0, u32>,
    port1: Port<Self, N1, u32>,
}

impl Env {
    fn advance_0<R>(&mut self, state: State<Self,N0>, handler: impl FnOnce(&mut Self, Opts0) -> R) -> R {
        let _ = state;
        let coupon = Coupon::fresh();
        let opts = Opts0::P0S1(coupon);
        handler(self, opts)
    }
    fn advance_1<R>(&mut self, state: State<Self,N1>, handler: impl FnOnce(&mut Self, Opts1) -> R) -> R {
        let _ = state;
        let coupon = Coupon::fresh();
        let opts = Opts1::P1S0(coupon);
        handler(self, opts)
    }
}

pub fn main(a: Box<dyn RawPort<u32>>, b: Box<dyn RawPort<u32>>) {
    let env = Env {
        port0: Port::new(a),
        port1: Port::new(b),
    };
    // Reo-stitcher will only accept an "atomic function" with the expected signature
    atomic(env, State::fresh());
}

pub trait HasBranches {
    type Branches;
}

impl<E,N> State<E,N> where State<E,N>: HasBranches {
    fn advance<R>(
        self,
        handler: impl FnOnce(<State<E,N> as HasBranches>::Branches) -> R
    ) -> R {
        let opts = unsafe { std::mem::uninitialized() };
        handler(opts)
    }
}
impl HasBranches for State<Env, N0> {
    type Branches = Opts0;
}
impl HasBranches for State<Env, N1> {
    type Branches = Opts1;
}


/*
THIS is the only part the user must implement
*/
pub fn atomic(mut env: Env, mut s0: State<Env,N0>) -> ! {
    loop {
        s0 = s0.advance(|opts| match opts {
            Opts0::P0S1(coupon) => {
                let s1 = env.port0.put(5, coupon);
                s1.advance(|opts| match opts {
                    Opts1::P1S0(coupon) => env.port1.put(5, coupon)
                })
            },
            Opts0::P1S0(coupon) => env.port1.put(5, coupon),
        });
    }
}

