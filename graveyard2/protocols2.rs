#[macro_export]
macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = bit_set::BitSet::new();
		$( s.insert($port); )*
		s
	}}
}
