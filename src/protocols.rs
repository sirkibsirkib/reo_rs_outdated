use mio::{Poll, Events};
use hashbrown::HashSet;
use crate::reo::PortClosed;
use bit_set::BitSet;
use indexmap::IndexSet;
use hashbrown::HashMap;

#[macro_export]
macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = BitSet::new();
		$( s.insert($port); )*
		s
	}}
}

#[macro_export]
macro_rules! tok_bitset {
    ($( $tok:expr ),*) => {{
        let mut s = BitSet::new();
        $( s.insert($tok.inner()); )*
        s
    }}
}

#[macro_export]
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

#[macro_export]
macro_rules! tpk {
    ($var:expr) => {
        match $var.try_peek() {
            Err(_) => return false,
            Ok(x) => x,
        }
    }
}

#[macro_export]
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

#[macro_export]
macro_rules! defm {
    () => {
        Memory::default()
    }
}

#[macro_export]
macro_rules! guard_cmd {
    ($guards:ident, $firing:expr, $data_con:expr, $action:expr) => {
        let data_con = $data_con;
        let action = $action;
        $guards.push(GuardCmd::new($firing, &data_con, &action));
    };
}

pub struct GuardCmd<'a, T> {
    ready_set: BitSet,
    data_constraint: &'a (dyn Fn(&mut T)->bool),
    action: &'a (dyn Fn(&mut T)->Result<(), PortClosed>),
}
impl<'a,T> GuardCmd<'a,T> {
    pub fn new(ready_set: BitSet,
        data_constraint: &'a (dyn Fn(&mut T)->bool),
        action: &'a (dyn Fn(&mut T)->Result<(), PortClosed>)) -> Self {
        Self { ready_set, data_constraint, action}
    }
    pub fn get_ready_set(&self) -> &BitSet {
        &self.ready_set
    }
    pub fn check_constraint(&self, t: &mut T) -> bool {
        (self.data_constraint)(t)
    }
    pub fn perform_action(&self, t: &mut T) -> Result<(), PortClosed> {
        (self.action)(t)
    }
}


macro_rules! active_gcmds {
    ($guards:expr, $active_guards:expr) => {
        $guards.iter().enumerate().filter(|(i,_)| $active_guards.contains(i))
    }
}

pub trait ProtoComponent: Sized {
    fn get_local_peer_token(&self, token: usize) -> Option<usize>;
    fn token_shutdown(&mut self, token: usize);
    fn register_all(&mut self, poll: &Poll);
    fn run_to_termination<'a,'b>(&'a mut self, gcmds: &'b [GuardCmd<Self>]) {

        let guard_idx_range = 0..gcmds.len();
        let mut active_guards: HashSet<_> = guard_idx_range.collect();
        let mut ready = BitSet::new();
        let mut make_inactive = IndexSet::new();
        let mut tok_counter = TokenCounter::new(gcmds.iter().map(|g| g.get_ready_set()));
        let mut events = Events::with_capacity(32);
        let poll = Poll::new().unwrap();
        self.register_all(&poll);

        while !active_guards.is_empty() {
            poll.poll(&mut events, None).unwrap();
            for event in events.iter() {
                // put the ready flag up
                ready.insert(event.token().0);
            }
            // 1+ events have occurrec
            for (i, g) in active_gcmds!(gcmds, active_guards) {
                if ready.is_superset(g.get_ready_set()) && (g.data_constraint)(self)
                {
                    ready.difference_with(g.get_ready_set());
                    let result = g.perform_action(self);
                    if result.is_err() {
                        make_inactive.insert(i);
                    };
                }
            }
            while let Some(i) = make_inactive.pop() {
                active_guards.remove(&i);
                let dead_bits = tok_counter.dec_return_dead(gcmds[i].get_ready_set());
                let mut dead_bit_peers = BitSet::default();
                for t in dead_bits.iter() {
                    if let Some(t_peer) = self.get_local_peer_token(t) {
                        dead_bit_peers.insert(t_peer);
                    }
                }
                for (i, g) in active_gcmds!(gcmds, active_guards) {
                    if g.get_ready_set().intersection(&dead_bit_peers).count() > 0 {
                        // this guard will never fire again! contains a token with dead peer
                        make_inactive.insert(i);
                    }
                }
            }
        }
    }
}


#[derive(Debug)]
struct TokenCounter {
    m: HashMap<usize, usize>,
}
impl TokenCounter {
    fn new<'a>(it: impl Iterator<Item=&'a BitSet>) -> Self {
        let mut m = HashMap::default();
        for b in it {
            for t in b.iter() {
                m.entry(t).and_modify(|e| *e += 1).or_insert(1);
            }
        }
        Self {m}
    }
    pub fn dec_return_dead(&mut self, bitset: &BitSet) -> BitSet {
        let mut dead = BitSet::new();
        for b in bitset.iter() {
            let v = self.m.get_mut(&b).expect("BAD BITSET");
            *v -= 1;
            dead.insert(b);
        }
        dead
    }
}