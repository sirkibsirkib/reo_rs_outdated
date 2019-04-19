/* IDEA:
The user is only allowed to hook up an atomic component if its implementation
is a function with the correct signature.
Here: fn atomic(mut env: Env, mut state: State<N0>) -> !;
interpreted:
- given environment object (ports struct) "Env"
- given start state State<N0> (state with 0th name "N0")
- which never returns

The user plays a game of trading STATE tokens for COUPON tokens and back.
STATE tokens represent being in a state of the automaton and not yet knowing
which branch will be taken. the user must invoke ADVANCE to collapse the sum type into
a concrete COUPON<_,P,S>. This coupon is only usable on port P (for Get or Put)
to generate the next state token State<_,S>.

*/
use std::marker::PhantomData;

/*
This coupon can be spent on a port with type Port<E,P> to create token
State<E,N>
*/
pub struct Coupon<E, P, S> {
    phantom: PhantomData<(E, P, S)>,
}
impl<E, P, S> Coupon<E, P, S> {
    pub(crate) fn fresh() -> Self {
        Self {
            phantom: PhantomData::default(),
        }
    }
}

pub struct State<E, S> {
    phantom: PhantomData<(E, S)>,
}
impl<E, S> State<E, S> {
    pub(crate) fn fresh() -> Self {
        Self {
            phantom: PhantomData::default(),
        }
    }
}

mod nums {
    pub struct N0;
    pub struct N1;
    pub struct N2;
    pub struct N3;
}
use nums::*;

pub trait RawPort<D> {
    fn put(&mut self, datum: D);
}

struct Port<E, P, D> {
    raw_port: Box<dyn RawPort<D>>,
    phantom: PhantomData<(E, P, D)>,
}
impl<E, P, D> Port<E, P, D> {
    fn new(raw_port: Box<dyn RawPort<D>>) -> Self {
        Self {
            phantom: PhantomData::default(),
            raw_port,
        }
    }
    pub fn put<N>(&mut self, datum: D, coupon: Coupon<E, P, N>) -> State<E, N> {
        let _ = datum;
        let _ = coupon;
        State::fresh()
    }
}

// State "U" may require the atomic to do either {A, nothing}
// this structure gives you a coupon for precisely one action
// pub type Vopts<E> = opts::S1<E>;

pub struct Env {
    port0: Port<Self, N0, u32>,
    port1: Port<Self, N1, u32>,
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
    type Branches: From<RuntimeDeliberation>;
}

pub trait Deliberator {
    fn deliberate(&mut self) -> RuntimeDeliberation;
} 

impl<E, N> State<E, N> where State<E, N>: HasBranches {
    fn advance<D: Deliberator,R>(
        self,
        deliberator: &mut D,
        handler: impl FnOnce(&mut D, <State<E, N> as HasBranches>::Branches) -> State<E, R>,
    ) -> State<E, R> {
        let deliberation = deliberator.deliberate();
        let opt = deliberation.into();
        handler(deliberator, opt)
    }
}

// this is the part that Reo must generate for the concrete automaton
impl HasBranches for State<Env, N0> {
    type Branches = Opts0;
}
impl HasBranches for State<Env, N1> {
    type Branches = Opts1;
}
pub enum Opts0 {
    P0S1(Coupon<Env, N0, N1>),
    P1S0(Coupon<Env, N1, N0>),
}
impl From<RuntimeDeliberation> for Opts0 {
    fn from(r: RuntimeDeliberation) -> Self {
        use Opts0::*;
        match [r.port, r.new_state] {
            [0,1] => P0S1(Coupon::fresh()),
            [1,0] => P1S0(Coupon::fresh()),
            _ => panic!("BAD DELIBERATION"),
        }
    }
}

pub enum Opts1 {
    P1S0(Coupon<Env, N1, N0>),
}
impl From<RuntimeDeliberation> for Opts1 {
    fn from(r: RuntimeDeliberation) -> Self {
        use Opts1::*;
        match [r.port, r.new_state] {
            [1,0] => P1S0(Coupon::fresh()),
            _ => panic!("BAD DELIBERATION"),
        }
    }
}

pub struct RuntimeDeliberation {
    port: u32,
    new_state: u32,
}

impl Deliberator for Env {
    fn deliberate(&mut self) -> RuntimeDeliberation {
        unimplemented!()
    }
}

// This is the part that the user must implement
pub fn atomic(mut env: Env, mut s0: State<Env, N0>) -> ! {
    let env = &mut env;
    loop {
        s0 = s0.advance(env, |env, opts| match opts {
            Opts0::P0S1(c) => {
                let s1 = env.port0.put(5, c);
                s1.advance(env, |env, opts| match opts {
                    Opts1::P1S0(c) => env.port1.put(5, c),
                })
            }
            Opts0::P1S0(c) => env.port1.put(5, c),
        })
    }
}
