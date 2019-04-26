// /// 1
// use itertools::izip;
// use std::fmt;

// #[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
// struct StateId(usize);

// #[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
// struct VarId(usize);

// struct Rba {
//     rules: Vec<Rule>,
// }

// struct Rule {}

// #[derive(Clone, Default)]
// struct StateConstraint {
//     value: Vec<usize>,
//     known: Vec<usize>,
// }
// impl PartialEq for StateConstraint {
//     fn eq(&self, other: &Self) -> bool {
//         unimplemented!()
//     }
// }
// impl fmt::Debug for StateConstraint {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let l = self.value.len() * Self::BITS_PER_CHUNK;
//         for idx in 0..l {
//             let c = match self.get(idx) {
//                 Some(true) => '1',
//                 Some(false) => '0',
//                 None => '*',
//             };
//             write!(f, "{}", c)?;
//         }
//         Ok(())
//     }
// }
// impl StateConstraint {
//     const BITS_PER_CHUNK: usize = std::mem::size_of::<usize>() * 8;
//     fn cvt_inward(idx: usize) -> [usize; 2] {
//         [idx / Self::BITS_PER_CHUNK, idx % Self::BITS_PER_CHUNK]
//     }
//     pub fn set(&mut self, idx: usize, val: Option<bool>) {
//         let [major, minor] = Self::cvt_inward(idx);
//         while self.value.capacity() < major {
//             self.value.push(0);
//             self.known.push(0);
//         }
//         if let Some(boolean) = val {
//             if boolean {
//                 self.value[major] |= 1 << minor;
//             } else {
//                 self.value[major] ^= 1 << minor;
//             }
//             self.known[major] |= 1 << minor;
//         } else {
//             self.known[major] ^= 1 << minor;
//         }
//     }
//     pub fn get(&self, idx: usize) -> Option<bool> {
//         let [major, minor] = Self::cvt_inward(idx);
//         if (self.known[major] & 1 << minor) > 0 {
//             Some((self.value[major] & 1 << minor) > 0)
//         } else {
//             None
//         }
//     }
//     fn compatible(&self, them: &Self) -> bool {
//         let four_iter = izip!(&self.value, &self.known, &them.value, &them.known);
//         for (&v1, &k1, &v2, &k2) in four_iter {
//             let diff = v1 ^ v2;
//             let both_known = k1 & k2;
//             let disagree = both_known & diff;
//             if disagree != 0 {
//                 return false;
//             }
//         }
//         true
//     }
// }

// #[test]
// fn test_state_constraint() {

// }
use crate::decimal::*;

use std::marker::PhantomData;

struct State<A: Tern, B: Tern, C: Tern> {
    phantom: PhantomData<(A,B,C)>,
}
impl<A: Tern, B: Tern, C: Tern> Token for State<A,B,C> {}

// trait R1Yes {}
// trait R1No {}
// impl<A: MaybeT, B: MaybeT, C: Tern> R1Yes for State<A,B,C> {}
// impl<B: MaybeT, C: Tern> R1No for State<F,B,C> {}
// impl<A: MaybeT, C: Tern> R1No for State<A,F,C> {}


struct Coupon<N, T> {
    phantom: PhantomData<(N,T)>,
}

enum OptsR1<A: Tern, C: Tern> {
    R1(Coupon<N0, State<A,F,C>>),
}
impl<A: MaybeT, B: MaybeT, C: Tern> Advance for State<A,B,C> {
    type Match = OptsR1<A,C>;
}

// enum R1R2<A: Tern, B: Tern, C: Tern> {
//     R1(Coupon<N0, State<A,F,C>>),
//     R2(Coupon<N1, State<A,B,C>>),
// }

trait Advance: Sized {
    type Match;
    fn advance<F,R>(self, _func: F) -> R where F: FnOnce(Self::Match) -> R {
        // func()
        unimplemented!()
    }
}



impl<A: Tern, B: Tern, C: Tern> State<A,B,C> {
    fn fresh() -> Self {
        // private method!
        State {
            phantom: PhantomData::default(),
        }
    }
}
impl<A: Bin, B: Tern, C: Tern> State<A,B,C> {
    pub fn weaken_a(self) -> State<X, B, C> {
        State::fresh()
    }
} 
impl<A: Tern, B: Bin, C: Tern> State<A,B,C> {
    pub fn weaken_b(self) -> State<A, X, C> {
        State::fresh()
    }
} 
impl<A: Tern, B: Tern, C: Bin> State<A,B,C> {
    pub fn weaken_c(self) -> State<A, B, X> {
        State::fresh()
    }
} 

// struct ToCover<A> {
//     phantom: PhantomData<A>,
// }

trait Token {}

// PRIVATE
trait NoData: Token {
    fn fresh() -> Self;
}
impl<T: Token> NoData for T {
    fn fresh() -> Self {
        unsafe {std::mem::uninitialized()}
    }
}

trait Bin: Tern {}
impl Bin for T {}
impl Bin for F {}

trait Tern: NoData {}
impl Tern for T {}
impl Tern for F {}
impl Tern for X {}

trait Uncertain {}
impl Uncertain for X {}

trait MaybeT: Tern {}
impl MaybeT for T {}
impl MaybeT for X {}
trait MaybeF: Tern {}
impl MaybeF for F {}
impl MaybeF for X {}

impl Token for T {}
impl Token for F {}
impl Token for X {}

struct T;
struct F;
struct X;

struct Port<N> {
    phantom: PhantomData<N>,
}
impl<N> Port<N> {
    fn act<T: NoData>(&mut self, coupon: Coupon<N, T>) -> T {
        T::fresh()
    }
}

fn atomic(fff: State<F,F,F>, mut p0: Port<N0>, _p1: Port<N1>) -> ! {
    let x = fff.weaken_a().weaken_b().weaken_c().advance(|opt| match opt {
        OptsR1::R1(c) => {}, 
    });
    unimplemented!()
    // let mut xxx = start.weaken_a().weaken_b().weaken_c();
    // let mut clos = |opts| match opts {
    //     OptsR1::R1(coupon) => p0.act(coupon),
    // };
    // loop {
    //     xxx = xxx.advance(&mut clos).weaken_b();
    // }
}





