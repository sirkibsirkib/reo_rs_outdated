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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct GetterId(usize);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PutterId(usize);

// #[derive(Debug)]
// struct Action {
//     from: Option<GetterId>,
//     to: Vec<PutterId>,
// }

// #[derive(Debug)]
// enum Check<T> {
//     PeekEquality(GetterId, GetterId),
// }

pub trait Actionable<D: DataMover>: HasRelevantTokens {
    fn act(&self, d: &mut D) -> Result<(),PortClosed>;
}

pub trait TryClone {
    fn try_clone(&self) -> Self;
}
impl<T:Clone> TryClone for T {
    fn try_clone(&self) -> Self {
        self.clone()
    }
} 

pub struct TypedAction<T:TryClone,D: DataMover> {
    from: GetterId,
    to: Vec<PutterId>,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_d: std::marker::PhantomData<D>,
}
impl<T:TryClone,D:DataMover> Actionable<D> for TypedAction<T,D> {
    fn act(&self, d: &mut D) -> Result<(),PortClosed> {
        let x = d.get::<T>(self.from)?;
        for &dest in self.to.iter().skip(1) {
            d.put(dest, x.try_clone())?;
        }
        if let Some(&dest) = self.to.iter().next() {
            d.put(dest, x)?;
        }
        unimplemented!()
    }
}
impl<T:TryClone,D:DataMover> HasRelevantTokens for TypedAction<T,D> {
    fn populate_relevant_tokens(&self, bitset: &mut BitSet) {
        bitset.insert(self.from.0);
        for t in self.to.iter() {
            bitset.insert(t.0);
        }
    }
}


pub trait HasRelevantTokens {
    fn populate_relevant_tokens(&self, bitset: &mut BitSet);
}

pub trait Checkable<P: Peeker>: HasRelevantTokens {
    fn check(&self, p: &mut P) -> Result<bool, PortClosed>;
    fn redundant_with_ready_bitset(&self) -> bool;
}

pub struct TypedCheck<T:PartialEq,P:Peeker> {
    a: GetterId,
    b: GetterId,
    _phantom_t: std::marker::PhantomData<T>,
    _phantom_p: std::marker::PhantomData<P>,
}
impl<T:PartialEq+'static,P:Peeker> Checkable<P> for TypedCheck<T,P> {
    fn check(&self, p: &mut P) -> Result<bool,PortClosed> {
        let [a_ref, b_ref] = p.try_peek_two::<T>([self.a, self.b])?;
        Ok(a_ref == b_ref)
    }
    fn redundant_with_ready_bitset(&self) -> bool {
        false
    }
}
impl<T:PartialEq,P:Peeker> HasRelevantTokens for TypedCheck<T,P> {
    fn populate_relevant_tokens(&self, bitset: &mut BitSet) {
        bitset.insert(self.a.0);
        bitset.insert(self.b.0);
    }
}

pub trait Peeker {
    fn try_peek_two<T:'static>(&mut self, ids: [GetterId;2]) -> Result<[Option<&T>;2],PortClosed>;
    // fn try_peek<T>(&mut self, id: GetterId) -> Result<Option<&T>,()>;
}

pub trait DataMover {
    fn get<T>(&mut self, id: GetterId) -> Result<T,PortClosed>;
    fn put<T>(&mut self, id: PutterId, datum: T) -> Result<(),PortClosed>;
}

pub struct GuardedCmd<X:Peeker+DataMover> {
    pub enabled: bool,
    min_ready_set: BitSet,
    data_constraint: Vec<Box<dyn Checkable<X>>>,
    actions: Vec<Box<dyn Actionable<X>>>,
}
impl<X:Peeker+DataMover> GuardedCmd<X> {
    pub fn construct(
            checkable: impl IntoIterator<Item=Box<(dyn Checkable<X>)>>,
            actionable: impl IntoIterator<Item=Box<(dyn Actionable<X>)>>,
            ) -> Self {
        let mut min_ready_set = BitSet::new();
        let mut data_constraint = vec![];
        let mut actions = vec![];
        for c in checkable {
            c.populate_relevant_tokens(&mut min_ready_set);
            if !c.redundant_with_ready_bitset() {
                data_constraint.push(c);
            }
        }
        for a in actionable {
            a.populate_relevant_tokens(&mut min_ready_set);
            actions.push(a);
        }
        Self {
            enabled: true,
            min_ready_set,
            data_constraint,
            actions,
        }
    }
    pub fn wont_block(&self, bitset: &BitSet) -> bool {
        self.min_ready_set.is_subset(bitset)
    }
    pub fn check_all(&self, x: &mut X) -> Result<bool,PortClosed> {
        for c in self.data_constraint.iter() {
            if !c.check(x)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn act_all(&self, x: &mut X) -> Result<(),PortClosed> {
        for a in self.actions.iter() {
            a.act(x)?;
        }
        Ok(())
    }
}


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
