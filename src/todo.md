#TODO
1. make sure the "vanilla" implemention is behaving as expected
1. 


```rust

struct ActionDef {
	putter: PortId,
	getters: Vec<PortId>,
}
struct RuleDef {
	actions: Vec<ActionDef>, 
	fire_fn: fn(&ProtoR, &mut ProtoActive),
}  
```