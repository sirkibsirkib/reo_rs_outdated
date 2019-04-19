struct PortAutomaton {
	num_states: usize,
	transitions: HashMap<usize, HashSet<Transition>>, 
}

struct Transition {
	to: usize,
	constraint Formula,
}

enum Formula {
	True,
	Negation(Box<Formula>),
	And(Vec<Formula>),
	Or(Vec<Formula>),
	Equals([PutterId; 2]),
}

struct PutterId(usize);