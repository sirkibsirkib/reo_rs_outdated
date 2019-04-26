use crate::decimal::*;
use std::marker::PhantomData;

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

pub struct Coupon<P: Token, R: Token> {
    phantom: PhantomData<(P, R)>,
}
impl<P: Token, R: Token> Token for Coupon<P, R> {}

// struct Obl<P: Token, N: Token> {
// 	phantom: PhantomData<(P,N)>,
// }
// impl<P: Token, N: Token> Token for Obl<P,N> {}
// impl<P: Token, N: Token> Obl<P,N> {
// 	fn cover<F>(self, work: F) -> N where F: FnOnce(Coupon<P>)->Receipt {
// 		let _receipt = work(NoData::fresh());
// 		NoData::fresh()
// 	}
// }

// struct CovReceipt;
// impl Token for CovReceipt {}

pub struct Port<P: Token> {
    phantom: PhantomData<P>,
}
impl<P: Token> Port<P> {
    pub fn act<R: Token>(&mut self, coupon: Coupon<P, R>) -> R {
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

trait UnifyTern {
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
// impl<A,B> UnifyTern for (A,B) where (B,A): UnifyTern {
// 	type Out = <(B,A) as UnifyTern>::Out;
// }

// impl<A: Tern> UnifyTern for (T,A) {
// 	type Out = T;
// }
// impl<A: FX> UnifyTern for (A,T) {
// 	type Out = T;
// }
// impl<A: FX> UnifyTern for (X,A) {
// 	type Out = X;
// }
// impl UnifyTern for (F,X) {
// 	type Out = X;
// }
// impl UnifyTern for (F,F) {
// 	type Out = F;
// }

// trait UnifyState: Sized {
// 	type Out: Token;
// 	fn unify(self) -> Self::Out {
// 		NoData::fresh()
// 	}
// }

trait UnifyState {
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
// impl<A: Tern> State<A> {
// 	fn unify_with<B: Tern>(self) -> State<<(A,B) as UnifyTern>::Out> where (A,B): UnifyTern {
// 		NoData::fresh()
// 	}
// }
impl<A: TF> State<A> {
    pub fn weaken_a(self) -> State<X> {
        NoData::fresh()
    }
}

pub trait KnownCoupon {
    type PortNum: Token;
    type State: Token;
}

pub enum P0<S0: Token> {
    P0(Coupon<N0, S0>),
}
impl<S0: Token> KnownCoupon for P0<S0> {
    type PortNum = N0;
    type State = S0;
}

pub enum P1<S1: Token> {
    P1(Coupon<N1, S1>),
}
impl<S1: Token> KnownCoupon for P1<S1> {
    type PortNum = N1;
    type State = S1;
}

pub enum P0P1<S0: Token, S1: Token> {
    P0(Coupon<N0, S0>),
    P1(Coupon<N1, S1>),
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
    type Opts = P1<State<F>>;
}
// {} (NONE)

trait Knowable: Advance {
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

struct States<S: Token, N: Token> {
    //        ^state    ^next
    phantom: PhantomData<(S, N)>,
}
impl<S: Token, N: Token> Token for States<S, N> {}

trait Collapsing: Token {
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

pub fn atomic(start: State<F>, mut p0: Port<N0>, mut p1: Port<N1>) -> ! {
    // let t: State<T> = NoData::fresh();
    // let t: <(State<T>, State<T>) as UnifyState>::Out;
    // let y: States<State<T>, States<State<T>, State<F>>> = NoData::fresh();
    // let z = y.collapse();
    // z == ();
    // t == ();
    // let s = start.un
    // let s: States<State<T>, State<X>> = NoData::fresh();
    // let s = s.collapse();
    // s == ();

    // let x = start;
    // let x = x.unify_with::<X>();
    // x==();

    // let f = start;
    // let mut x = f.weaken_a();
    // // x == ();
    // loop {
    // 	x = x.advance(|o| match o {
    // 		P0P1::P0(c) => p0.act(c).weaken_a(),
    // 		P0P1::P1(c) => p1.act(c).weaken_a(),
    // 	});
    // }

    unimplemented!()
    // unimplemented!()
    // loop {
    // 	let t = p0.act(f.only_coupon());
    // 	f = p1.act(t.only_coupon())
    // 	// let t = p0.act(f.only_coupon());
    // 	// let t = f.advance(|o| match o {
    // 	// 	P0::P0(c) => p0.act(c),
    // 	// });
    // 	// f = t.advance(|o| match o {
    // 	// 	P1::P1(c) => p1.act(c),
    // 	// });
    // }
}
