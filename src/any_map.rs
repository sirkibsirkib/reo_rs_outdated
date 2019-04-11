use std::any::TypeId;
use hashbrown::HashMap;
use std::mem::transmute;

type Key = usize;
type Offset = isize;
type UntypedPtr = *const u8;
struct ProtoMemory {
	storage: Vec<u8>,
	drop_funcs: HashMap<TypeId, fn(UntypedPtr)>,
	entries: HashMap<Key, (Offset, TypeId)>,
}
impl ProtoMemory {
	pub unsafe fn access(&self, tid: TypeId, key: Key) -> (bool, UntypedPtr) {
		let (offset, tid2) = self.entries.get(&key).expect("BAD KEY");
		assert_eq!(tid, *tid2);
		let entry_ptr = self.storage.as_ptr().offset(*offset);
		let bool_ptr: &bool = transmute(entry_ptr);
		let occupied = *bool_ptr;
		let datum_ptr = entry_ptr.offset(1);
		(occupied, datum_ptr)
	}
	pub unsafe fn set_occupied(&self, key: Key, occupied: bool) {
		let (offset, _tid) = self.entries.get(&key).expect("BAD KEY");
		let entry_ptr = self.storage.as_ptr().offset(*offset);
		let bool_ptr: &mut bool = transmute(entry_ptr);
		*bool_ptr = occupied;
	}
}
impl Drop for ProtoMemory {
	fn drop(&mut self) {
		for (_k, (offset, tid)) in self.entries.iter() {
			let f = self.drop_funcs.get(tid).expect("NO DROP FN WTF");
			unsafe {
				let entry_ptr = self.storage.as_ptr().offset(*offset);
				let bool_ptr: &bool = transmute(entry_ptr);
				if *bool_ptr {
					// entry is occupied
					let datum_ptr = entry_ptr.offset(1);
					f(transmute(datum_ptr));
				}
			};
		}
	}
}

struct ProtoBuilder {
	mem: ProtoMemory,
}
impl ProtoBuilder {
	pub fn new() -> Self {
		Self {
			mem: ProtoMemory {
				storage: vec![],
				drop_funcs: HashMap::default(),
				entries: HashMap::default(),
			}
		}
	}
	pub fn allocate<T: 'static>(mut self, key: Key, init: Option<T>, drop_fn: fn(&mut T)) -> Result<Self, AllocationError> {
		use AllocationError::*;
		if self.mem.entries.contains_key(&key) {
			return Err(RepeatedKey);
		}
		let bytes = std::mem::size_of::<T>() + 1; // first byte is BOOL FLAG
		self.mem.storage.extend(std::iter::repeat(0).take(bytes));
		let tid = TypeId::of::<T>();
		let new_f: fn(UntypedPtr) = unsafe {
			std::mem::transmute(drop_fn)
		};
		match self.mem.drop_funcs.get(&tid) {
			Some(f) => if f != &new_f {return Err(DropFnInconsistent)},
			None => { self.mem.drop_funcs.insert(tid, new_f); },
		}
		self.mem.entries.insert(key, (self.mem.storage.len() as isize, tid));
		match init {
			Some(x) => {

			},
			None => {},
		}
		Ok(self)
	}
	pub fn finish(self) -> ProtoMemory {
		self.mem
	}
}

#[derive(Debug, Copy, Clone)]
pub enum AllocationError {
	DropFnInconsistent,
	RepeatedKey,
}


#[test]
fn anymap_test() {
	struct Whatever;
	impl Drop for Whatever {
		fn drop(&mut self) {
			println!("WHATEVER DROPPED");
		}
	}

	let m = ProtoBuilder::new().allocate(0, Some(Whatever), |_| ()).unwrap().finish();
	println!("YE");
}