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


pub struct State<S> {
    phantom: PhantomData<S>,
}
impl<S> State<S> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
}


/*
used as the P parameter for some Coupon<E,P>
Not a protected type as they are never created or destroyed
only used to distinguish Coupon<E,x> from Coupon<E,y> where x is not y
*/
mod port_label {
    pub struct A;
    pub struct B;
}

/*
used as the S parameter for some State<S>
Not a protected type as they are never created or destroyed
only used to distinguish State<x> from State<y> where x is not y
*/
mod state_label {
    pub struct U;
    pub struct V;
}
use state_label::*;

struct Port<E,P,D> {
    phantom: PhantomData<(E,P,D)>,
}
impl<E,P,D> Port<E,P,D> {
    fn new() -> Self {
        Self { phantom: PhantomData::default() }
    }
    pub fn put(&mut self, datum: D, coupon: Coupon<E,P>) -> Receipt<E> {
        let _ = datum;
        let _ = coupon;
        Receipt::fresh()
    }
}

// State "U" may require the atomic to do either {A, nothing}
// this structure gives you a coupon for precisely one action
pub enum Uopts {
    PortA(Coupon<Env, port_label::A>),
    NoAction(Receipt<Env>),
}
pub enum Vopts {
    PortB(Coupon<Env, port_label::B>),
}

pub struct Env {
    port_a: Port<Self, port_label::A, u32>,
    port_b: Port<Self, port_label::B, u32>,
}
impl Env {
    fn advance_u(&mut self, u: State<U>, handler: impl FnOnce(&mut Self, Uopts) -> Receipt<Self>) -> State<V> {
        let _ = u;
        let coupon = Coupon::fresh();
        let opts = Uopts::PortA(coupon);
        let _done = handler(self, opts);
        State::fresh()
    }
    fn advance_v(&mut self, v: State<V>, handler: impl FnOnce(&mut Self, Vopts) -> Receipt<Self>) -> State<U> {
        let _ = v;
        let coupon = Coupon::fresh();
        let opts = Vopts::PortB(coupon);
        let _done = handler(self, opts);
        State::fresh()
    }
}

#[test]
pub fn main() {
    let env = Env {
        port_a: Port::new(),
        port_b: Port::new(),
    };
    // Reo-stitcher will only accept an "atomic function" with the expected signature
    atomic(env, State::fresh());
}

/*
THIS is the only part the user must implement
*/
fn atomic(mut env: Env, mut u: State<U>) -> ! {
    loop {
        let v = env.advance_u(u, |env, opts| match opts {
            Uopts::PortA(coupon) => env.port_a.put(5, coupon),
            Uopts::NoAction(receipt) => receipt,
        });
        u = env.advance_v(v, |env, opts| match opts {
            Vopts::PortB(coupon) => env.port_b.put(3, coupon),
        });
    }
}