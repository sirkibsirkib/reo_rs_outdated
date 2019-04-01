# TODO discuss
1. ports CLOSING
	1. happens when 1+ peer closes OR unreachable inside protocol (for propagation)
	2. Can you define behavior that depends on whether a port has closed?
	3. Can ports be revived or Created at runtime?
1. Channel type
	1. Can a channel have no type? What does that even mean?


# Reo Compiler
1. Remove the use of "Object" as a fallback type
1. Memory cells need to acquire actual types (not just "Object")
2. Type propegation down the wire
3. auxiliary type info rules

# Reo_rs
1. Explore using writable() and readable() readiness to represent ready and dead
	* these calls are mutually-exclusive for a port. it must guarantee that
		if it ever raises READY it won't raise DEAD until get or put are called to lower READY
		(alternatively: other rules such as DEAD is not removed until no longer ready. idk)
		(alternatively2: DEAD signal just ASKS for death. deadbitset decoupled to allow these cases)
2. Change the Action closure signature never to return error. should panic.
	* After the firing constraint is evaluated, if any DEAD ports are detected.
		* kill all waiting ports (drop in place?) consider Option<Port>.
	* by the time Action is executed, any portErrors raise panic, since it should have been taken care of


///////////////////////
look into rust deps. we want to remove Component trait
make .o files.

create the main function
hook missing components to windows 
primitivees can be cmdline args