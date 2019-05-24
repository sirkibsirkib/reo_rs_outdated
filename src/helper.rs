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

pub trait WithFirstTrait: Iterator + Sized {
    fn with_first(self) -> WithFirst<Self> {
        WithFirst {
            first: true,
            it: self,
        }
    }
}
impl<I: Iterator> WithFirstTrait for I {}
pub struct WithFirst<I: Iterator> {
    first: bool,
    it: I,
}
impl<I: Iterator> Iterator for WithFirst<I> {
    type Item = (bool, I::Item);
    fn next(&mut self) -> Option<Self::Item> {
        match (self.first, self.it.next()) {
            (_, None) => None,
            (true, Some(x)) => {
                self.first = false;
                Some((true, x))
            }
            (false, Some(x)) => Some((false, x)),
        }
    }
}
