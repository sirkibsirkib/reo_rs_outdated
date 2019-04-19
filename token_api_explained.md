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

1. replace all occurrences with port operations NOT in "local ports" with dot •.
	this may create things like (1.get AND •). 
2. rewrite instances of (_x_ AND •) and (• AND _x_) to _x_
this gives local CA.

### Api generation
take local CA
1. for every state _u_ define token `State<u>`.
2. for every port _p_ in the "local ports" define token `Coupon<p>`.
3. define token `Receipt`.

The api generated has the following characteristics:
1. put and get operations for some port _p_ consume `Coupon<p>` and return `Receipt`.
2. the atomic is provided a "controller" object with function `advance_x` for every state _x_
	which has parameters:
	* `State<x>` which is consumed
	* `FnOnce(B) -> Receipt` where "B" is an enum, with variants correspondong to precisely the
		neighbourhood set (the outgoing transitions) of _x_.
		Variants representing a transition with port _p_ provide `Coupon<P>`
		ie  being represented as `PortP(Coupon<P>)`.
		If present, the dot (•) transition manifests as variant `Idle(Receipt)`.
	* The transition returns the sum of possible reachable states from _x_ by one transition.
		where each variant, when matched, returns the appropriate state token.

## User's perspective
1. users cannot invoke put or get without the appropriate coupon
2. users have no `Receipt` tokens outside Fnonce closures.
3. users don't have any coupons OUTSIDE FnOnce() closures because:
	* the closure doesnt return without a receipt
	* receipts can only be created by destroying a coupon
	* every branch gives either 1 coupon or 1 receipt
4. the branches available at any moment is a function of the state
5. the user has:
	* 1 state token (outside any FnOnce)
	* 0 state tokens (inside any FnOnce)
5. the user is not in control over which state they are in because:
	* they have no control over the state they begin with
	* they have no control over the state the advance function returns
	* they have precisely 1 advance function they can call

## Optimizations
1. 

## Pros and Cons of this approach

-------------------------------------------------------------------------

# Rule-based token API

## Info representation
implicit: state of memory cells
explicit: the automaton state and which transitions are possible next.

## Idea