
use std::any::Any;
use std::marker::PhantomData;
use crossbeam::sync::ShardedLock;
use std::sync::Arc;
use std::borrow::Borrow;

trait Trait: Any {
	fn say(&self);
}

// trait AnyTrait: Any + Trait {}

struct Reader {
	x: Arc<dyn Trait>,
}
struct Writer<T: Trait> {
	x: Arc<dyn Any>,
	// type is actually Arc<ShardedLock<Box<T>>>
	phantom: PhantomData<*const T>,
}
impl<T: 'static +  Trait> Writer<T> {
	pub fn new(innermost: T) -> Self {
		let inner: ShardedLock<Box<dyn Trait>> = ShardedLock::new(Box::new(innermost));
		let outer: Arc<dyn Any> = Arc::new(inner);
		Writer {
			x: outer,
			phantom: PhantomData::default(),
		}
		// unimplemented!()
	}

	pub fn inner(&self) -> &Arc<dyn Trait> {
		// &self.x
		unimplemented!()
	}

	pub fn alter<Q: 'static +  Trait>(self: Self, q: Q) -> Writer<Q> {

		if let Some(x) = Any::downcast_ref::<ShardedLock<Box<dyn Trait>>>(self.x.borrow()) {
		// if let Some(x) = self.x.downcast_ref::<ShardedLock<Box<dyn Trait>>>() { // WORKS??
			let mut locked = x.write().expect("POIS");
			let mut new: Box<dyn Trait> = Box::new(q);
			let old: &mut Box<dyn Trait> = &mut locked;
			let _prev = std::mem::swap(old, &mut new);
			//prev dropped

		} else {
			panic!("DIDNT WORK")
		}
		unsafe {
			std::mem::transmute(self)
		}
	}
}

impl Trait for ShardedLock<Box<dyn Trait>> {
	fn say(&self) {
		self.read().unwrap().say()
	}
} 

struct A;
impl Trait for A {
	fn say(&self) {
		println!("A");
	}
}
struct B;
impl Trait for B {
	fn say(&self) {
		println!("B");
	}
}

#[test]
fn foo() {
	let w = Writer::new(A);

	w.alter(B);
}

impl<T: Trait> Trait for ShardedLock<Box<T>> {
	fn say(&self) {
		self.read().expect("R POISONED").say()
	}
}