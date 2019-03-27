use crate::reo::PortClosed;
use bit_set::BitSet;
use hashbrown::HashMap;

macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = BitSet::new();
		$( s.insert($port); )*
		s
	}}
}

macro_rules! tok_bitset {
    ($( $tok:expr ),*) => {{
        let mut s = BitSet::new();
        $( s.insert($tok.inner()); )*
        s
    }}
}

macro_rules! def_consts {
    ($offset:expr =>) => {{};};
    ($offset:expr => $e:ident) => {
        const $e: usize = $offset;
    };
    ($offset:expr => $e:ident, $($es:ident),+) => {
        const $e: usize = $offset;
        def_consts!($offset+1 => $($es),*);
    };
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

macro_rules! guard_cmd {
    ($guards:ident, $firing:expr, $data_con:expr, $fire_func:expr) => {
        let data_con = $data_con;
        let fire_func = $fire_func;
        let g: (
            BitSet,
            &(dyn Fn(&mut _) -> bool),
            &(dyn Fn(&mut _) -> Result<(), PortClosed>),
        ) = ($firing, &data_con, &fire_func);
        $guards.push(g);
    };
}

macro_rules! ready_set {
    ($guard:expr) => {
        $guard.0
    };
}
macro_rules! data_constraint {
    ($guard:expr) => {
        $guard.1
    };
}
macro_rules! action_cmd {
    ($guard:expr) => {
        $guard.2
    };
}
