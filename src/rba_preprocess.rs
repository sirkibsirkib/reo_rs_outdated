type PortId = u32;
const LEN: usize = 3;
use std::fmt;



use derive_new::new;

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
}

#[derive(Debug, Clone)]
struct Rba {
	rules: Vec<Rule>,
}
impl Rba {
	pub fn count_silents(&self) -> usize {
		self.rules.iter().filter(|r| r.is_silent()).count()
	}
	pub fn normalize(&self) -> Rba {
		let mut buf = vec![];
		let mut rba = self.clone();
		while let Some(idx) = rba.first_silent_idx() {
			let silent = &rba.rules[idx];
			println!("TODO. Remove silent rule at idx={}", idx);
			for (i,r) in rba.rules.iter().enumerate() {
				if i == idx {continue}
				if let Some(composed) = r.compose(silent) {
					println!("ADDING rule ({},{})", i, idx);
					buf.push(composed);
				}
			}
			let _ = rba.rules.remove(idx);
			rba.rules.append(&mut buf);
			println!("AFTER: {:#?}\n----------------", &rba.rules);
		}
		rba
	}
	pub fn first_silent_idx(&self) -> Option<usize> {
		self.rules.iter().enumerate().filter(|(_,r)| r.is_silent()).map(|(i,_)| i).next()
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
			use Val::*;
			match [a,g] {
				[T,F] | [F,T] => return false,
				_ => (),
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
	// let rba = Rba { rules: vec![
	// 	Rule::new([X,F], Some(1), [T,X]),
	// 	Rule::new([T,F], Some(2), [F,T]),
	// 	Rule::new([T,X], None   , [F,F]),
	// ]};
	let rba = Rba { rules: vec![
		Rule::new([X,X,F], None   , [X,X,T]),
		Rule::new([X,F,T], None   , [X,T,F]),
		Rule::new([F,T,T], None   , [T,F,F]),
		Rule::new([T,T,T], Some(1), [F,F,F]),
	]};
	println!("BEFORE");
	for r in rba.rules.iter() {
		println!("{:?}", r);
	}
	println!("AFTER");
	for r in rba.normalize().rules.iter() {
		println!("{:?}", r);
	}
}
