/*
Storage structure for OWNING allocation


Invariants:
1. integrity of bytes buffer always OK
2. for every value in `owned`, there is a key in `type_info`.
*/

use std::alloc;
use std::alloc::Layout;
use std::mem::MaybeUninit;

use crate::proto::reflection::TypeInfo;
use hashbrown::HashMap;
use std::any::TypeId;
use std::sync::Arc;

use std::hash::{Hash, Hasher};

// type DataLen = usize;
type StackPtr = *mut u8;
type StorePtr = *mut u8;

#[derive(Debug, Eq, PartialEq)]
struct LayoutHashable(Layout);
impl Hash for LayoutHashable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.align().hash(state);
        self.0.size().hash(state);
    }
}

#[derive(Debug, Default)]
pub struct Storage {
    free: HashMap<LayoutHashable, Vec<StorePtr>>,
    owned: HashMap<StorePtr, TypeId>,
    type_info: HashMap<TypeId, Arc<TypeInfo>>,
}
impl Storage {
    #[inline]
    pub fn move_value_in<T: 'static>(&mut self, mut value: T) -> StorePtr {
        let info = Arc::new(TypeInfo::new::<T>());
        let stored_ptr = unsafe {
            // SAFE. info type matches ptr type
            let stack_ptr: *mut u8 = std::mem::transmute(&mut value as *mut T);
            self.move_in(stack_ptr, &info)
        };
        std::mem::forget(value);
        stored_ptr
    }
    pub unsafe fn move_in(&mut self, src: StackPtr, type_info: &Arc<TypeInfo>) -> StorePtr {
        let dest = self.inner_alloc(type_info);
        type_info.move_fn_execute(src, dest);
        dest
    }
    pub unsafe fn clone_in(&mut self, src: StackPtr, type_info: &Arc<TypeInfo>) -> StorePtr {
        let dest = self.inner_alloc(type_info);
        type_info.clone_fn.execute(src, dest);
        dest
    }
    pub unsafe fn move_out(&mut self, src: StorePtr, dest: StackPtr, layout: &Layout) {
        std::ptr::copy(src, dest, layout.size());
        self.inner_free(src, &LayoutHashable(*layout));
    }
    pub unsafe fn drop_inside(&mut self, ptr: StorePtr, info: &Arc<TypeInfo>) {
        info.drop_fn.execute(ptr);
        self.inner_free(ptr, &LayoutHashable(info.layout));
    }
    /// Deallocates emptied allocations
    pub fn shrink_to_fit(&mut self) {
        for (layout_hashable, vec) in self.free.drain() {
            for ptr in vec {
                // println!("dropping (empty) alloc at {:p}", ptr);
                unsafe { alloc::dealloc(ptr, layout_hashable.0) }
            }
        }
    }
    ///////////////////
    unsafe fn inner_free(&mut self, ptr: StorePtr, lh: &LayoutHashable) {
        self.owned.remove(&ptr).expect("not owned?");
        self.free
            .get_mut(&lh)
            .expect("not prepared for this len")
            .push(ptr);
    }
    unsafe fn inner_alloc(&mut self, type_info: &Arc<TypeInfo>) -> StorePtr {
        // println!("move_ining.. looking for a free space...");
        // preserve the invariant
        self.type_info
            .entry(type_info.type_id)
            .or_insert_with(|| type_info.clone());
        let dest = self
            .free
            .entry(LayoutHashable(type_info.layout))
            .or_insert_with(Vec::new)
            .pop()
            .unwrap_or_else(|| {
                // println!("allocating with layout {:?}", &type_info.layout);
                alloc::alloc(type_info.layout)
            });
        if let Some(_) = self.owned.insert(dest, type_info.type_id) {
            panic!("move_in allocated something already owned??")
        }
        dest
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        // println!("DROPPING");
        for (ptr, tid) in self.owned.drain() {
            // invariant: self.owned keys ALWAYS are mapped in type_info
            let info = self.type_info.get(&tid).unwrap();
            unsafe {
                info.drop_fn.execute(ptr);
                // println!("dropping occupied alloc at {:p}", ptr);
                alloc::dealloc(ptr, info.layout);
            };
        }
        self.shrink_to_fit();
        // self.frees is necessarily empty now
    }
}

#[test]
fn memtest() {
    #[derive(Debug)]
    struct Foo {
        x: [usize; 3],
    }
    impl Drop for Foo {
        fn drop(&mut self) {
            println!("dropping foo {:?}", self.x);
        }
    }

    let mut storage = Storage::default();

    let mut a: MaybeUninit<Foo> = MaybeUninit::new(Foo { x: [1, 2, 3] });
    let mut b: MaybeUninit<Foo> = MaybeUninit::uninit();
    println!("A=[1,2,3], B=?, []");
    let info = Arc::new(TypeInfo::new::<Foo>());

    unsafe {
        let ra = std::mem::transmute(a.as_mut_ptr());
        let rb = std::mem::transmute(b.as_mut_ptr());

        let rc = storage.move_in(ra, &info);
        println!("A=[1,2,3], B=?, [C=[1,2,3]]");

        storage.move_out(rc, rb, &info.layout);
        println!("A=[1,2,3], B=[1,2,3], []");

        println!("PRINTING: A:{:?}, B:{:?}", a.assume_init(), b.assume_init());
    };
}
