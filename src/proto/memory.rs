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

#[derive(Debug, derive_new::new)]
struct Storage {
    #[new(default)]
    free: HashMap<LayoutHashable, Vec<StorePtr>>,

    #[new(default)]
    owned: HashMap<StorePtr, TypeId>,

    type_info: Arc<HashMap<TypeId, Arc<TypeInfo>>>,
}
impl Storage {
    pub unsafe fn insert(&mut self, src: StackPtr, type_info: &TypeInfo) -> StorePtr {
    	println!("inserting.. looking for a free space...");
        let dest = self
            .free
            .entry(LayoutHashable(type_info.layout))
            .or_insert_with(Vec::new)
            .pop()
            .unwrap_or_else(|| {
                println!("allocating with layout {:?}", &type_info.layout);
                alloc::alloc(type_info.layout)
            });

        if let Some(_) = self.owned.insert(dest, type_info.type_id) {
            panic!("insert allocated something already owned??")
        }

        std::ptr::copy_nonoverlapping(src, dest, type_info.layout.size());
        println!("OK INSERTED {:p}", dest);
        dest
    }
    pub unsafe fn move_out<T>(&mut self, src: StorePtr, dest: StackPtr) {
        let layout = Layout::new::<T>();
        std::ptr::copy_nonoverlapping(src, dest, layout.size());
        self.free(src, &LayoutHashable(layout));
    }
    pub unsafe fn drop_inside(&mut self, ptr: StorePtr, info: &TypeInfo) {
        info.drop_fn.execute(ptr);
        self.free(ptr, &LayoutHashable(info.layout));
    }
    unsafe fn free(&mut self, ptr: StorePtr, lh: &LayoutHashable) {
        self.owned.remove(&ptr).expect("not owned?");
        self.free
            .get_mut(&lh)
            .expect("not prepared for this len")
            .push(ptr);
    }

    /// Deallocates emptied allocations
    pub fn shrink_to_fit(&mut self) {
        for (layout_hashable, vec) in self.free.drain() {
            for ptr in vec {
            	println!("dropping (empty) alloc at {:p}", ptr);
                unsafe { alloc::dealloc(ptr, layout_hashable.0) }
            }
        }
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        println!("DROPPING");
        for (ptr, tid) in self.owned.drain() {
            // invariant: self.owned keys ALWAYS are mapped in type_info
            let info = self.type_info.get(&tid).unwrap();
            unsafe {
                info.drop_fn.execute(ptr);
                println!("dropping occupied alloc at {:p}", ptr);
                alloc::dealloc(ptr, info.layout);
            };
        }
        self.shrink_to_fit();
        // self.frees is necessarily empty now
    }
}

#[derive(Debug)]
struct Foo {
    x: [usize; 3],
}
impl Drop for Foo {
    fn drop(&mut self) {
        println!("dropping foo {:?}", self.x);
    }
}

#[test]
fn memtest() {
    let info_map = Arc::new(type_info_map![Foo]);
    let mut storage = Storage::new(info_map);

    let mut a: MaybeUninit<Foo> = MaybeUninit::new(Foo { x: [1,2,3] });
    let mut b: MaybeUninit<Foo> = MaybeUninit::uninit();
	println!("A=[1,2,3], B=?, []");
    let info = TypeInfo::new::<Foo>();

    unsafe {
    	let ra = std::mem::transmute(a.as_mut_ptr());
    	let rb = std::mem::transmute(b.as_mut_ptr());

    	let rc = storage.insert(ra, &info);
		println!("A=[1,2,3], B=?, [C=[1,2,3]]");

    	storage.move_out::<Foo>(rc, rb);
		println!("A=[1,2,3], B=[1,2,3], []");

		println!("PRINTING: A:{:?}, B:{:?}", a.assume_init(), b.assume_init());
    };
}
