
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
	}

	pub fn inner(&self) -> &Arc<dyn Trait> {
		&self.x
	}

	pub fn hotswap<Q: 'static +  Trait, F>(self: Self, swap_fn: F) -> Writer<Q>
	where F: Fn(Box<T>) -> Box<Q> {
		use std::mem::{transmute, replace};
		// 1. downcast the Arc<dyn Trait> to the shardedlock you know it to be
		let lockref = self.x.borrow();
		let concrete_lockref = unsafe {
			&*(lockref as *const dyn Trait as *const ShardedLock<Box<dyn Trait>>)
		};
		// 2. lock it using a WRITER lock
		let mut locked = concrete_lockref.write().expect("WRITER POISONED");
		let trait_obj_here: &mut Box<dyn Trait> = &mut locked;
		// 3. use mem::replace to remove the current trait object and leave a
		//    temp Trait object in place (in case the closure panics).
		struct Temp;
		impl Trait for Temp {
			fn say(&self){}
		}
		let temp_trait_obj = Box::new(Temp) as Box<dyn Trait>;
		let old_trait_obj: Box<dyn Trait> = replace(trait_obj_here, temp_trait_obj);
		// 4. extract Box<T:Trait> from Box<dyn Trait>. vptr is static so safe to discard
		let (boxt, _vtbl): (Box<T>, *const ()) = unsafe { transmute(old_trait_obj) };
		
		// 5. call the user-provided swap fn to get the new Box<Q>
		let boxq = swap_fn(boxt);
		let _temp_box = replace(trait_obj_here, boxq as Box<dyn Trait>);
		// 6. drop temp box
		// 7. transmute to Memory<Q> (only 0-byte PhantomData differs)
		unsafe { transmute(self) }
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
impl Drop for A {
	fn drop(&mut self) {
		println!("ADROP");
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
			w.hotswap::<B,_>(|_| {panic!("YE");});
		});
	}).unwrap();
}

impl<T: Trait> Trait for ShardedLock<Box<T>> {
	fn say(&self) {
		self.read().expect("R POISONED").say()
	}
}