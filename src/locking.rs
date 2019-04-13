
// use std::any::Any;
use std::marker::PhantomData;
use crossbeam::sync::ShardedLock;
use std::sync::Arc;
use std::borrow::Borrow;

// unsafe impl Send for Box<Trait + 'static> {}
trait Trait {
	fn say(&self);
}
// unsafe impl Send for (dyn Trait + 'static) {}


// trait AnyTrait: Any + Trait {}

unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}
unsafe impl<T: Trait> Send for Writer<T> {}
unsafe impl<T: Trait> Sync for Writer<T> {}
struct Reader {
	x: Arc<dyn Trait>,
}
struct Writer<T: Trait> {
	x: Arc<dyn Trait>,
	// type is actually Arc<ShardedLock<Box<T>>>
	phantom: PhantomData<*const T>,
}
impl<T: 'static +  Trait> Writer<T> {
	pub fn new(innermost: Box<T>) -> Self {
		let inner: ShardedLock<Box<dyn Trait>> = ShardedLock::new(innermost);
		let outer: Arc<dyn Trait> = Arc::new(inner);
		Writer {
			x: outer,
			phantom: PhantomData::default(),
		}
		// unimplemented!()
	}

	pub fn inner(&self) -> &Arc<dyn Trait> {
		&self.x
		// unimplemented!()
	}

	pub fn alter<F,Q: 'static +  Trait>(self: Self, swap_fn: F) -> Writer<Q>
	where F: Fn(Box<T>) -> Box<Q> {
		println!("HOTSWAP");
		unsafe {
			let y = self.x.borrow();
			let x = &*(y as *const dyn Trait as *const ShardedLock<Box<dyn Trait>>);
			let mut z = x.write().expect("WRITER POISONED");
			let trait_obj_here: &mut Box<dyn Trait> = &mut z;
			let out: Box<dyn Trait> = std::mem::replace(trait_obj_here, std::mem::uninitialized());
			let (boxt, _vtbl): (Box<T>, *const ()) = std::mem::transmute(out);

			let boxq = swap_fn(boxt);
			let new_trait_obj: Box<dyn Trait> = boxq;
			// z == ();
			// let new: Box<dyn Trait> = Box::new(q);
			let uninit = std::mem::replace(trait_obj_here, new_trait_obj);
			std::mem::forget(uninit);
			std::mem::transmute(self)
		}
	}
}

impl Trait for ShardedLock<Box<dyn Trait>> {
	fn say(&self) {
		self.read().unwrap().say()
	}
}
struct A(u32);
impl Trait for A {
	fn say(&self) {
		println!("A:{}", self.0);
	}
}
struct B(&'static str);
impl Trait for B {
	fn say(&self) {
		println!("B:{}", self.0);
	}
}

#[test]
fn foo() {
	let w = Writer::new(Box::new(A(4)));
	let r = Reader{x: w.inner().clone()};

	crossbeam::scope(|s| {
		s.spawn(move |_| for _ in 0..100 {
			r.x.say();
			std::thread::sleep(std::time::Duration::from_millis(100));
		});
		s.spawn(move |_| {
			std::thread::sleep(std::time::Duration::from_millis(3000));
			w.alter(|_| Box::new(B("BATCH")));
		});
	}).unwrap();
}

impl<T: Trait> Trait for ShardedLock<Box<T>> {
	fn say(&self) {
		self.read().expect("R POISONED").say()
	}
}