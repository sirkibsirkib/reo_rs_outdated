
use std::sync::Arc;
use std::marker::PhantomData;

type Id = usize;


trait Proto<T> {
	fn get(&self, id: Id) -> T;
	fn put(&self, id: Id, datum: T);
}


struct PortCommon<T> {
	id: Id,
	phantom: PhantomData<*const T>,
	proto: Arc<dyn Proto<T>>,
}

struct Getter<T>(PortCommon<T>);
impl<T> Getter<T> {
	fn get(&self) -> T {
		self.0.proto.get(self.0.id)
	}
}
struct Putter<T>(PortCommon<T>);
impl<T> Putter<T> {
	
} 