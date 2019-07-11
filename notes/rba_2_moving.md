formula:
* boolterm(term),
* and(formula[]),
* or(formula[]),
* not(formula),
* eq(term, term),

Term:
* const(ptr)
* true,
* false,
* null,
* fn(funcptr, term),


CAN YOU ALWAYS CONVERT to runrule?
```rust
struct RunRule {
    guard_ready: BitSet,
    guard_full: BitSet, // full & ready mem means Some(full), !ful & ready means None(full)
	guard: formula,
	actions: Vec<Action>,
}
struct Action {
	putter: LocId,
	getters: Vec<LocId>,
}
```

some forms are obvious:
p0=g0 & p1=g1 & p1=g2 & m0=@

=>
guard: 