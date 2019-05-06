1. reo.rs
	1. proto trait, Getter, Putter, put(), get() etc.

2. reo compiler backend. generates foo_proto.rs dependent on reo.rs

(foo_proto.rs)
	protocol RBPA
	has begin() function which 

3. reo_app_builder.rs
	depends on reo.rs. 


4. reo_api_builder.rs
	given foo_proto.rs
	projects and normalizes to produce bar_atomic.rs (as a stub)


################# 
questions: interaction between CLI tools? Or one lib _depending_ on another?
1. Rbpa must be communicated from `foo_proto.rs` to `reo_api_builder.rs `
2. `reo_api_builder.rs` can be:
-- CLI that finds functions in `foo_proto.rs` and `bar_atomic.rs` and creates `main.rs`
-- entirely absent. 
```rust
let (a,b,c) = foo.new();
thread::spawn(|| ):

```

can we embed everything in reo.rs and foo_proto.rs?
1. main is unnecessary if we just expose ports
2. the proto itself has a begin_safe() function that allows you to call it in a new way
```rust

fn producer(a: Putter) -> ! {
	loop { a.put(5) }
}
fn consumer(b: Getter) -> ! {
	loop { println!("{}", b.get()) }
}

fn atomic_bc(mut state: State::Start, b: Safe<Getter>, c: Safe<Getter>) -> ! {
	loop {
		state = state
		.advance_only(|o| println!("{}", b.get(o)))
		.advance_only(|o| println!("{}", c.get(o)));
	}
}

let (a,b,c) = MyProto::new();
thread::spawn(move || producer(a));
thread::spawn(move || consumer(b));
thread::spawn(move || consumer(c));

let (a,b,c) = MyProto::new();
thread::spawn(move || producer(a));
thread::spawn(move || AtomicBc::run(b, c, atomic_bc));
```

