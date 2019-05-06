use crate::decimal::*;

use std::marker::PhantomData;

pub struct Unk;
trait Var: Sized {}

pub struct Tru; impl Var for Tru {}
pub struct Fal; impl Var for Fal {}
impl Into<Unk> for Tru {
    fn into(self) -> Unk {Unk}
}
impl Into<Unk> for Fal {
    fn into(self) -> Unk {Unk}
}

pub struct Coupon<N, Z> {
    phantom: PhantomData<(N, Z)>
}
impl<N,Z> Coupon<N,Z> {
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
    pub fn put<K1,K2>(&mut self, coupon: Coupon<N, Mem2<K1,K2>>, datum: T) -> Mem2<K1,K2> {
        let _ = coupon;
        let _ = datum;
        Mem2::fresh()
    }
}

//////////////////// CONCRETE ///////////////



enum OptsVuu {
    P0Vff(Coupon<N0, Mem2<Fal, Fal>>),
}
impl OptsVuu {
    fn advance<F,R,K1,K2>(req: Mem2<K1, K2>, handler: F)
    -> R where F: FnOnce(Self)->R {
        let _ = req;
        let decision = OptsVuu::P0Vff(Coupon::fresh());
        handler(decision)
    }
}


enum OptsVxf<K1> {
    P0Vtu(Coupon<N0, Mem2<Tru, Unk>>),
    P0Vxf(Coupon<N0, Mem2<K1, Fal>>),
}
impl<K1> OptsVxf<K1> {
    fn advance<F,R>(req: Mem2<K1, Fal>, handler: F)
    -> R where F: FnOnce(Self)->R {
        let _ = req;
        let decision = if std::mem::size_of::<Self>() == 0 {
            // unsafe {std::mem::uninitialized()}
            unreachable!()
        } else {
        	// some bogus runtime check
        	if unsafe {std::mem::transmute::<_,usize>(&req as *const Mem2<K1,Fal>)} & 0b1000 == 0 {
	        	OptsVxf::P0Vtu(Coupon::fresh())
	        } else {
	        	OptsVxf::P0Vxf(Coupon::fresh())
	        }
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
            OptsVuu::P0Vff(coupon) => porty.put(coupon, 1),
        });
        uu = OptsVxf::advance(ff, |opts| match opts {
            OptsVxf::P0Vtu(coupon) => porty.put(coupon, 2).weaken_1(),
            OptsVxf::P0Vxf(coupon) => porty.put(coupon, 3).weaken_1().weaken_2(),
        });
    }
}

// pub fn whee() {
// 	let x = 6;
// 	match x {
// 		6 => println!("hey"),
// 		_ => {},
// 	}
// }