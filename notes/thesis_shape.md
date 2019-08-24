
----------------------------------------
## PART 1: Preliminaries
# Ch 1 intro
programming concurrent applications is hard and backwards. abstractions lag behind.
Reo is a language for specifying protocols.
It has tooling that takes advantage of this to generate coordination glue code
for your computation code. "Reo compiler".
The compiler has support for several target languages, but not all.
Some work has gone into C, C++ and Rust targets in the past to varying degrees of
success, but not sufficiently to be integrated into the compiler.
The Java backend is well-established and has much experimentation and maintenance.
The backend is primarily intended for sending primitives which are not subject to
data races; in other cases it has problems with correctness and the code generated
has some flaws with overwriting memory.

In this thesis we cover contributions partitioned into three chapters which roughly
follow in sequence: (a) imperative form, a new intermediate protocol representation
for removing language specifics from the Reo compiler for imperative targets.
(b) A Rust library that constructs protocol objects that act as the specified coordinators
(c) A granular design for a code generator which generates a dependency for Rust;
when used in conjunction with a Rust protocol, the user's Rust compiler itself
verifies protocol adherence at compile time. 

# Ch 2 background
1. Reo
	1. goal
	2. language
	3. semantic models
	4. the Reo compiler

2. target languages
	1. affine types
	2. the rust language
	2. programming patterns
		1. type-state
		2. proof of work

# Ch 3 Related Work

----------------------------------------
## PART 2: Contributions
# Ch 4 Imperative Form
1. role in the Reo pipeline
1. definition
1. Reo to Imperative Form
	1. Compiler Internal Representation
	2. Action Sequencing
	3. Type clusterinig and Constraining
3. Imperative Form to Rust:
	1. Well-formedness checks
	2. Initial memory values
	3. Preprocessing optimizations
		(avoid checks) (separate out values)

# Ch 5 	Protocol Runtime
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

# Ch 7 Static Component Governors
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
	1. match macros

# Ch 8 Benchmarks
1. goal
2. experimental setup
1. Rust vs Java
	1. large values
	2. small values
	3. with heap allocation
2. With without governor
3. (test effect of not having proto thread)
	1. many rules
	2. resource contention

----------------------------------------
## PART 3: Reflection

# Ch 9 Discussion
1. future work:
	1. distributed: Reowolf current project at CWI. sockets problems
	2. smaller stuff
	3. governors, software layer -> more low level
2. conclusion



