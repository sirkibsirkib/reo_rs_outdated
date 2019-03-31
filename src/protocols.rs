use mio::{Poll, Events};
use hashbrown::{HashSet, HashMap};
use crate::PortClosed;
use bit_set::BitSet;
use indexmap::IndexSet;

#[macro_export]
macro_rules! bitset {
	($( $port:expr ),*) => {{
		let mut s = BitSet::new();
		$( s.insert($port); )*
		s
	}}
}

// #[macro_export]
// macro_rules! tok_bitset {
//     ($( $tok:expr ),*) => {{
//         let mut s = BitSet::new();
//         $( s.insert($tok.inner()); )*
//         s
//     }}
// }

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

// short for "try peek". used in data-constraint closures for brevity
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

/*
function for clarity and brevity
allows one to seemingly define closures inside the GuardCmd structure
in reality, the closures exist in the same scope as the command.
this is necessary to ensure it has the closures have the necessary lifetime
and don't need heap-allocation
 */
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

/*
Contains all the behaviour common to protocol components.
Stub functions are defined to expose the minimal surface area for defining the
differences between specific protocols. The samey work is provided as a default
implementation.
*/
pub trait ProtoComponent: Sized {
    // given a raw token, if it has a peer (eg: putter for getter) that this struct
    // also manages locally, return its token. (This is thus only needed for Memory tokens)
    fn get_local_peer_token(&self, token: usize) -> Option<usize>;

    // shut down the structure for this token. used to kill memory cells that are unreachable
    // TODO error handling 
    fn token_shutdown(&mut self, token: usize);

    // register all local ports and memory cells with the provided poll instance
    fn register_all(&mut self, poll: &Poll);

    /*
    The system runs until termination, where termination is the state where all
    guard commands are INACTIVE.

    A command becomes INACTIVE if any of the ports in its firing set emit PortClosed error
    when the command executes its ACTION.
    A port is closed by the protocol itself if it becomes UNREACHABLE, where 
    there are no occurrences of its PEER in the the union of all firing-bitsets for active
    guard-commands. (intuitively: A command relying on port1-getter will never progress
    if there are no remaining )

    // TODO change implementation of Memory cell such that dropping the putter doesn't
    // result in closing the getter until any data inside the memory cell is yielded.
    */
    fn run_to_termination<'a,'b>(&'a mut self, gcmds: &'b [GuardCmd<Self>]) {
        // aux data about the provided guard command slice
        let guard_idx_range = 0..gcmds.len();
        let mut active_guards: HashSet<_> = guard_idx_range.collect();
        let mut tok_counter = TokenCounter::new(gcmds.iter().map(|g| g.get_ready_set()));

        // build mio::Poll object and related structures for polling.
        // delegate token registration to the other methods
        let mut ready_bits = BitSet::new();
        let mut dead_bits = BitSet::new();
        let mut make_inactive = IndexSet::new();
        let mut events = Events::with_capacity(32);
        let poll = Poll::new().unwrap();
        self.register_all(&poll);
        
        while !active_guards.is_empty() {
            // blocking call. resumes when 1+ events are stored inside `events`
            poll.poll(&mut events, None).unwrap();
            for event in events.iter() { // iter() consumes 1+ stored events.
                // put the ready flag up `$.0` unwraps the mio::Token, 
                // exposing the usize (mapping 1-to-1) with bitmap index.
                ready_bits.insert(event.token().0);
            }
            // check if any guards can be fired
            /*
            TODO detect unsatisfiable guard? (eg: data_constraint depends only on
            values that will never change.
            */
            for (i, g) in active_gcmds!(gcmds, active_guards) {
                if ready_bits.is_superset(g.get_ready_set()) && (g.data_constraint)(self)
                {
                    // unset the bits that have fired.
                    ready_bits.difference_with(g.get_ready_set());
                    // this call releases getters and putters
                    let result = g.perform_action(self);
                    if result.is_err() {
                        // Err(PortClosed) caught!
                        // TODO somehow acquire GETS and PUTS safely 
                        // such that all can be killed with PortError at once.
                        make_inactive.insert(i);
                    };
                }
            }
            while let Some(i) = make_inactive.pop() {
                active_guards.remove(&i);
                // `dead_bits` represent tokens that have just become unreachable
                for new_dead_bit in tok_counter.dec_return_dead(gcmds[i].get_ready_set()) {
                    dead_bits.insert(new_dead_bit);
                    if let Some(t_peer) = self.get_local_peer_token(new_dead_bit) {
                        dead_bits.insert(t_peer);
                    }
                }
                // make any guards with these peers inactive
                for (i, g) in active_gcmds!(gcmds, active_guards) {
                    if g.get_ready_set().intersection(&dead_bits).count() > 0 {
                        // this guard will never fire again! make inactive
                        make_inactive.insert(i);
                    }
                }
            }
        }
    }
}


#[derive(Debug)]
struct TokenCounter {
    // maps from bit-index to refcounts
    m: HashMap<usize, usize>,
}
impl TokenCounter {
    // given some bitsets, count the total references
    // eg: [{011}, {110}] gives counts [1,2,1]
    fn new<'a>(it: impl Iterator<Item=&'a BitSet>) -> Self {
        let mut m = HashMap::default();
        for b in it {
            for t in b.iter() {
                m.entry(t).and_modify(|e| *e += 1).or_insert(1);
            }
        }
        Self {m}
    }

    // decrement all refcounts flagged by this bitset by 1.
    // eg: [1,2,1] given {110} results in {0,1,1}
    // the function returns a bitset of indices that have become 0.
    // in the example above, we would return {100}
    pub fn dec_return_dead<'a,'b: 'a>(&'a mut self, bitset: &'b BitSet) -> impl Iterator<Item=usize> + 'a {
        bitset.iter().filter(move |b| {
            let v = self.m.get_mut(b).expect("BAD BITSET");
            *v -= 1;
            *v == 0
        })
    }
}