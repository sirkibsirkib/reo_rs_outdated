use std::marker::PhantomData;
struct T;
struct F;

// transforms an n-ary tuple into nested binary tuples. 
// (a,b,c,d) => (a,(b,(c,d)))
// (a,b) => (a,b)
// () => ()
macro_rules! nest {
	() => {()};
    ($single:ty) => { $single };
    ($head:ty, $($tail:ty),*) => {
        ($head, nest!($( $tail),*))
    };
}

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
	nest!(A, Neg<B>, Neg<C>): Nand,
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
    let x: (T, F, F) = unsafe { std::mem::uninitialized() };
    let r = x.advance(|o| o);
}


pub struct Wang;
impl Wang {
	pub fn foo(&self) {

	} 
}

//wrapper type
pub struct Safe<T>(T);
impl Safe<Wang> {
	pub fn safe_foo(&self) {
		self.0.foo();
	}
}

pub fn tyes() {
	let x = Wang;
	x.foo();
	let y = Safe(Wang);
	y.safe_foo();
}
