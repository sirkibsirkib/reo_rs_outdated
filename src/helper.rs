#[macro_export]
macro_rules! id_iter {
    ($($id:expr),*) => {
        [$( $id, )*].iter().cloned()
    };
}

#[macro_export]
macro_rules! new_rule_def {
    ($guard_pred:expr ;    $( $p:tt => $(  $g:tt ),*   );* ) => {{
        RuleDef {
            guard_pred: $guard_pred,
            actions: vec![
                $(ActionDef {
                    putter: $p,
                    getters: &[$($g),*],
                }),*
            ],
        }
    }}
}

#[macro_export]
macro_rules! putters_getters {
    ($__arc_p:expr => $($id:tt),* ) => {{
        (
            $(
                $__arc_p.claim($id).try_into().expect("BAD CLAIM")
            ),*
        )
    }}
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
macro_rules! type_info_map {
    ( $( $type:ty ),* ) => {{
        map![
            $(
                TypeId::of::<$type>() => Arc::new(TypeInfo::new::<$type>())
            ),*
        ]
    }}
}
#[macro_export]
macro_rules! type_ids {
    ( $( $type:ty ),* ) => {{
        vec![
            $(TypeId::of::<$type>()),*
        ]
    }}
}

#[macro_export]
macro_rules! set {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(set!(@single $rest)),*]));

    ($($value:expr,)+) => { set!($($value),+) };
    ($($value:expr),*) => {
        {
            let _countcap = set!(@count $($value),*);
            let mut _set = hashbrown::HashSet::with_capacity(_countcap);
            $(
                let _ = _set.insert($value);
            )*
            _set
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

pub struct WithFirstIter<T: Iterator> {
    t: T,
    b: bool,
}
impl<T: Iterator> Iterator for WithFirstIter<T> {
    type Item = (bool, T::Item);
    fn next(&mut self) -> Option<Self::Item> {
        let was = self.b;
        self.b = false;
        self.t.next().map(|x| (was, x))
    }
}

pub(crate) trait WithFirst: Sized + Iterator {
    fn with_first(self) -> WithFirstIter<Self>;
}
impl<T: Iterator + Sized> WithFirst for T {
    fn with_first(self) -> WithFirstIter<Self> {
        WithFirstIter { t: self, b: true }
    }
}
