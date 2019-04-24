# General

## Concepts:
1. The system can move from state to state by performing ACTIONS (correspond 1-to-1 to guarded cmds).
2. An atomic cannot make any assumptions about how foreign ports are grouped into other atomics.
3. [from #2] it suffices for an atomic to model the actions of foreign ports as the actions of
	the protocol itself, constrained ONLY by the behaviours defined in the protocol.
4. when a state has 2+ possible transitions from the current state, the protocol itself has
	the final word on which of the transitions are realized.
5. Definition of atomic with port set S is INVALID if for any transition in the protocol,
	2+ ports of S are involved (requires the atomic to fire 2 ports synchronously).

## Intuition & Paradigm:
execution can be viewed as a WALK in the protocol's automaton. Each atomic
envisions a flood from the current state through transitions. The flood is impeded by
transitions involving one of its local ports. The set of POTENTIAL next actions is
defined as the port-set involved in these flood-reached transitions. The atomic
commits to (ANY 1) of these operations, and awaits a "callback" from the protocol
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
	of possible next actions, the concrete selection of which has not yet been determined.
	* "Coupon": corresponding to a transition. Gives the atomic permission to interact
	with a port and acquire the next State token.
2. The atomic's "main" function is invoked given the starting "State" token.
3. The atomic's function is provided a signature that prohibits returning: `... -> !`.

## CA vs RBA
Automaton | CA | RBA
--- | --- | ---
Representing State | One type, corresponds with CA state | A tuple of memory variable tokens
Representing Transitions | Each state is associated with an `Option` enum, with one variant per transition reachable in this state | RBAs define guarded commands, each of which corresponds to a set of concrete transitions. Guarded-cmds map to generic functions; the monomorphism of each guarded command corresponds to the mapping from guarded command to transition set. The set of guarded commands available given the current state tuple represents the next reachable transitions.
State space explosion | Inherent in the CA. The tokens do not change that. | Explosion is introduced in the monomorphization of the guarded-command-functions, as such its evaluated by the compiler lazily according to whichever ones the user instantiates.

## Preference
The system as outlined above will ensure that every branch in the automaton is
available for the protocol to "choose from". with all branches potentially becoming 
available at once.

In traditional Reo applications, the guarded commands were evaluated eagerly, whenever possible.