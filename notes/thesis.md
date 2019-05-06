My goal:
* Develop the Reo compiler and build a Reo runtime that supports the creation
of efficient C, C++ and Rust applications. The bulk of the novelty will lie
in the introduction of a type-constraint system. The absence or presence of these
properties will determine how the circuit is compiled. 

Breakdown:
1. Abstract
1. Intro
1. Background
	1. Coordination vs. Computation
	2. The Reo Language
	3. Compiling Reo to Native Source
	4. The Current State of Reo Tools
		1. The existing Reo Compiler 
		2. The existing Java Runtime
2. Type Properties
	1. Terminology
		<!-- clonability, independence, pointers etc. -->
	2. Case-distinct data operations
		<!-- how to clone something. replicators. linear types -->
	3. 
3. Reo Runtime
	1. Components and Ports
	2. Protocol as Component
		1. Event-driven progression
	3. Properties of Types
	4. FFI with C and C++
	5. Wrapper Types
		1. Box
		2. Arc<Mutex<T>>
		3. Linear
4. Experimentation
5. Future work
6. Conclusion

