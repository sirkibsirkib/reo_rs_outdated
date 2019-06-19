Reo has memory cells. we all know how they function and change at runtime.
the question: how are they INITIALIZED?

Which of these can Reo express?
1. This value is initially empty
2. This value is initially full with THIS value
3. This value is initially full with ANY value
4. This value's initialization is user defined
5. This value is full or empty. but if full THIS value




--------------------
in either case:
1. nothing is exposed to the user. In memory its the Option::None type
2. the type acquires a FromStr bound
3. Instantiate() acquires a T field
4. Instantiate() acquires an Option<T> field
5. Instantiate() acquires a boolean flag and the type acquires a FromStr bound 

//FromStr is conventional for parsing. is in the prelude. has associated Err type.


-----------------------
# Open Questions:
1. Reo channels? how am I supposed to link that jazz? Seems like we want them to appear inside the generated proto struct. Maybe as a dependnecy of the reo proto object itself? How are they meant to integrate with the Proto object anyway?

2. What about this API tool? It's a real pain in the ass that you need two separate tools for this
WHY? just because we need to compute the API separately AND we need to ensure that we have the 
same protocol RBPA.

3. Entrypoint for the protocol object
	* Reduce the footprint of the ProtoDef. make it just statics? :D
	* Have the MyProto object (target of reo compiler) generate the proto memory buffer
	* ProtoMem buffer can be added to but it otherwise opaque. ie type ensures its internally consistent
	* ProtoMem object contains &'a ProtoDef so that the ProtoAll can CHECK it on creation.
	* ProtoAll objects can be constructed 

```rust
type FilledFIlled = bool;

#[derive(Default)]
struct ProtoInit {
	bytes: Vec<u8>,	
	contents: HashMap<usize, (TypeId, Filled)>
}
impl ProtoMemory {
	fn add_filled<T>(&mut self, t: T); // err if already contains something
	fn add_empty<T>(&mut self, t: T); // make use of MaybeUninit here.
}

struct Proto {
	pub fn new(mem: ProtoInit) -> Result<Self, ()>;
	// verifies that this memory structure contains the stuff we need
}



struct MyProto;
impl Proto for MyProto<T> {
	type MemoryInit: Sized = ();
	fn instantiate(init: Self::MemoryInit) -> ProtoHandle {
		let 
	}
}

```

<!-- 

design choice: the meat and potatoes of what runs at runtime is TYPE-ERASED
this is to deduplicate all the proto types. -->