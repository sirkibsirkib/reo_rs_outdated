#Idea
we are given a protocol with a global automaton.
ultimately, this automaton has an "interface", a set of ports which must be 
connected to those of atomic components.
Given a subset of the interface ports called "local ports", we wish to create an
API for an atomic given those ports such that IF the atomic performs blocking
gets and puts as allowed by the API, the entire system will never livelock or 
deadlock "inside the circuit".

# CA token API
## Info representation
implicit: state of memory cells
explicit: the automaton state and which transitions are possible next.

## Naive Idea
TODO

## algorithm
### Local projection
take global CA.
-- Note that CA does not need to represent memory cells so the
	only annotations are port PUTS and GETS
-- do hiding of everything not in "local ports":

#### Hiding:
1. replace all occurrences with port operations NOT in "local ports" with dot •.
	this may create things like (1.get AND •). 
2. rewrite instances of (_x_ AND •) and (• AND _x_) to _x_
this gives local CA.
3. remove "•" transitions (those that really correspond to no action at all).
	* remove all u--•-->u. (don't represent anything meaningful)
	* consider states {u,v} with u--•-->v.
		1. add transitions u--#-->w for every v--#-->w
		2. remove u--•-->v.

### Api generation
take local CA
1. for every state _u_ define token `State<u>`.
2. for every state _u_ and port _p_ define token `Coupon<u,p>`.

The api generated has the following characteristics:
1. put and get operations for some port _p_ will consume `Coupon<p,s>` and return `State<s>`.
2. every state defines a sum type `BranchOpts` for all concrete branches in the
	automaton from that state.
2. states are consumed when `advance` is invoked on them. The function requires
	as input a `FnOnce(O)` where O is the `BranchOpt` for the input state.

## User's perspective
1. users cannot invoke put or get without the appropriate coupon
2. users always have exactly 1 token in {state, coupon}.
3. `advance` converts tokens: state => coupon.
4. `put` or `get` converts tokens: coupon => state.

## Optimizations
1. 

## Pros and Cons of this approach

-------------------------------------------------------------------------

# Rule-based token API

## Info representation
implicit: the automaton state and which transitions are possible next.
explicit: state of memory cells

## Idea