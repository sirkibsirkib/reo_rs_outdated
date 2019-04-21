
mod num {
	pub struct N0;
	// ...
}
use num::*;

mod rba {
    use std::marker::PhantomData;

    pub struct Unknown;
    trait Var: Sized {}

    pub struct True; impl Var for True {}
    pub struct False; impl Var for False {}

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

        pub fn weaken_1(self) -> Mem2<Unknown, K2> {
            Mem2::fresh()
        }
        pub fn weaken_2(self) -> Mem2<K1, Unknown> {
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
}

#[test]
pub fn rba_like() {
    use rba::*;

    trait RbaOpts: Sized {
        type K1;
        type K2;
        fn advance<F,R>(req: Mem2<Self::K1, Self::K2>, handler: F)
        -> R where F: FnOnce(Self)->R;
    }

    enum Opts0 {
        P0Vff(Payment<N0, Mem2<False, False>>),
        P0Vft(Payment<N0, Mem2<False, True>>),
        P0Vuu(Payment<N0, Mem2<Unknown, Unknown>>),
    }

    impl RbaOpts for Opts0 {
        type K1 = Unknown;
        type K2 = Unknown;
        fn advance<F,R>(req: Mem2<Self::K1, Self::K2>, handler: F)
        -> R where F: FnOnce(Self)->R {
            let _ = req;
            let decision = if std::mem::size_of::<Opts0>() == 0 {
                // 0 or 1 variants
                unsafe {std::mem::uninitialized()}
            } else {
                // TODO interact with proto to determine outcome
                Opts0::P0Vff(Payment::fresh())
            };
            handler(decision)
        }
    }

    fn atomic(start: Mem2<True, Unknown>, mut porty: Porty<N0, u32>) -> ! {
        let mut m = start.weaken_1();
        loop {
            m = Opts0::advance(m, |opts| match opts {
                Opts0::P0Vff(payment) => porty.put(payment, 1).weaken_1().weaken_2(),
                Opts0::P0Vft(payment) => porty.put(payment, 2).weaken_1().weaken_2(),
                Opts0::P0Vuu(payment) => porty.put(payment, 3),
            });
        }
    }

    atomic(Mem2::fresh(), Porty::fresh())
}