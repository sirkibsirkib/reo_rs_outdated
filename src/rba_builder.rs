
use std::marker::PhantomData;
use hashbrown::{HashSet,HashMap};
use std::cmp::Ordering;
use crate::decimal::*;
use std::fmt;

type PortId = u32;


const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const NUM_MEMS: usize = 3;

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
enum AssignVal {
	T, // True
	F, // False
	G, // generic. never appears in STATE
	U, // explicity UNKNOWN. used only in assignment
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
enum Val {
	T, // True
	F, // False
	G, // generic. never appears in STATE
	U, // explicity UNKNOWN. used only in assignment
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct ConcretePred {
	known: [Option<bool>; NUM_MEMS],
}
impl fmt::Debug for ConcretePred {
	fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		for &k in self.known.iter() {
			let c: char = match k {
				None => '*',
				Some(true) => 'T',
				Some(false) => 'F',
			};
			formatter.write_fmt(format_args!("{}", c))?
		}
		Ok(())
	}
}
impl ConcretePred {
	fn new(it: impl Iterator<Item=(usize, bool)>) -> Self {
		let mut known = [None; NUM_MEMS];
		for (k,v) in it {
			known[k] = Some(v);
		}
		Self { known }
	}
	fn compatible(&self, other: &Self) -> bool {
		for (&a, &b) in self.known.iter().zip(other.known.iter()) {
			if let [Some(x), Some(y)] = [a, b] {
				if x != y {return false}
			}
		}
		true
	}
}
// impl std::cmp::PartialOrd for ConcretePred {
// 	// ordering in terms of SPECIFICITY
// 	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
// 		let mut res = Ordering::Equal;
// 		for i in 0..NUM_MEMS {
// 			match (res, self.known[i], other.known[i]) {
// 				(_, Some(l), Some(r)) if l != r => return None,
// 				(Ordering::Less, Some(_), None) |
// 				(Ordering::Greater, None, Some(_)) => return None,
// 				(_, Some(_), None) => res = Ordering::Greater,
// 				(_, None, Some(_)) => res = Ordering::Less,
// 				(_, _, _) => {},
// 			}
// 		}
// 		Some(res)
// 	}
// }

#[derive(Debug, Clone, Hash)]
struct SymbolicPred {
	values: [AssignVal; NUM_MEMS],
}
impl SymbolicPred {
	fn new(it: impl Iterator<Item=(usize, AssignVal)>) -> Self {
		let mut values = [AssignVal::G; NUM_MEMS];
		for (k,v) in it {
			values[k] = v;
		}
		Self { values }
	}
	fn apply_to(&self, state: &ConcretePred) -> ConcretePred {
		let mut new = state.clone();
		for (n, &a) in new.known.iter_mut().zip(self.values.iter()) {
			match a {
				AssignVal::G => {},
				AssignVal::T => *n = Some(true),
				AssignVal::F => *n = Some(false),
				AssignVal::U => *n = None,
			}
		}
		new
	}
}

// type Pred = [Val;3];

#[derive(Debug)]
struct Gcmd {
	input: ConcretePred,
	involved: PortId,
	output: SymbolicPred,
}
impl Gcmd {
	fn new(input: ConcretePred, involved: PortId, output: SymbolicPred) -> Self {
		Self {input, involved, output}
	}
}

#[derive(Debug, Hash, Eq, Clone, PartialEq)]
struct Branch {
	port: PortId,
	dest: ConcretePred,
}

#[macro_export]
macro_rules! set {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(set!(@single $rest)),*]));

    ($($value:expr,)+) => { set!($($value),+) };
    ($($value:expr),*) => {
        {
            let _countcap = set!(@count $($value),*);
            let mut _the_set = HashSet::with_capacity(_countcap);
            $(
                let _ = _the_set.insert($value);
            )*
            _the_set
        }
    };
}

macro_rules! concrete {
	($($key:expr => $value:expr),*) => {{
		let mut _x = [None; NUM_MEMS];
		$(
            _x[$key] = Some($value);
        )*
        ConcretePred { known: _x } 
	}}
}

macro_rules! symbolic {
	($($key:expr => $value:expr),*) => {{
		let mut _x = [AssignVal::G; NUM_MEMS];
		$(
            _x[$key] = $value;
        )*
        SymbolicPred { values: _x } 
	}}
}

#[test]
fn rba_build() {
	use AssignVal::*;
	let mut states = set!{concrete!{A=>false, B=>false, C=>false}};
	let mut states_todo = vec![states.iter().next().unwrap().clone()];
	let mut opts: HashMap<ConcretePred, HashSet<Branch>> = HashMap::default();

	//input: gcmds and starting state.
	//output: map state->Vec<Branch>
	let gcmd = vec![
		Gcmd::new(concrete!{A=>true, C=>true}, 1, symbolic!{B=>U}),
		Gcmd::new(concrete!{A=>true, C=>false}, 1, symbolic!{B=>T}),
		Gcmd::new(concrete!{B=>false}, 2, symbolic!{A=>T}),
		Gcmd::new(concrete!{C=>true}, 2, symbolic!{A=>T}),
		Gcmd::new(concrete!{B=>true, C=>false}, 2, symbolic!{A=>T}),
	];
	while let Some(state) = states_todo.pop() {
		println!("\n~~~~~~~~~~ processing: {:?}", state);
		let mut o = HashSet::default();
		for (gid, g) in gcmd.iter().enumerate() {
			if g.input.compatible(&state) {
				let new_state = g.output.apply_to(&state);
				println!("state {:?} can apply rule {} ({:?}). gets {:?}", &state, gid, &g.input, &new_state);
				if !states.contains(&new_state) {
					states_todo.push(new_state.clone());
					states.insert(new_state.clone());
				}
				o.insert(Branch{
					port: g.involved,
					dest: new_state,
				});
			} else {
				// println!("state {:?} cannot apply rule {} ({:?})", &state, gid, &g.input);
			}
		}
		opts.insert(state, o);
	}
	// println!("{:#?}", opts);

	println!("DENSE...");
	for (src, opts) in opts.iter() {
		print!("{:?}: {{", src);
		for b in opts.iter() {
			print!("={}=> {:?},\t", b.port, &b.dest);
		}
		print!("}}\n");
	}
}


//////////////////////////// STEP 1 ///////////////////

struct StateId(usize);

struct ProtoRba {
	start_state: ConcretePred,
	rules: Vec<Rule>,
}
struct Rule {
	guard: ConcretePred,
	ports: HashSet<PortId>,
	action: SymbolicPred,
}

struct ProjectionError {
	rba_rule_idx: usize,
	synchronous: [PortId;2],
}

struct Opty {
	branches: Vec<Branch>,
}

struct Branchy {
	dest: StateId,
	port: PortId,
}


fn internal_close(state: &ConcretePred, rules: &[Rule]) -> HashSet<ConcretePred> {
	let mut res = HashSet::default();
	let mut todo = set!{state.clone()};
	while let Some(state) = todo.iter().cloned().next() {
		todo.remove(&state);
		for r in rules.iter() {
			if state.compatible(&r.guard) {
				let new_state = r.action.apply_to(&state);
				if !res.contains(&new_state) {
					res.insert(new_state.clone());
					todo.insert(new_state);
				}
			}
		}
	}
	res
}


// struct Repper<A: NoData, B: NoData> {
// 	phantom: PhantomData<(A,B)>,
// }

// pub enum Reps<A: NoData, B: NoData, C: NoData> {
// 	More(A, Rep<A,B,C>),
// 	End(C),
// }


// pub trait NoData: Sized {
// 	fn new() -> Self;
// }

// pub struct Rep<A: NoData, B: NoData, C: NoData> {
// 	remaining: usize,
// 	phantom: PhantomData<(A,B,C)>,
// }
// impl<A: NoData, B: NoData, C: NoData> Rep<A,B,C> {
// 	fn new(reps: usize) -> Self {
// 		Rep { remaining: reps, phantom: PhantomData::default() }
// 	}
// 	pub fn next(self) -> Reps<A,B,C> {
// 		match self.remaining {
// 			0 => Reps::End(C::new()),
// 			x => Reps::More(A::new(), Self::new(x-1)),
// 		}
// 	}
// 	pub fn until<F>(self, mut work: F) -> C where F: FnMut(A, usize)->B {
// 		let mut r = self;
// 		loop {
// 	        match r.next() {
// 	            Reps::More(a, next) => {
// 	                r = next;
// 	                work(a);
// 	            },
// 	            Reps::End(c) => break c,
// 	        }
// 	    }
// 	}
// }


// pub struct State<T>(PhantomData<T>);
// impl<T> NoData for State<T> {fn new() -> Self {State(PhantomData::default())}}
// #[test]
// pub fn repetition() {
// 	let work = |_n0: State<N0>| {
// 		println!("YAH");
// 		State::<N1>::new()
// 	};
//     let _c: State<N2> = Rep::new(11).until(work);
// }