

/*
Storage structure for OWNING allocation


Invariants:
1. integrity of bytes buffer always OK
2. for every value in `owned`, there is a key in `type_info`.
*/

use std::mem::MaybeUninit;
use std::mem::ManuallyDrop;
use hashbrown::HashMap;
use std::sync::Arc;
use crate::proto::reflection::TypeInfo;
use std::any::TypeId;

type DataLen = usize;
type StackPtr = *mut u8;
type StorePtr = *mut u8;


#[derive(Debug)]
struct Storage {
	bytes: Vec<u8>, // fixed length
	free: HashMap<DataLen, Vec<StorePtr>>,
	owned: HashMap<StorePtr, TypeId>,
	type_info: Arc<HashMap<TypeId, Arc<TypeInfo>>>,
}
impl Storage {
	pub fn new(sizes: Vec<(TypeId, usize)>, type_info: Arc<HashMap<TypeId, Arc<TypeInfo>>>) -> Result<Self, TypeId> {
		let mut cap = 0;
		for (type_id, count) in sizes.iter().copied() {
			if let Some(type_info) = type_info.get(&type_id) {
				let a = type_info.align;
				assert_eq!(0, type_info.bytes % a);
				assert!(a == 1 || a == 2 || a == 4 || a == 8);
				let remainder = cap % a;
				if remainder > 0 {
					cap += a - remainder;
				}
				cap += type_info.bytes * count;
			} else {
				return Err(type_id)
			}
		}
		let mut bytes = Vec::with_capacity(cap + std::mem::size_of::<usize>());
		assert_eq!(0, bytes.as_ptr() as usize % std::mem::size_of::<usize>());
		let mut free = HashMap::default();
		for (type_id, count) in sizes.iter().copied() {
			for _ in 0..count {
				let type_info = type_info.get(&type_id).unwrap();
				let ptr = Self::alloc_in_buffer(&mut bytes, type_info);
				free.entry(type_info.bytes).or_insert_with(|| vec![]).push(ptr);
			}
		}
		Ok(Self {
			bytes,
			free,
			owned: Default::default(),
			type_info,
		})
	}
	pub unsafe fn insert(&mut self, src: StackPtr, type_info: &TypeInfo) -> StorePtr {
		println!("inserting");
		let dest: StorePtr =
			self.free.get_mut(&type_info.bytes)
			.and_then(Vec::pop)
			.unwrap_or_else(|| Self::alloc_in_buffer(&mut self.bytes, type_info));
		let was = self.owned.insert(dest, type_info.type_id);
		assert!(was.is_none());
		std::ptr::copy_nonoverlapping(src, dest, type_info.bytes);
		dest
	}
	fn alloc_in_buffer(bytes: &mut Vec<u8>, type_info: &TypeInfo) -> StorePtr {
		println!("allocating in buffer..");
		let mut ptr = unsafe { // we only rely on the vector being consistent
			bytes.as_mut_ptr().offset(bytes.len() as isize)
		};
		let offset = ptr.align_offset(type_info.align);
		let new_size = if offset > 0 {
			ptr = unsafe { // offset will only be as small as the type
				ptr.offset(offset as isize)
			};
			bytes.len() + type_info.bytes + offset
		} else {
			bytes.len() + type_info.bytes
		};
		assert!(bytes.capacity() >= new_size);
		bytes.resize(new_size, 0u8);
		println!("buffer is now {:?} bytes", bytes.len());
		ptr
	}
	pub unsafe fn move_out<T>(&mut self, src: StorePtr, dest: StackPtr) {
		let bytes = std::mem::size_of::<T>();
		std::ptr::copy_nonoverlapping(src, dest, bytes);
		self.free(src, bytes);
	}
	pub unsafe fn drop_inside(&mut self, ptr: StorePtr, info: &TypeInfo) {
		info.drop_fn.execute(ptr);
		self.free(ptr, info.bytes);
	}
	unsafe fn free(&mut self, ptr: StorePtr, bytes: DataLen) {
		self.owned.remove(&ptr).expect("not owned?");
		self.free.get_mut(&bytes).expect("not prepared for this len").push(ptr);
	}
}
impl Drop for Storage {
	fn drop(&mut self) {
		for (&ptr, tid) in self.owned.iter() {
			println!("dropping {:p}", ptr);
			let info = self.type_info.get(tid).expect("unknown type!");
			unsafe { 
				info.drop_fn.execute(ptr)
			};
		}
	}
}


#[derive(Debug)]
struct Foo { x: [usize;3] }
impl Drop for Foo {
	fn drop(&mut self) {
		println!("dropping foo {:?}", self.x);
	}
}

#[test]
fn memtest() {
	let info_map = Arc::new(type_info_map![Foo]);
	let elements = vec![
		(TypeId::of::<Foo>(), 1)
	];
	let mut storage = Storage::new(elements, info_map).expect("BAD");
	let src = deeper(&mut storage);


	let mut x: MaybeUninit<Foo> = MaybeUninit::uninit();
	let dest: *mut u8 = unsafe {
		std::mem::transmute(x.as_mut_ptr())	
	};

	unsafe {
		storage.move_out::<Foo>(src, dest)
	};
	let x = unsafe { x.assume_init() };
	println!("x {:?}", x);
}

fn deeper(storage: &mut Storage) -> StorePtr {
	let mut y: Foo = Foo { x: [0,1,2] };
	let ptr: *mut Foo = &mut y;
	let ptr: *mut u8 = unsafe { std::mem::transmute(ptr) };
	let src = unsafe {
		storage.insert(ptr, &TypeInfo::new::<Foo>())
	};
	unsafe {
		std::mem::forget(y);
	}
	src
}