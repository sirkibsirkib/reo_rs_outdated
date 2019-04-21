#Idea
we are given a protocol with a global automaton.
ultimately, this automaton has an "interface", a set of ports which must be 
connected to those of atomic components.
Given a subset of the interface ports called "local ports", we wish to create an
API for an atomic given those ports such that IF the atomic performs blocking
gets and puts as allowed by the API, the entire system will never livelock or 
deadlock "inside the circuit".

# Assumptions & views
1. the protocol defines, for every state of the global system, a set of permitted
	actions available to atomic components.
2. atomics make no assumptions about the actions of other atomics (aside from
	knowing that they adhere to the protocol). the only way to alter the knowledge
	one atomic has of the behaviour of another, is to alter the protocol.
2. atomics may only COMMIT to an action if they are certain that the decision
	will succeed. (ie. will not be contradicted by another atomic's commit).
Derived:
1. an atomic can safely commit to action a if the set of possible actions in
	the current state is {a}.
2. if the set of possible actions in the current state is {a, b, ...}, the atomic
	is unable to commit to either without outside help. Thus, we introduce an
	operation that effectively ASKS the protocol which one to commit to.

----------------------------
# CA token API
## Info representation
implicit: state of memory cells
explicit: the automaton state and which transitions are possible next.

## Idea
The type system encodes states in the protocol's CA.
All atomics (and the protocol, in its own way) transition in lockstep through
their automata. 

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

NOTE: if any transition is thereby left with (a AND b) where a!=b, this port set
requires synchronous firing and this local projection would create an unusable API.
Return ERROR.

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

## Pros and Cons of this approach
1. state explosion results in 

-------------------------------------------------------------------------

# Rule-based token API

## Info representation
implicit: the automaton state and which transitions are possible next.
explicit: state of memory cells

## Idea
abandon the idea of state tokens. Instead, we mimick the memory cell space by
defining generic token T (with instantiations `T<True>`, `T<False>`, and
`T<Unknown>`) for every memory cell "T".

For example, let us consider a system with memory cells {T,U}.
At any moment, the atomic has exactlyl one variant of each token in {T,U}.
The user has access to functions of the form:

fn advance(T<?>, U<?>, FnOnce(X) -> (T<?>, U<?>)) -> (T<?>, U<?>) {
	
}




## algorithm


---------------- EXPERIMENTATION ----------

```
notation:
{m}	m must be SOME
{~m} m must be NONE
{} we don't specify m


BEFORESTATE | FIRING | AFTERSTATE
1. {m} =={3}==> {~m}
2. {~m} =={1,2,3}==> {m}

WITH HIDING

1. {m} =={3}==> {~m}
2. {~m} =={1,2,3}==> {m}
shit

/////////////////////////////////////
try represent the merger-replicator circuit:
  _____________
 1             |
[X]2<-._[fifo]<' 
[Y]3<-'       <.
 4_____________|
 
1. {~m} =={1}==> {m}
2. {~m} =={4}==> {m}
3. {m}  =={2,3}==> {~m}

the only restriction on atomics is that {2,3} cannot be grouped, as they have synchronous behavior.
consider grouping {1,2}

start state {~m}
1. {~m} =={1}==> {m}
2. {~m} =={•}==> {m}
3. {m}  =={2,•}==> {~m}

(~m) ---1+•--> (m)
     <---2&•---
```

# IDEA
RBA and CA are two extremes on a continuum between runtime and compile time information.
(actually: state-space and guard-predicate-space information)
RBAs dont implicitly track the progress through the automaton, so they remember 
changes by manipulating logical variables (V and !V are interpreted
as memory cell V is full and empty respectively).
full rule-based form is therefore a degenerate automaton with 1 state. guards check states of memory cells and perform transitions that alter logical variables.
one could freely change between RBA and CA by representing memory cells
in state space or vice versa. state-space requires 2^N states, memory
cells require N spaces.
we start with RBA. first we project onto the local port set. next, we perform
hiding. let 
while ??:
1. select memory variable M (pick one we DO want to distinguish)
2. partition transitions on every state X into new states {X, X'} where
	all with M go to X and all with !M go X'.
	Repeat as desired.
3. hide all remaining memory variables and coalesce with + operators.
4. bob's your uncle.

## Two representations
### ONE
Guard = (BeSub)
Ready set has `P + (M * 2)` bits
memory cells have a cell for their COMPLEMENT
>>> take care: always UNFLIP a memory bit when you FLIP the other 

### TWO
Guard = (Mask, BeMatch)
Ready set has `P + M` bits
>>> take care: don't accidentally express "port must NOT be ready".
Instead of subsets, guard must be an exact match, and irrelevant bits are masked to 0 first.