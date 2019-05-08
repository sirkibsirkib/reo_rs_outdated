#[macro_export]
macro_rules! id_iter {
    ($($id:expr),*) => {
        [$( $id, )*].iter().cloned()
    };
}


#[macro_export]
macro_rules! finalize_ports {
    ($commons:expr => $($struct:path),*) => {
        (
            $(
                $struct($commons.next().unwrap(), Default::default()),
            )*
        )
    }
}

// transforms an n-ary tuple into nested binary tuples.
// (a,b,c,d) => (a,(b,(c,d)))
// (a,b) => (a,b)
// () => ()
#[macro_export]
macro_rules! nest {
    () => {()};
    ($single:ty) => { $single };
    ($head:ty, $($tail:ty),*) => {
        ($head, nest!($( $tail),*))
    };
}

#[macro_export]
macro_rules! milli_sleep {
    ($millis:expr) => {{
        std::thread::sleep(std::time::Duration::from_millis($millis));
    }};
}

#[macro_export]
macro_rules! bitset {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(bitset!(@single $rest)),*]));

    ($($value:expr,)+) => { bitset!($($value),+) };
    ($($value:expr),*) => {
        {
            let _countcap = bitset!(@count $($value),*);
            let mut _the_bitset = crate::bitset::BitSet::with_capacity(_countcap);
            $(
                let _ = _the_bitset.set($value);
            )*
            _the_bitset
        }
    };
}

#[macro_export]
macro_rules! map {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(map!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { map!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = map!(@count $($key),*);
            let mut _map = hashbrown::HashMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}
