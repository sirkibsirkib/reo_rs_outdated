
# Ch 1 intro

# Ch 2 Reo backend
1. reference implementation: java generator
	1. structure
	2. behavior: rules
	3. observations
1. goals
	1. functional requirements
		1. features 
			(same features as java version)
			(flexible initialization)
			(FFI)
			(termination detection)
		2. safety
			(value passing semantics)
			(safer port connections)
			(mem init safely)
	2. non-functional requirements
		1. performance
			(support larger datatypes)
			(stack allocation)
			(protocol guard eval)
		2. maintainability
1. code generation
	1. api trait interface
	2. reo side:
		1. type constraining
		2. imperative form
	3. rust side:
		1. checking and fallability
		2. optimization pass
		3. commandification
1. rust runtime properties
	1. user-facing
		1. proto construction
		2. port claiming
		3. teardown and termination
	2. internal
	TODO
		1. protocol as data
		1. value passing ports
		2. memory cell allocator
		3. 
		2. coordinator
1. goals evaluated and summary

# Ch 3 Static Governors
1. problem: unintended constraints
1. governor defined
2. solution: static governance with types
3. functionality
	1. encoding CA and RBA as type-state automata
	1. rule consensus
	1. Governed Environment
		1. Governor Entrypoint
		1. Port Wrappers
4. functionality
	1. RBA simplification
	1. RBA preprocessing
		1. projection
		1. normalization
	1. opt-in simplification
	1. match macros