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

pub struct Coupon<P: Token, S: Token> {
    phantom: PhantomData<(P, S)>,
}
impl<P: Token, S: Token> Token for Coupon<P, S> {}

pub struct Port<P: Token> {
    phantom: PhantomData<P>,
}
impl<P: Token> Port<P> {
    pub fn act<S: Token>(&mut self, coupon: Coupon<P, S>) -> S {
        let _ = coupon;
        NoData::fresh()
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
    // type Coupon: Token;
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

pub trait Knowable: Advance {
    // type CouponType: Token;
    type CouponType: Token;
    fn only_coupon(self) -> Self::CouponType;
}
impl<X, Y> Knowable for X
where
    X: Advance<Opts = Y>,
    Y: KnownCoupon,
{
    type CouponType = Coupon<<Y as KnownCoupon>::PortNum, <Y as KnownCoupon>::State>;
    fn only_coupon(self) -> Self::CouponType {
        NoData::fresh()
    }
}

// pub struct Locked<T: Token> {
//     phantom: PhantomData<T>,
// }
// impl<T: Token> Token for Locked<T> {}
// impl<T: Tern> Locked<State<T>> {
//     pub fn unlock(self, act_result: ActResult) -> State<T> {
//         let _ = act_result;
//         NoData::fresh()
//     }
// }
// impl<A: Token, N: Token> Locked<States<A, N>> {
//     pub fn unlock(self, act_result: &ActResult) -> States<A, N> {
//         let _ = act_result;
//         NoData::fresh()
//     }
// }

// pub enum StatesReified<S: Token, N: Token> {
//     Head(S),
//     Tail(N),
// }

// pub struct States<S: Token, N: Token> {
//     //        ^state    ^next
//     phantom: PhantomData<(S, N)>,
// }
// impl<S: Token, N: Token> Token for States<S, N> {}
// impl<S: Token, N: Token> States<S,N> {
//     fn head_or(self, act_res: &ActResult) -> StatesReified<S,N> {
//         if act_res.proto_state_data == 5 {
//             StatesReified::Head(NoData::fresh())
//         } else {
//             StatesReified::Tail(NoData::fresh())
//         }
//     }
// }

// pub trait Collapsing: Token {
//     type Out: Token;
//     fn collapse(self) -> Self::Out {
//         NoData::fresh()
//     }
// }
// impl<A: Token, B: Token> Collapsing for States<A, B>
// where
//     (A, B): UnifyState,
// {
//     type Out = <(A, B) as UnifyState>::Out;
// }

// pub struct ActResult {
//     proto_state_data: u32,
// }

// impl<A: Token, B: Token, N: Token> Collapsing for States<A, States<B, N>>
// where
//     (A, B): UnifyState,
//     States<<(A, B) as UnifyState>::Out, N>: Collapsing,
// {
//     type Out = <States<<(A, B) as UnifyState>::Out, N> as Collapsing>::Out;
// }

/////////////////////// SPECIFIC /////////////////

// {R1,R2}
pub enum R1R2<S0: Token, S1: Token> {
    R1(Coupon<N0, S0>),
    R2(Coupon<N1, S1>),
}
impl Advance for State<X> {
    type Opts = R1R2<X, State<T>>;
}

// {R1}
pub enum R1<S0: Token> {
    R1(Coupon<N0, S0>),
}
impl<S0: Token> KnownCoupon for R1<S0> {
    type PortNum = N0;
    type State = S0;
}
impl Advance for State<F> {
    type Opts = R1<State<T>>;
}

// {R2}

pub enum R2<S1: Token> {
    R2(Coupon<N1, S1>),
}
impl Advance for State<T> {
    type Opts = R2<State<F>>;
}

// {} (NONE)

pub fn atomic(mut f: State<F>, mut p0: Port<N0>, mut p1: Port<N1>) -> ! {
    loop {
        let t = p0.act(f.only_coupon());
        // let t = f.advance(|o| match o {
        //     R1::R1(coupon) => p0.act(coupon),
        // });
        f = t.advance(|o| match o {
            R2::R2(coupon) => p1.act(coupon),
        })
    }
}

// mod testing {
//     use std::marker::PhantomData;
//     struct T;
//     struct F;
//     struct X;

//     trait IsTrue {}
//     impl<T> IsTrue for T {}

//     trait TX: Terny {}
//     impl TX for T {}
//     impl TX for X {}
//     trait Terny {
//         type Not: Terny;
//     }
//     impl Terny for T {
//         type Not = F;
//     }
//     impl Terny for F {
//         type Not = T;
//     }
//     impl Terny for X {
//         type Not = X;
//     }

//     trait NandT<X: Terny> {
//         type Nand: Terny;
//     }
//     impl NandT<T> for T {
//         type Nand = F;
//     }
//     impl NandT<F> for T {
//         type Nand = T;
//     }
//     impl<Q: Terny> NandT<Q> for F {
//         type Nand = T;
//     }

//     trait SomeFalse {}
//     impl SomeFalse for F {}
//     impl<A,B> SomeFalse for (A,B) where A: SomeFalse, B: TX {}
//     impl<A> SomeFalse for (A,F) {}

//     pub struct State<A:Terny, B:Terny, C:Terny> {
//         phantom: PhantomData<(A,B,C)>,
//     }
//     pub trait Advance: Sized {
//         fn advance(self) {
//             unimplemented!()
//         }
//     }

//     // FIRST
//     // match [**F] but not [TF*]
//     impl<X1: Terny, X2: Terny> Advance for State<X1,X2,F>
//     where
//         X1: NandT<X2>,
//     {}

//     // SECOND
//     impl Advance for State<T,F,F> {}

//     pub fn tryit() {
//         let start: State<T,F,F> = unsafe {std::mem::uninitialized()};
//         match start.advance() {

//         }
//     }

//     // R1: XXF
//     // R2: XFT
//     // R3: TFX

//     // // {2} but not {1,3}
//     // enum Opts2 {
//     //     R2,
//     // }
//     // impl<X1: Terny> Advance for State<X1,F,T>
//     // where
//     //     // r1 [XXF] implicitly blocked
//     //     // r3 [TFX]
//     //     X1: SomeFalse,
//     // {
//     //     type Option = Opts2;
//     // }

//     // // TODO note that this is unusatisfiable
//     // // {3} but not {1,2}
//     // enum Opts3 {
//     //     R3,
//     // }
//     // impl<X3: Terny> Advance for State<T,F,X3>
//     // where
//     //     // r1 [XXF]
//     //     X3::Not: SomeFalse,
//     //     // r2 [TFX]
//     //     X3: SomeFalse,
//     // {
//     //     type Option = Opts3;
//     // }

//     // {1,2,3} but not {} is unsatisfiable!

// }
