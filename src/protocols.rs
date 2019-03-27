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
//
// struct ReachTracker {
//     token_occurrences: HashMap<usize,usize>,
// }
// impl ReachTracker {
//     pub fn new<'a>(it: impl Iterator<Item=&'a BitSet<usize>>) -> Self {
//         let mut map = HashMap::new();
//         for bitset in it {
//             for s in bitset.iter() {
//                 map.entry(s)
//                .and_modify(|e| { *e += 1 })
//                .or_insert(0);
//             }
//         }
//         ReachTracker
//     }
// }

// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
// pub struct Tok(usize);
// impl Tok{
//     const COUNTMASK: usize = 0 ^ 1;
//     pub const fn new_putter(pid: usize) -> Self {
//         Self(pid*2)
//     }
//     pub const fn new_getter(pid: usize) -> Self {
//         Self(pid*2 + 1)
//     }
//     pub const fn is_putter(self) -> bool {
//         self.0 & 1 == 0
//     }
//     pub const fn to_getter(self) -> Self {
//         Self(self.0 | 1)
//     }
//     pub const fn to_putter(self) -> Self {
//         Self(self.0)
//     }
//     pub const fn peer_tok(self) -> Self {
//         Self(self.0 ^ Self::COUNTMASK)
//     }
//     pub const fn token(self) -> mio::Token {
//         mio::Token(self.0)
//     }
//     pub const fn inner(self) -> usize {
//         self.0
//     }
// }

// pub struct GuardTracker {
//     refcounts: HashMap<usize, usize>,
// }
// impl GuardTracker {
//     pub fn new<'a>(it: impl IntoIterator<Item=&'a BitSet>) -> Self {
//         let mut refcounts = HashMap::new();
//         for bitset in it.into_iter() {
//             for index in bitset.iter() {
//                 let tok: Tok = Tok(index);
//                 refcounts.entry(tok.to_getter().inner())
//                 .and_modify(|e| { *e += 1 })
//                 .or_insert(0);
//             }
//         }
//         Self {refcounts}
//     }
//     pub fn decrement_set(&mut self, bitset: &BitSet) {
//         for index in bitset.iter() {
//             let tok: Tok = Tok(index);
//             *self.refcounts.get_mut(&tok.to_getter().inner())
//             .expect("DECREMENTING MISSING BIT") += 1;
//         }
//     }
//     pub fn any_reachable(&self, bitset: &BitSet) -> bool {

//     }
// }

// #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
// struct GetterId(usize);
// #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
// struct PutterId(usize);

// #[derive(Debug)]
// struct Action {
//     from: Option<GetterId>,
//     to: Vec<PutterId>,
// }

// #[derive(Debug)]
// enum Check {
//     PeekEquality([GetterId;2]),
//     PeekSome(GetterId),
//     PeekNone(GetterId),
// }
// impl Check {
//     pub fn pass_check<A: ActionExecutor>(&self, a: &mut A) -> bool {
//         match self {
//             Check::PeekEquality([id1, id2]) => {
//                 a.peek(*id1) == a.peek(*id2)
//             },
//         }
//     }
// }

// struct GuardedCmd {
//     pub enabled: bool,
//     nonblocking_set: BitSet,
//     data_const: Vec<Check>,
//     actions: Vec<Action>,
// }
// impl GuardedCmd {
//     pub fn new<'a>(
//         _checks: impl IntoIterator<Item=&'a Check>,
//         _actions: impl IntoIterator<Item=&'a Action>) -> Self
//     {
//         let mut nonblocking_set = BitSet::new();
//         let mut data_const: Vec<Check> = vec![];
//         let mut actions: Vec<Action> = vec![];
//         Self {nonblocking_set, data_const, actions, enabled: true}
//     }

//     pub fn would_block(&self, ready: &BitSet) -> bool {
//         self.nonblocking_set.is_subset(ready)
//     }

//     pub fn pass_guard<A: ActionExecutor>(&self, executor: &mut A) {
//         for d in self.data_const {

//         }
//     }
// }

// trait ActionExecutor {
//     fn get<T>(&mut self, id: GetterId) -> T;
//     fn put<T>(&mut self, id: PutterId, datum: T);
//     fn peek<T>(&mut self, id: GetterId) -> &T;
//     fn try_peek<T>(&mut self, id: GetterId) -> &T;
// }
