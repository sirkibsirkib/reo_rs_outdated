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
pub struct Coupon<E,P> {
    phantom: PhantomData<(E,P)>,
}
impl<E,P> Coupon<E,P> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
}

/*
Receipt<E> is provided by some port with label P when provided Coupon<E,P>.
*/
pub struct Receipt<E> {
    phantom: PhantomData<E>,
}
impl<E> Receipt<E> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
}


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
    pub fn put(&mut self, datum: D, coupon: Coupon<E,P>) -> Receipt<E> {
        let _ = datum;
        let _ = coupon;
        Receipt::fresh()
    }
}

mod opts {
    use super::{Coupon, Receipt, nums::*};
    pub enum S0i<E> {
        Port0(Coupon<E, N0>),
        Idle(Receipt<E>),
    }
    pub enum S1<E> {
        Port1(Coupon<E, N1>),
    }
}

// State "U" may require the atomic to do either {A, nothing}
// this structure gives you a coupon for precisely one action
pub type Vopts<E> = opts::S1<E>;

pub struct Env {
    port0: Port<Self, N0, u32>,
    port1: Port<Self, N1, u32>,
}

impl Env {
    fn advance_0(&mut self, state: State<Self,N0>, handler: impl FnOnce(&mut Self, opts::S0i<Self>) -> Receipt<Self>) -> State<Self,N1> {
        let _ = state;
        let coupon = Coupon::fresh();
        let opts = opts::S0i::Port0(coupon);
        let _done = handler(self, opts);
        State::fresh()
    }
    fn advance_1(&mut self, state: State<Self,N1>, handler: impl FnOnce(&mut Self, opts::S1<Self>) -> Receipt<Self>) -> State<Self,N0> {
        let _ = state;
        let coupon = Coupon::fresh();
        let opts = opts::S1::Port1(coupon);
        let _done = handler(self, opts);
        State::fresh()
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


/*
THIS is the only part the user must implement
*/
pub fn atomic(mut env: Env, mut u: State<Env,N0>) -> ! {
    loop {
        use opts::{S0i::*, S1::*};
        let v = env.advance_0(u, |env, opts| match opts {
            Port0(coupon) => env.port0.put(5, coupon),
            Idle(receipt) => receipt,
        });
        u = env.advance_1(v, |env, opts| match opts {
            Port1(coupon) => env.port1.put(3, coupon),
        });
    }
}

