use crate::reo::*;
use hashbrown::HashMap;
// use crate::protocols::*;

use crossbeam::scope;
use crossbeam::channel::Select;
use bit_set::BitSet;

struct Producer {
	p00p: PortPutter<u32>,
}
impl Component for Producer { fn run(&mut self) {
	for i in 0..1000 {
		self.p00p.put(i).unwrap();
	}
}}

struct Consumer {
	p01g: PortGetter<u32>,
}
impl Component for Consumer { fn run(&mut self) {
	while let Ok(x) = self.p01g.get() {
		println!("{:?}", x);
	}
}}

struct ProdConsProto {
	p00g: PortGetter<u32>,
	p01p: PortPutter<u32>,
}
impl ProdConsProto {
	const P00G_BIT: usize = 0;
	const P01P_BIT: usize = 1;
}
impl Component for ProdConsProto { fn run(&mut self) {
	let mut running = true;
	let guards = vec![
		(bitset!{Self::P00G_BIT, Self::P01P_BIT},
			// TODO data constraint
			|me: &Self| { me.p01p.put(me.p00g.get()?) }),
	];

	let mut ready = BitSet::new();
	let mut sel = Select::new();
	let bitmap = map! {
		sel.recv(self.p00g.inner()) => Self::P00G_BIT,
		sel.send(self.p01p.inner()) => Self::P01P_BIT,
	};
	while running {
		let sel_flagged = &sel.ready();
		ready.insert(*bitmap.get(sel_flagged).unwrap());
		for g in guards.iter() {
			if ready.is_subset(&g.0) {
				if (g.1)(&self).is_err() {
					running = false;
				};
				ready.difference_with(&g.0);
			}
		}
	}
}}

#[test]
fn sync() {
	let (p00p, p00g) = new_port();
	let (p01p, p01g) = new_port();
	scope(|s| {
		s.builder().name("Producer".into()).spawn(
			|_| Producer{p00p}.run()).unwrap();
		s.builder().name("ProdConsProto".into()).spawn(
			|_| ProdConsProto{p00g, p01p}.run()).unwrap();
		s.builder().name("Consumer".into()).spawn(
			|_| Consumer{p01g}.run()).unwrap();
	}).unwrap()
}