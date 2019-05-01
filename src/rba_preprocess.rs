type PortId = u32;
const LEN: usize = 3;
use std::{fmt, mem};

use hashbrown::HashSet;

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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum Val {
	T, F, X,
}
impl Val {
	pub fn generic(self) -> bool {
		self == Val::X
	}
	pub fn specific(self) -> bool {
		!self.generic()
	}
	pub fn mismatches(self, other: Self) -> bool {
		use Val::*;
		let s: [Val;2] = [self, other];
		match s {
			[T,F] | [F,T] => true,
			_ => false,
		}
	}
}

#[derive(Debug, Clone)]
pub struct Rba {
	rules: Vec<Rule>,
}
impl Rba {
	pub fn normalize(mut self) -> Self {
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
					println!("ADDING rule ({},{})", idx, i);
					buf.push(composed);
				}
			}
			self.rules.append(&mut buf);
			println!("AFTER: {:#?}\n----------------", &self.rules);
			self = self.rule_merge();
			println!("... rules_merged {:#?}", &self.rules);
		}
		self
	}
	pub fn first_silent_idx(&self) -> Option<usize> {
		self.rules.iter().enumerate().filter(|(_,r)| r.is_silent()).map(|(i,_)| i).next()
	}
	pub fn rule_merge(mut self) -> Self {
		'outer: loop {
			for (idx1, r1) in self.rules.iter().enumerate() {
				let rng = (idx1 + 1)..;
				for (r2, idx2) in self.rules[rng.clone()].iter().zip(rng) {
					if let Some(new_rule) = r1.try_merge(r2) {
						let _ = mem::replace(&mut self.rules[idx1], new_rule);
						self.rules.remove(idx2);
						continue 'outer;
					}
				}
			}
			return self
		}
	}
}


#[derive(Clone, Eq, PartialEq, Hash)]
struct Rule {
	// invariant: an X in assignment implies an X in guard at same position
	guard: [Val; LEN],
	port: Option<PortId>,
	assign: [Val; LEN],
}
impl Rule {
	pub fn no_effect(&self) -> bool {
		self.port.is_none() && self.guard == self.assign 
	}
	pub fn try_merge(&self, other: &Self) -> Option<Rule> {
		if self.port != other.port
		|| &self.assign != &other.assign {
			None
		} else {
			let mut guard = self.guard.clone();
			let mut mismatches = 0;
			for (g, &g2) in izip!(guard.iter_mut(), other.guard.iter()) {
				if g.mismatches(g2) {
					mismatches += 1;
					if mismatches >= 2 {
						return None;
					}
					*g = Val::X;
				}
			}
			let r = Rule::new(guard, self.port.clone(), self.assign.clone());
			println!("combine {:?} + {:?}   TO   {:?}", self, other, &r);
			Some(r)
		}
	}
	pub fn compose(&self, other: &Self) -> Option<Rule> {
		if !self.can_precede(other) {
			return None
		}
		let port: Option<PortId> = self.port.or(other.port);
		let mut guard = self.guard.clone();
		// where the LATTER rule specifies something the FORMER leaves generic, specify it.
		// Eg: [X->X . F->T] becomes [F->T] not [X->T]
		for (&g2, &a1, ng) in izip!(other.guard.iter(), self.assign.iter(), guard.iter_mut()) {
			if a1.generic() && g2.specific() {
				*ng = g2;
			}
		}
		// where the FORMER rule specifies something the LATTER leaves generic, specify it.
		// Eg: [F->F . X->X] becomes [F->F] not [F->X]
		let mut assign = other.assign.clone();
		for (a, &a1, &a2) in izip!(assign.iter_mut(), self.assign.iter(), other.assign.iter()) {
			if a1.specific() && a2.generic() {
				*a = a1;
			}
		}

		Some(Rule::new(guard, port, assign))
	}
	pub fn new(guard: [Val; LEN], port: Option<PortId>, mut assign: [Val; LEN]) -> Self {
		for (g, a) in guard.iter().zip(assign.iter_mut()) {
			if *a == Val::X && *g != Val::X {
				*a = *g; // make assignment more specific
			}
		}
		Self {guard, port, assign}
	} 
	pub fn is_silent(&self) -> bool {
		self.port.is_none()
	}
	pub fn can_precede(&self, other: &Self) -> bool {
		if self.port.is_some() && other.port.is_some() {
			return false;
		}
		for (&a, &g) in self.assign.iter().zip(other.guard.iter()) {
			if a.mismatches(g) {
				return false
			}
		}
		true
	}
}
impl fmt::Debug for Rule {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		for x in self.guard.iter() {
			write!(f, "{:?}", x)?;
		}
		match self.port {
			Some(x) => write!(f, " ={:?}=> ", x)?,
			None => write!(f, " =.=> ")?,
		};
		for x in self.assign.iter() {
			write!(f, "{:?}", x)?;
		}
		Ok(())
	}
}

#[test]
fn testy() {
	wahey()
}

pub fn project(mut rba: Rba, atomic_ports: HashSet<PortId>) -> Rba {
	for rule in rba.rules.iter_mut() {
		if let Some(p) = rule.port {
			if !atomic_ports.contains(&p) {
				// hide!
				rule.port = None;
			}
		}
	}
	let rba2 = rba.normalize();
	rba2
}

pub fn wahey() {
	use Val::*;
	let rba = Rba { rules: vec![
		Rule::new([X,X,F], Some(1), [X,X,T]),
		Rule::new([X,F,T], Some(2), [X,T,F]),
		Rule::new([F,T,T], Some(3), [T,F,F]),
		Rule::new([T,T,T], Some(4), [F,F,F]),
	]};
	println!("BEFORE");
	for r in rba.rules.iter() {
		println!("{:?}", r);
	}
	let atomic_ports = hashset!{2,4};
	let start = std::time::Instant::now();
	let rba2 = project(rba, atomic_ports);
	println!("ELAPSED {:?}", start.elapsed());
	println!("AFTER: {:#?}", rba2);
}