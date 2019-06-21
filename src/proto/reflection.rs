use super::*;

// an untyped CloneFn pointer. Null variant represents an undefined function
// which will cause explicit panic if execute() is invoked.
// UNSAFE if the type pointed to does not match the type used to instantiate the ptr.
#[derive(Debug, Copy, Clone)]
pub(crate) struct CloneFn(Option<NonNull<fn(*mut u8, *mut u8)>>);
impl CloneFn {
    fn new<T>() -> Self {
        let clos: fn(*mut u8, *mut u8) = |src, dest| unsafe {
            let datum = T::maybe_clone(transmute(src));
            let dest: &mut T = transmute(dest);
            *dest = datum;
        };
        let opt_nn = NonNull::new(unsafe { transmute(clos) });
        debug_assert!(opt_nn.is_some());
        CloneFn(opt_nn)
    }
    /// safe ONLY IF:
    ///  - src is &T to initialized memory
    ///  - dst is &mut T to uninitialized memory
    ///  - T matches the type provided when creating this CloneFn in `new_defined`
    #[inline]
    pub unsafe fn execute(self, src: *mut u8, dst: *mut u8) {
        if let Some(x) = self.0 {
            (*x.as_ptr())(src, dst);
        } else {
            panic!("proto attempted to clone an unclonable type!");
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct PartialEqFn(Option<fn(*mut u8, *mut u8) -> bool>);
impl PartialEqFn {
    fn new<T>() -> Self {
        let clos: fn(*mut u8, *mut u8) -> bool = |a, b| unsafe {
            let a: &T = transmute(a);
            a.maybe_partial_eq(transmute(b))
        };
        PartialEqFn(Some(clos))
    }
    #[inline]
    pub unsafe fn execute(self, a: *mut u8, b: *mut u8) -> bool {
        if let Some(x) = self.0 {
            (x)(a, b)
        } else {
            panic!("proto attempted to partial_eq a type for which its not defined!");
        }
    }
}

// an untyped DropFn pointer. Null variant represents a trivial drop Fn (no behavior).
// new() automatically handles types with trivial drop functions
// UNSAFE if the type pointed to does not match the type used to instantiate the ptr.
#[derive(Debug, Copy, Clone)]
pub(crate) struct DropFn(Option<fn(*mut u8)>);
impl DropFn {
    fn new<T>() -> Self {
        if std::mem::needs_drop::<T>() {
            let clos: fn(*mut u8) = |ptr| unsafe {
                let ptr: &mut ManuallyDrop<T> = transmute(ptr);
                ManuallyDrop::drop(ptr);
            };
            DropFn(Some(clos))
        } else {
            DropFn(None)
        }
    }
    /// safe ONLY IF the given pointer is of the type this DropFn was created for.
    #[inline]
    pub unsafe fn execute(self, on: *mut u8) {
        if let Some(x) = self.0 {
            (x)(on);
        } else {
            // None variant represents a drop with no effect
        }
    }
}

/// A structure used for type erasure. Describes the type in as much detail
/// that a memory cell needs to handle all the operations on it
#[derive(Debug, Clone, Copy)]
pub struct TypeInfo {
    pub(crate) type_id: TypeId,
    pub(crate) drop_fn: DropFn,
    pub(crate) clone_fn: CloneFn,
    pub(crate) partial_eq_fn: PartialEqFn,
    pub(crate) is_copy: bool,
    pub(crate) layout: Layout,
}
impl TypeInfo {
    pub fn get_tid(&self) -> TypeId {
        self.type_id
    }
    pub fn new<T: 'static>() -> Self {
        // always true: clone_fn.is_none() || !is_copy
        // holds because Copy trait is mutually exclusive with Drop trait.
        Self {
            type_id: TypeId::of::<T>(),
            drop_fn: DropFn::new::<T>(),
            clone_fn: CloneFn::new::<T>(),
            partial_eq_fn: PartialEqFn::new::<T>(),
            layout: Layout::new::<T>(),
            is_copy: <T as MaybeCopy>::IS_COPY,
        }
    }
}

#[test]
fn drops_ok() {
    #[derive(Debug)]
    struct Foo(u32);
    impl Drop for Foo {
        fn drop(&mut self) {
            println!("dropped Foo({}) !", self.0);
        }
    }

    let drop_fn = DropFn::new::<Foo>();

    let foo = Foo(2);
    let x1: *const Foo = &foo as *const Foo;
    let x2: *mut u8 = unsafe { transmute(x1) };
    println!("{:?}", (x1, x2));

    unsafe { drop_fn.execute(x2) };
    std::mem::forget(foo);
}
