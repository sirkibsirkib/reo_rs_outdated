use bit_set::BitSet;
use hashbrown::HashMap;

macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = BitSet::new();
		$( s.insert($port); )*
		s
	}}
}

macro_rules! map {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(map!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { map!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = map!(@count $($key),*);
            let mut _map = HashMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}