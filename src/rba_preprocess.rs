type PortId = u32;
use std::{fmt, mem, cmp};

use hashbrown::HashSet;

macro_rules! ss {
	($arr:expr) => {{StateSet {predicate: $arr}}}
}

macro_rules! hashset {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(hashset!(@single $rest)),*]));

    ($($key:expr,)+) => { hashset!($($key),+) };
    ($($key:expr),*) => {
        {
            let _cap = hashset!(@count $($key),*);
            let mut _set = HashSet::with_capacity(_cap);
            $(
                let _ = _set.insert($key);
            )*
            _set
        }
    };
}

// part of the state-set predicate. 
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Val {
	T, F, 	// specific values corresponding to boolean true and false,
	X,		// generic over T and F. Interpreted as an unspecified value.
}
impl PartialOrd for Val {
	// ordering is on SPECIFICITY: X<T, X<F, T is not comparable to F.
	fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
		use Val::*;
		match [self, other] {
			[a,b] if a==b => Some(cmp::Ordering::Equal),
			[X,_] => Some(cmp::Ordering::Less),
			[_,X] => Some(cmp::Ordering::Greater),
			_ => None,
		}
	}
}
impl Val {
	pub fn generic(self) -> bool {
		self == Val::X
	}
	pub fn specific(self) -> bool {
		!self.generic()
	}
	pub fn mismatches(self, other: Self) -> bool {
		self.partial_cmp(&other).is_none()
	}
}

// represents a port automaton with transitions represented as logical rules
#[derive(Debug, Clone)]
pub struct Rbpa {
	/* 	A mask for which memory-variable indices are _irrelevant_.
		where the value is `false`, T==F==X. */
	mask: StateMask,
	rules: Vec<Rule>,
}
impl Rbpa {
	// 
	pub fn mask_irrelevant_vars(&mut self) -> bool {
		let mut changed_something = false;
		'outer: for i in 0..StateSet::LEN {
			if !self.mask.relevant_index[i] {
				continue; // already irrelevant
			}
			for r in self.rules.iter() {
				if r.guard.predicate[i].specific() {
					continue 'outer;
				}
			}
			// this index is irrelevant
			println!("index {} is irrelevant", i);
			self.mask.relevant_index[i] = false;
			changed_something = true;
			for r in self.rules.iter_mut() {
				r.guard.predicate[i] = Val::X;
				r.assign.predicate[i] = Val::X;
			}
		}
		changed_something
	}
	pub fn normalize(&mut self) {
		let mut buf = vec![];
		while let Some(idx) = self.first_silent_idx() {
			let silent = self.rules.remove(idx);
			println!("... Removing silent rule at idx={}", idx);
			if silent.no_effect() {
				// when [silent . x] == x
				continue;
			}
			for (i,r) in self.rules.iter().enumerate() {
				if let Some(composed) = silent.compose(r) {
					let old_i = if i>=idx {i+1} else {i};
					println!("ADDING composed rule ({},{})", idx, old_i);
					buf.push(composed);
				}
			}
			self.rules.append(&mut buf);
			println!("AFTER: {:#?}\n----------------", &self.rules);
			self.merge_rules();
			println!("... rules_merged {:#?}", &self.rules);
		}
		self.merge_rules();
		while self.mask_irrelevant_vars() || self.merge_rules() {}
		// DONE
	}
	pub fn first_silent_idx(&self) -> Option<usize> {
		self.rules.iter().enumerate().filter(|(_,r)| r.is_silent()).map(|(i,_)| i).next()
	}
	pub fn merge_rules(&mut self) -> bool {
		let mut changed_something = false;
		'outer: loop {
			for (idx1, r1) in self.rules.iter().enumerate() {
				let rng = (idx1 + 1)..;
				for (r2, idx2) in self.rules[rng.clone()].iter().zip(rng) {
					if let Some(new_rule) = r1.try_merge(r2) {
						changed_something = true;
						let _ = mem::replace(&mut self.rules[idx1], new_rule);
						self.rules.remove(idx2);
						continue 'outer;
					}
				}
			}
			return changed_something
		}
	}
}


#[derive(Eq, PartialEq, Copy, Clone, Hash)]
pub struct StateSet {
	predicate: [Val; Self::LEN],
}
impl PartialOrd for StateSet {
	fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
		use cmp::Ordering::*;
		let mut o = Equal;
		for (&a, &b) in izip!(self.iter(), rhs.iter()) {
			match a.partial_cmp(&b) {
				None => return None,
				Some(x @ Less) | Some(x @ Greater) => {
					if o==Equal {
						o = x;
					} else if o!=x {
						return None;
					}
				},
				Some(Equal) => (),
			} 
		}
		Some(o)
	}
}
impl StateSet {
	const LEN: usize = 3;
	pub fn make_specific_wrt(&mut self, other: &Self) {
		for (s, o) in izip!(self.iter_mut(), other.iter()) {
			if *s < *o { // s is X, o is specific. copy specific value.
				*s = *o;
			}
		}
	}
	pub fn iter(&self) -> impl Iterator<Item=&Val> {
		self.predicate.iter()
	}
	pub fn iter_mut(&mut self) -> impl Iterator<Item=&mut Val> {
		self.predicate.iter_mut()
	}
}
impl fmt::Debug for StateSet {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		for x in self.predicate.iter() {
			write!(f, "{:?}", x)?;
		}
		Ok(())
	}
}


#[derive(Clone, Eq, PartialEq, Hash)]
struct Rule {
	// invariant: an X in assignment implies an X in guard at same position
	guard: StateSet,
	port: Option<PortId>,
	assign: StateSet,
}
impl Rule {
	// apply the given rule to this state set. Return the new state set
	pub fn apply(&self, set: &StateSet) -> Option<StateSet> {
		let mut res = set.clone();
		for (&g, &a, r) in izip!(self.guard.iter(), self.assign.iter(), res.iter_mut()) {
			if g.mismatches(*r) {
				// guard is not satisfied.
				return None;
			} else if a.specific() {
				// explicit assignment. Eg: X->T or T->T
				*r = a;
			} else if g.specific() {
				// implicit assignment because of guard. Eg: X  ==> (T->X) ==> T
				*r = g;
			}
		}
		Some(res)
	}
	// true if the rule has no port and has no effect on memory. eg: F->F 
	pub fn no_effect(&self) -> bool {
		if self.port.is_some() {
			return false
		}
		for (&g, &a) in izip!(self.guard.iter(), self.assign.iter()) {
			if g.mismatches(a) || g < a {
				return false
			}
		}
		true
	}

	// if these two rules can be represented by one, return that rule
	pub fn try_merge(&self, other: &Self) -> Option<Rule> {
		let g_cmp = self.guard.partial_cmp(&other.guard);
		let a_cmp = self.assign.partial_cmp(&other.assign);

		use cmp::Ordering::*;
		match [g_cmp, a_cmp] {
			// 1st case: one rule subsumes the other. Eg: {X->T, T->T} give X->T 
			[Some(g), Some(a)] if (a==Equal || a==g) && (g==Equal || g==Less) => Some(self.clone()),
			[Some(g), Some(a)] if (a==Equal || a==g) && g==Greater => Some(other.clone()),
			// 2nd case. There exists rule R which is split in half by these two rules. Return R.
			// eg: {T->T, F->T} give X->T
			[None   , Some(Equal)] => {
				let mut guard = self.guard.clone();
				let mut equal_so_far = true;
				for (g, &g2) in izip!(guard.iter_mut(), other.guard.iter()) {
					if *g != g2 {
						if !equal_so_far {
							// 2+ indices mismatch
							return None
						}
						equal_so_far = false;
						*g = Val::X;
					}
				}
				Some(Rule::new(guard, self.port.clone(), self.assign.clone()))
			},
			_ => None,
		}
	}
	// return a new rule that represents two rules applied in the sequence: [self, other]
	// prodecure fails if provided rules that cannot be composed. Eg: [F->F, T->T]
	pub fn compose(&self, other: &Self) -> Option<Rule> {
		println!("composing {:?} and {:?}", self, other);
		if !self.can_precede(other) {
			return None
		}
		let port: Option<PortId> = self.port.or(other.port);
		let mut guard = self.guard.clone();
		// where the LATTER rule specifies something the FORMER leaves generic, specify it.
		// Eg: [X->X . F->T] becomes [F->T] not [X->T]
		use Val::X;
		for (&g1, &a1, &g2, ng) in izip!(self.guard.iter(), self.assign.iter(), other.guard.iter(), guard.iter_mut()) {
			if g1==X && a1==X && g2.specific() {
				*ng = g2;
			}
		}
		// where the FORMER rule specifies something the LATTER leaves generic, specify it.
		// Eg: [F->T . X->X] becomes [F->T] not [F->X]
		let mut assign = other.assign.clone();
		for (a, &g1, &a1, &g2, &a2) in izip!(assign.iter_mut(), self.guard.iter(), self.assign.iter(), other.guard.iter(), other.assign.iter()) {
			let latter_is_generic = g2==X && a2==X;
			if latter_is_generic {
				if a1.specific() {
					*a = a1;
				} else if g1.specific() {
					*a = g1;
				}
			}
		}
		Some(Rule::new(guard, port, assign))
	}
	pub fn new(guard: StateSet, port: Option<PortId>, mut assign: StateSet) -> Self {
		assign.make_specific_wrt(&guard);
		Self {guard, port, assign}
	} 
	pub fn is_silent(&self) -> bool {
		self.port.is_none()
	}
	pub fn can_precede(&self, other: &Self) -> bool {
		if self.port.is_some() && other.port.is_some() {
			// rules by our definition can involve 0 or 1 ports. this would require 2.
			return false;
		}
		for (&a, &g) in self.assign.iter().zip(other.guard.iter()) {
			// assignment of first rule produces something that mismatches guard of the 2nd.
			if a.mismatches(g) {
				return false
			}
		}
		true
	}
}
impl fmt::Debug for Rule {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self.port {
			Some(p) => write!(f, "{:?} ={:?}=> {:?}", &self.guard, p, &self.assign)?,
			None => write!(f, "{:?} =.=> {:?}", &self.guard, &self.assign)?,
		};
		Ok(())
	}
}

#[test]
fn testy() {
	wahey()
}

pub fn project(mut r: Rbpa, atomic_ports: HashSet<PortId>) -> Rbpa {
	for rule in r.rules.iter_mut() {
		if let Some(p) = rule.port {
			if !atomic_ports.contains(&p) {
				// hide!
				rule.port = None;
			}
		}
	}
	r.normalize();
	r
}

pub fn wahey() {
	use Val::*;
	let rba = Rbpa { rules: vec![
		Rule::new(ss![[X,X,F]], Some(1), ss![[X,X,T]]),
		Rule::new(ss![[X,F,T]], Some(2), ss![[X,T,F]]),
		Rule::new(ss![[F,T,T]], Some(3), ss![[T,F,F]]),
		Rule::new(ss![[T,T,T]], Some(1), ss![[F,F,F]]),
	], mask: StateMask {relevant_index: [true;StateSet::LEN]}};
	let org = rba.clone();
	println!("BEFORE");
	for r in rba.rules.iter() {
		println!("{:?}", r);
	}
	let atomic_ports = hashset!{1};
	let start = std::time::Instant::now();
	let rba2 = project(rba, atomic_ports.clone());
	println!("ELAPSED {:?}", start.elapsed());
	println!("AFTER: {:#?}", rba2);
	pair_test(ss![[F,F,F]], org, rba2, atomic_ports);
}

#[derive(Clone, derive_new::new)]
pub struct StateMask {
	relevant_index: [bool; StateSet::LEN], 
}
impl fmt::Debug for StateMask {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		for &x in self.relevant_index.iter() {
			write!(f, "{}", if x {'1'} else {'0'})?;
		}
		Ok(())
	}
}
impl StateMask {
	pub fn mask(&self, state: StateSet) -> StateSet {
		let mut ret = state.clone();
		for (r, &b) in izip!(ret.iter_mut(), self.relevant_index.iter()) {
			if !b {
				*r = Val::X;
			}
		}
		ret
	}
}

pub fn pair_test(mut state: StateSet, rba: Rbpa, atomic: Rbpa, atomic_ports: HashSet<PortId>) {
	println!("PROTO: {:#?}\nATOMIC: {:#?}", &rba, &atomic);
	// let mut buf = HashSet::default();
	let mut atomic_state = state.clone();
	let mut rng = rand::thread_rng();
	let mut trace = format!("P: {:?}", &state);
	let mut trace_atomic = format!("A: {:?}", &state);
	let mut try_order: Vec<usize> = (0..rba.rules.len()).collect();

	'outer: for _ in 0..24 {
		use rand::seq::SliceRandom;
		try_order.shuffle(&mut rng);
		for rule in try_order.iter().map(|&i| &rba.rules[i]) {
			if let Some(new_state) = rule.apply(&state) {
				state = new_state;
				while trace_atomic.len() < trace.len() {
					trace_atomic.push(' ');
				}
				trace.push_str(&match rule.port {
					Some(p) => format!(" --{}-> {:?}", p, &new_state),
					None => format!(" --.-> {:?}", &new_state),
				});
				if let Some(p) = rule.port {
					if atomic_ports.contains(&p) {
						// took NONSILENT TRANSITION
						// check that the atomic can simulate this step.
						'inner: for rule2 in atomic.rules.iter().filter(|r| r.port == Some(p)) {
							if let Some(new_atomic_state) = rule2.apply(&atomic_state) {
								let new_atomic_state = atomic.mask.mask(new_atomic_state);
								if new_atomic_state != atomic.mask.mask(new_state) {
									continue 'inner;
								} else {
									// match!
									atomic_state = new_atomic_state;
									trace_atomic.push_str(&format!(" --{}-> {:?}", p, &new_atomic_state));
									continue 'outer;
								}
							}
						}
						println!("FAILED TO MATCH");
						break 'outer;
					}
				}
				continue 'outer; // some progress was made
			}
		}
		println!("STUCK!");
		break;
	}
	println!("{}\n{}", trace, trace_atomic);
}