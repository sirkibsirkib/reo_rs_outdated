type PortId = u32;
const LEN: usize = 4;
use std::{fmt, mem};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
		match [self, other] {
			[T,F] | [F,T] => true,
			_ => false,
		}
	}
}

#[derive(Debug, Clone)]
struct Rba {
	rules: Vec<Rule>,
}
impl Rba {
	pub fn normalize(&self) -> Rba {
		let mut buf = vec![];
		let mut rba = self.clone();
		while let Some(idx) = rba.first_silent_idx() {
			let silent = &rba.rules[idx];
			println!("TODO. Remove silent rule at idx={}", idx);
			for (i,r) in rba.rules.iter().enumerate() {
				if i == idx {continue}
				if let Some(composed) = silent.compose(r) {
					println!("ADDING rule ({},{})", i, idx);
					buf.push(composed);
				}
			}
			let _ = rba.rules.remove(idx);
			rba.rules.append(&mut buf);
			println!("AFTER: {:#?}\n----------------", &rba.rules);
		}
		rba.minimize()
	}
	pub fn first_silent_idx(&self) -> Option<usize> {
		self.rules.iter().enumerate().filter(|(_,r)| r.is_silent()).map(|(i,_)| i).next()
	}
	pub fn minimize(mut self) -> Self {
		'outer: loop {
			for (idx1, r1) in self.rules.iter().enumerate() {
				let rng = (idx1 + 1)..;
				for (r2, idx2) in self.rules[rng.clone()].iter().zip(rng) {
					if let Some(new_rule) = r1.pair_collapse(r2) {
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


#[derive(Clone)]
struct Rule {
	// invariant: an X in assignment implies an X in guard at same position
	guard: [Val; LEN],
	port: Option<PortId>,
	assign: [Val; LEN],
}
impl Rule {
	pub fn pair_collapse(&self, other: &Self) -> Option<Rule> {
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
		let assign = other.assign.clone();
		// where the LATTER rule specifies something the FORMER leaves generic, specify it.
		// Eg: [X->X . F->T] becomes [F->T] not [X->T]
		for (&g2, &a1, ng) in izip!(other.guard.iter(), self.assign.iter(), guard.iter_mut()) {
			if a1.generic() && g2.specific() {
				*ng = g2;
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

pub fn wahey() {
	use Val::*;
	let rba = Rba { rules: vec![
		Rule::new([X,X,X,F], None   , [X,X,X,T]),
		Rule::new([X,X,F,T], None   , [X,X,T,F]),
		Rule::new([X,F,T,T], Some(2), [X,T,F,F]),
		Rule::new([F,T,T,T], None, [T,F,F,F]),
		Rule::new([T,T,T,T], Some(1), [F,F,F,F]),
	]};
	println!("BEFORE");
	for r in rba.rules.iter() {
		println!("{:?}", r);
	}
	for r in rba.normalize().rules.iter() {
		println!("{:?}", r);
	}
}
