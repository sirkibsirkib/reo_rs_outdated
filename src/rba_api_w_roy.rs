use crate::decimal::*;
use std::marker::PhantomData;


//////////// FULLY GENERIC ////////////////////

impl Token for () {}
impl<T: Token> Token for D0<T> {}
impl<T: Token> Token for D1<T> {}
impl<T: Token> Token for D2<T> {}
impl<T: Token> Token for D3<T> {}
impl<T: Token> Token for D4<T> {}
impl<T: Token> Token for D5<T> {}
impl<T: Token> Token for D6<T> {}
impl<T: Token> Token for D7<T> {}
impl<T: Token> Token for D8<T> {}
impl<T: Token> Token for D9<T> {}

pub trait Token: Sized {
    /*PUBLIC*/
}

trait NoData: Token {
    // PRIVATE
    fn fresh() -> Self {
        unsafe { std::mem::uninitialized() }
    }
}
impl<T: Token> NoData for T {}

pub struct Coupon<P: Token> {
    phantom: PhantomData<P>,
}
impl<P: Token> Token for Coupon<P> {}

pub struct ActResult {
    proto_state_data: u32,
}

pub struct Port<P: Token> {
    phantom: PhantomData<P>,
}
impl<P: Token> Port<P> {
    pub fn act(&mut self, coupon: Coupon<P>) -> ActResult {
        let _ = coupon;

        ActResult {
            proto_state_data: 5,
        }
    }
}

pub struct F;
pub struct T;
pub struct X;
impl Token for F {}
impl Token for T {}
impl Token for X {}

pub trait FX: Tern {}
impl FX for F {}
impl FX for X {}

pub trait TX: Tern {}
impl TX for T {}
impl TX for X {}

pub trait TF: Tern {}
impl TF for T {}
impl TF for F {}

pub trait Tern: Token {}
impl Tern for T {}
impl Tern for F {}
impl Tern for X {}

pub trait UnifyTern {
    type Out: Tern;
}
impl UnifyTern for (F, X) {
    type Out = X;
}
impl<A: FX> UnifyTern for (T, A) {
    type Out = X;
}
impl UnifyTern for (X, F) {
    type Out = X;
}
impl<A: FX> UnifyTern for (A, T) {
    type Out = X;
}
impl UnifyTern for (T, T) {
    type Out = T;
}
impl UnifyTern for (F, F) {
    type Out = F;
}

pub trait UnifyState {
    type Out: Token;
}

impl<A1: Tern, A2: Tern> UnifyState for (State<A1>, State<A2>)
where
    (A1, A2): UnifyTern,
{
    type Out = State<<(A1, A2) as UnifyTern>::Out>;
}

pub struct State<A: Tern> {
    phantom: PhantomData<A>,
}
impl<A: Tern> Token for State<A> {}

impl<A: TF> State<A> {
    pub fn weaken_a(self) -> State<X> {
        NoData::fresh()
    }
}

pub trait KnownCoupon {
    type PortNum: Token;
    type State: Token;
}

pub trait Advance: Token {
    type Opts;
    fn advance<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self::Opts) -> R,
    {
        let x = unsafe { std::mem::uninitialized() };
        f(x)
    }
}


// pub trait Knowable: Advance {
//     // type CouponType: Token;
//     type PortId: Token;
//     type States: Token;
//     fn only_coupon(self) -> Self::CouponType;
// }
// impl<X, Y> Knowable for X
// where
//     X: Advance<Opts = Y>,
//     Y: KnownCoupon,
// {
//     type PortId = 
//     type CouponType = Coupon<<Y as KnownCoupon>::PortNum, <Y as KnownCoupon>::State>;
//     fn only_coupon(self) -> Self::CouponType {
//         NoData::fresh()
//     }
// }

pub struct Locked<T: Token> {
    phantom: PhantomData<T>,
}
impl<T: Token> Token for Locked<T> {}
impl<T: Tern> Locked<State<T>> {
    pub fn unlock(self, act_result: ActResult) -> State<T> {
        let _ = act_result;
        NoData::fresh()
    }
}
impl<A: Token, N: Token> Locked<States<A, N>> {
    pub fn unlock(self, act_result: &ActResult) -> States<A, N> {
        let _ = act_result;
        NoData::fresh()
    }
} 


pub enum StatesReified<S: Token, N: Token> {
    Head(S),
    Tail(N),
}

pub struct States<S: Token, N: Token> {
    //        ^state    ^next
    phantom: PhantomData<(S, N)>,
}
impl<S: Token, N: Token> Token for States<S, N> {}
impl<S: Token, N: Token> States<S,N> {
    fn head_or(self, act_res: &ActResult) -> StatesReified<S,N> {
        if act_res.proto_state_data == 5 {
            StatesReified::Head(NoData::fresh())
        } else {
            StatesReified::Tail(NoData::fresh())
        }
    }
}

pub trait Collapsing: Token {
    type Out: Token;
    fn collapse(self) -> Self::Out {
        NoData::fresh()
    }
}
impl<A: Token, B: Token> Collapsing for States<A, B>
where
    (A, B): UnifyState,
{
    type Out = <(A, B) as UnifyState>::Out;
}

impl<A: Token, B: Token, N: Token> Collapsing for States<A, States<B, N>>
where
    (A, B): UnifyState,
    States<<(A, B) as UnifyState>::Out, N>: Collapsing,
{
    type Out = <States<<(A, B) as UnifyState>::Out, N> as Collapsing>::Out;
}

/////////////////////// SPECIFIC /////////////////



pub enum P0<S0: Token> {
    P0(Coupon<N0>, Locked<S0>),
}
impl<S0: Token> KnownCoupon for P0<S0> {
    type PortNum = N0;
    type State = S0;
}
pub enum P1<S1: Token> {
    P1(Coupon<N1>, Locked<S1>),
}
impl<S1: Token> KnownCoupon for P1<S1> {
    type PortNum = N1;
    type State = S1;
}

pub enum P0P1<S0: Token, S1: Token> {
    P0(Coupon<N0>, Locked<S0>),
    P1(Coupon<N1>, Locked<S1>),
}


// {P0,P1}
impl Advance for State<X> {
    type Opts = P0P1<State<T>, State<F>>;
}
// {P0}
impl Advance for State<F> {
    type Opts = P0<State<T>>;
}
// {P1}
impl Advance for State<T> {
    type Opts = P1<States<State<T>, State<F>>>;
}
// {} (NONE)



pub fn atomic(mut f: State<F>, mut p0: Port<N0>, mut p1: Port<N1>) -> ! {
    loop {
        let t = f.advance(|o| match o {
            P0::P0(coupon, state) => {
                let res = p0.act(coupon);
                let s = state.unlock(res);
                s
            },
        });
        let mut t_or_f = TorF::T(t);
        f = loop {
            t_or_f = match t_or_f {
                TorF::F(f) => break f,
                TorF::T(t) => {
                    t.advance(|o| match o {
                        P1::P1(coupon, state) => {
                            let res = p1.act(coupon);
                            let s = state.unlock(&res);
                            use StatesReified::*;
                            match s.head_or(&res) {
                                Head(t) => TorF::T(t),
                                Tail(f) => TorF::F(f),
                            }
                        }
                    })
                },
            }
        }
    }
}

enum TorF {
    T(State<T>),
    F(State<F>),
}
