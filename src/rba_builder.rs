
use hashbrown::{HashSet,HashMap};
use std::cmp::Ordering;
use std::fmt;

type PortId = u32;
type StateId = usize;


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

#[derive(Debug)]
struct Branch {
	port: PortId,
	// dest: Pred,
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
	let mut opts: HashMap<ConcretePred, Vec<Branch>> = HashMap::default();

	//input: gcmds and starting state.
	//output: map state->Vec<Branch>
	let gcmd = vec![
		Gcmd::new(concrete!{A=>true, C=>false}, 1, symbolic!{B=>U}),
		Gcmd::new(concrete!{A=>true, C=>false}, 1, symbolic!{B=>T}),
		Gcmd::new(concrete!{B=>false}, 2, symbolic!{A=>T}),
		Gcmd::new(concrete!{C=>true}, 2, symbolic!{A=>T}),
		Gcmd::new(concrete!{B=>true, C=>false}, 2, symbolic!{A=>T}),
	];
	while let Some(state) = states_todo.pop() {
		println!("\n~~~~~~~~~~ processing: {:?}", state);
		let mut o = vec![];
		for (gid, g) in gcmd.iter().enumerate() {
			if g.input.compatible(&state) {
				let new_state = g.output.apply_to(&state);
				println!("state {:?} can apply rule {} ({:?}). gets {:?}", &state, gid, &g.input, &new_state);
				if !states.contains(&new_state) {
					states_todo.push(new_state.clone());
					states.insert(new_state.clone());
				}
				o.push(Branch{
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
			print!("={}=> {:?}, ", b.port, &b.dest);
		}
		print!("}}\n");
	}
}


//////////////////////////// STEP 1 ///////////////////

// struct ProtoRba {
// 	rules: Vec<Rule>,
// }
// struct Rule {
// 	guard: Pred,
// 	ports: HashSet<PortId>,
// 	action: Pred,
// }

// struct ProjectionError {
// 	rba_rule_idx: usize,
// 	synchronous: [PortId;2],
// }

// type ValId = usize;
// struct ConditionalFlip {
// 	condition: (ValId, Val),
// 	flip: (ValId, Val),
// }

// fn project(proto_rba: &ProtoRba, atomic_ports: &HashSet<PortId>) ->Result<Vec<Gcmd>, ProjectionError> {
// 	//1 check that we can do this at all
// 	for (rid, rule) in proto_rba.rules.iter().enumerate() {
// 		let intersection = rule.ports.intersection(atomic_ports);
// 		if intersection.count() >= 2 {
// 			return Err(ProjectionError {
// 				rba_rule_idx: rid,
// 				synchronous: [0,0],
// 			});
// 		}
// 	}
// 	//2 classify rules according to whether they are participating
// 	let mut participating = proto_rba.rules.iter().filter(|rule| rule.ports.intersection(atomic_ports).next().is_some());
// 	let mut non_participating = proto_rba.rules.iter().filter(|rule| rule.ports.intersection(atomic_ports).next().is_none());
// 	let mut flips: Vec<ConditionalFlip> = vec![];
// 	unimplemented!()
// }


// #[test]
// fn rba_atomize() {
// 	println!("{:?}", std::mem::size_of::<Option<bool>>());
// 	// let 
// }