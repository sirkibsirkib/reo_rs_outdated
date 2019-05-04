use std::marker::PhantomData;
struct T;
struct F;

trait Nand {}
impl Nand for F {}
impl Nand for Neg<T> {}
// left is NAND
impl<A:Nand> Nand for (A, T) {}
impl<A:Nand> Nand for (A, Neg<F>) {}
// right is NAND
impl<B:Nand> Nand for (T,      B) {}
impl<B:Nand> Nand for (Neg<F>, B) {}
// both are NAND
impl<A:Nand, B:Nand> Nand for (A, B) {}

struct Neg<T: Var> {
    phantom: PhantomData<T>,
}

trait Var {}
impl Var for T {}
impl Var for F {}

trait Advance: Sized {
    type Opts;
    fn advance<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self::Opts) -> R,
    {
        f(unsafe { std::mem::uninitialized() })
    }
}

impl<A: Var, B: Var, C: Var> Advance for (A, B, C)
where
    (A, (Neg<B>, Neg<C>)): Nand,
{
    type Opts = u32;
}

impl Advance for (T, F, F) where {
    type Opts = String;
}

#[test]
fn test_call() {
    test()
}
pub fn test() {
    let x: (T, T, F) = unsafe { std::mem::uninitialized() };
    let r = x.advance(|o| o);
    r == 5;
}
