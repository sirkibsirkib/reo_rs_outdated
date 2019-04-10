
use crate::threadless2::{Putter, Getter};

pub trait Protocol {
	type Interface: PortTuple;
	fn new() -> Self::Interface;
	fn port_automaton() -> PortAutomaton;
}

pub struct PortAutomaton;

pub trait PortTuple {}
pub trait Port {}

impl<T:Clone> Port for Putter<T> {}
impl<T:Clone> Port for Getter<T> {}

impl<A> PortTuple for (A,) where A: Port {}
impl<A,B> PortTuple for (A,B,) where A: Port, B: Port {}
impl<A,B,C> PortTuple for (A,B,C,) where A: Port, B: Port, C: Port {}
impl<A,B,C,D> PortTuple for (A,B,C,D,) where A: Port, B: Port, C: Port, D: Port {}

////////////

struct MyProto;
impl Protocol for MyProto {
	type Interface = (Getter<u32>, Putter<u32>);
	fn new() -> Self::Interface {
		unimplemented!()
	}
	fn port_automaton() -> PortAutomaton {
		unimplemented!()
	}
}