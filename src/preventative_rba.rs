
mod num {
	pub struct N0;
	// ...
}
use num::*;

use std::marker::PhantomData;

pub struct Unk;
trait Var: Sized {}

pub struct Tru; impl Var for Tru {}
pub struct Fal; impl Var for Fal {}

pub struct Payment<N, Z> {
    phantom: PhantomData<(N, Z)>
}
impl<N,Z> Payment<N,Z> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
} 

pub struct Mem2<K1, K2> {
    phantom: PhantomData<(K1, K2)>
}
impl<K1,K2> Mem2<K1,K2> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }

    pub fn weaken_1(self) -> Mem2<Unk, K2> {
        Mem2::fresh()
    }
    pub fn weaken_2(self) -> Mem2<K1, Unk> {
        Mem2::fresh()
    }
}

pub struct Porty<N, T> {
    phantom: PhantomData<(N,T)>,
}
impl<N,T> Porty<N,T> {
    pub(crate) fn fresh() -> Self {
        Self { phantom: PhantomData::default() }
    }
    pub fn put<K1,K2>(&mut self, payment: Payment<N, Mem2<K1,K2>>, datum: T) -> Mem2<K1,K2> {
        let _ = payment;
        let _ = datum;
        Mem2::fresh()
    }
}

//////////////////// CONCRETE ///////////////



enum OptsVuu {
    P0Vff(Payment<N0, Mem2<Fal, Fal>>),
}
impl OptsVuu {
    fn advance<F,R,K1,K2>(req: Mem2<K1, K2>, handler: F)
    -> R where F: FnOnce(Self)->R {
        let _ = req;
        let decision = if std::mem::size_of::<OptsVuu>() == 0 {
            unsafe {std::mem::uninitialized()}
        } else {
            unreachable!()
        };
        handler(decision)
    }
}


enum OptsVuf {
    P0Vtu(Payment<N0, Mem2<Tru, Unk>>),
}
impl OptsVuf {
    fn advance<F,R,V1>(req: Mem2<V1, Fal>, handler: F)
    -> R where F: FnOnce(Self)->R {
        let _ = req;
        let decision = if std::mem::size_of::<OptsVuf>() == 0 {
            unsafe {std::mem::uninitialized()}
        } else {
            unreachable!()
        };
        handler(decision)
    }
}

#[test]
pub fn rba_like() {
    atomic(Mem2::fresh(), Porty::fresh())
}

fn atomic(start: Mem2<Tru, Unk>, mut porty: Porty<N0, u32>) -> ! {
    let mut uu = start.weaken_1();
    loop {
    	let ff = OptsVuu::advance(uu, |opts| match opts {
            OptsVuu::P0Vff(payment) => porty.put(payment, 1),
        });
        uu = OptsVuf::advance(ff, |opts| match opts {
            OptsVuf::P0Vtu(payment) => porty.put(payment, 1).weaken_1()
        });
    }
}