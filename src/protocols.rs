
use bit_set::BitSet;

pub struct Guard<'a> {
	pub ready_set: BitSet,
	pub action_fn: &'a (dyn Fn()),
	// TODO data constraint
}
impl<'a> Guard<'a> {
	pub fn new(ready_set: BitSet, action_fn: &'a (dyn Fn())) -> Self {
		Self {
			ready_set,
			action_fn,
		}
	}
}


macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = BitSet::new();
		$( s.insert($port); )*
		s
	}}
}