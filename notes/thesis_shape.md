
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
	1. two-step generation
		(can work without but causes repetition, "general imperative". implementation requires a bunch of actions. the firing in particular)
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
		1. protocol object architecture
			3. read-only data
			2. critical region
			3. implicitly protected region
		1. rule firing
			1. coordinator in critical region
			2. readiness
			3. value distribution
			4. persistent and temporary values
		1. design choices and optimizations
			1. dense bit sets
			1. memory storage and allocator
			2. type reflection
			3. port operation variants
				1. signal
				2. timeout
				3. lossy
			1. port creation and destruction
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
4. feasibility
	1. RBA simplification
		1. motivation
		2. consequence: loss of distinction
	1. RBA preprocessing
		1. projection and hiding
		1. normalization
			1. purpose
			1. algorithm
	1. opt-in simplification
		1. 
	1. match macros

# Ch 4 Benchmarks
1. Rust vs Java
	1. large values
	2. small values
	3. with heap allocation
2. With without governor
3. (test effect of not having proto thread)
	1. many rules
	2. resource contention

# Ch 5 Discussion
1. future work:
	1. distributed: Reowolf current project at CWI. sockets problems
	2. smaller stuff
	3. governors, software layer -> more low level
2. conclusion


