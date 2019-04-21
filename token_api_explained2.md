# General

## Concepts:
1. The system can move from state to state by performing ACTIONS (correspond 1-to-1 to guarded cmds).
2. An atomic cannot make any assumptions about how foreign ports are grouped into other atomics.
3. [from #2] it suffices for an atomic to model the actions of foreign ports as the actions of
	the protocol itself, constrained ONLY by the behaviours defined in the protocol.
4. when a state has 2+ possible transitions from the current state, the protocol itself has the final word on which of the transitions are realized.
5. Definition of atomic with port set S is INVALID if for 1+ transitions in the protocol,
2+ ports of S are involved in the same synchronous transition.

## Intuition & Paradigm:
execution can be viewed as a WALK in the protocol's automaton. Each atomic
envisions a flood from the current state through transitions. The flood is impeded by
transitions involving one of its local ports. The set of POTENTIAL next actions is
defined as the port-set involved in these flood-reached transitions. The atomic
commits to the SUM of these operations, and awaits a "callback" from the protocol
to reify the specific transition. this callback also brings with it information
about the new state of the automaton (immediately after the transition).

In either case, an atomic must consider the presence of transitions that DO NOT
involve it (as they represent the system's ability to change state without the 
atomic's involvement), however, the automaton does not need to care about the NATURE
of these foreign actors (just distinguish between their presence or absence).

The same local port firing for a different transition is ONLY relevant to the atomic
insofaras the resulting new state may differ.

## Tokens
A well-behaved atomic can be implemented following this algorithm by being
implemented in a corresponding fashion. Ie:
```
loop:
	(which_port, new_state) = protocol.determine(options)
	ports[which_port].port_action()
	state = new_state
	options = next_options(new_state)
```
This system is encoded into affine token types to leverage the compiler's type
system itself to impose adhrence to this paradigm on the atomic's programmer.
Done correctly, an atomic will:
1. compile <=> it adheres to the paradigm
2. be able to exercise choice insofar as it is expressed by the protocol.

###  Token Encoding (general to CA and RBA)
1. There are two "kinds" of affine token:
	* "State": representing the state of the automaton. associated with a SET
	of possible next actions, the concrete option of which has not yet been determined.
	* "Coupon": corresponding to a transition. Gives permission to interact with a port and
	acquire the next State token.
2. Some affine type is used to represent the state of the automaton.
3. The atomic's "main" function is invoked given the start token.
4. The atomic's function is provided a signature that prohibits returning: `fn foo() -> !`.

# CA
## Representation Specifics
1. The global state is coalesced into a concrete name
2. states are annotated with the precise set of transitions, of which one will be next.


# RBA
## Representation Specifics
1. The global state is represented as a tuple of (binary) logical variables.
2. the set of transitions available next is not trivially determined; it is the
	set of transitions for which the current state SATISFIES a guard predicate.
