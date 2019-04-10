
pub trait Protocol {
	type Interface;
	fn new() -> Self::Interface;
	fn port_automaton() -> PortAutomaton;
}


struct PortAutomaton;