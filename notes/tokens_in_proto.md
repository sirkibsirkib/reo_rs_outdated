# Idea
1. protocols have several sets:
-- Ready
-- Committed
2. the proto is able to call back Ready bits by sending them a message: `GetReady`
	they respond with `AckUnready<&IdSet>`, telling the proto which other bits became unready


# Static Atomic groupings
1. before being able to interact with a port, the protocol needs to know how
	the ports are grouped.
	with each guarded cmd, it can know 

# Difficult concepts:
1. Api dot hiding
2. RBA state weakening
3. RBA multiple rules possible?
3. ready vs committed bitsets
4. proto statically reasoning about atomic readiness


----------
# The game
It's always safe to give the atomic something that looks like:
```
loop {
	let x = ask_proto()
	match x {
		// a match arm for EVERY port in portset.
		// eg: ports[1].activate()
	}
}
```
However, we wish for the atomic to be able to "prune" branches in these match arms.
Ideally, we want the atomic to be able to (statically) reason about the system's
state insofaras it is possible such that it can have more control over which
possibilities are possible. Ideally, we want to identify in every situation 
EXACTLY the set of ports that the protocol may require next. 

# terminology
* gcmd: "guarded command" of RBA. written {guard} ={ports}=> {result} where
	guard: a set of "checks", assertions on boolean variables eg: {a, !b}
		"a is true, b is false and all else are unspecified".
	ports: set of ports "involved" in the guarded command.
	result: a set of assignments to boolean variables

For boolean* variables we also consider a third element represented in the atomic's
local view only: "unknown". Represents uncertainty about the concrete value.
intuition: If we don't know `a`, we need to consider gcmds that require `a` and 
	also those that require `~a`.

# how to minimize an RBA?
We consider an atomic with port set {1}
ie. how do we produce an atomic's local view of the gcmds 
Here we consider some problematic observations:

## TOO CONSERVATIVE to hide memory cells I never interact with.
eg rules:
1. {a}={1}=>{b}
2. {b}={ }=>{c}
3. {c}={ }=>{d}
4. {d}={1}=>{e}
// atomic can reason about the fact that after rule 1 they will certainly participate in 
// rule 4, but this requires considering memory cell 'c' which the atomic
// never interacts with.

## TOO CONSERVATIVE to use all foreign gcmds to weaken outputs of my own
eg rules:
1. {a}={1}=>{b,c}
2. {b,!c}={ }=>{!b}
here it seems like the local view of rule 1 should be `{a}={1}=>{b?,c}` because
rule 2 seems able to change `b` to `!b` without our involvement. However, we miss the
fact that after rule 1, rule 2 cannot be applied.

## Cannot always reason about generic GCMDS
eg rules:
1. {a,!c}={1}=>{b,!c}
1. {a,c}={1}=>{b?,c}
2. {c}={ }=>{!b}
... 
Do we weaken rule 1 to `{a}={1}=>{b?}`? Whether 2 can be applied after 1 depends on variable
`c`, so in SOME instances of 1, yes, but in others, no. We need to fragment rule 1
into rule 1a and rule 1b. Could this approach simply lead to CA-like state explosion?


# Conclusions
1. The most accurate approach may lead to state explosion anyway.
2. Is the approximate approach good enough?

## Test 1
fifo-chain
Protocol RBA rules:
1. { ,!a}={ }=>{  , a} // input
2. {a,!b}={ }=>{!a, b}
3. {b,!c}={1}=>{!b, c}
4. {c,!d}={1}=>{!c, d}
5. {d,!e}={ }=>{!d, e}
6. {e,  }={ }=>{!e,  } // output

Goal: want the atomic to recognize that its safe to do [get, put]*
	and that [get, {get,put}]* is too generous.

### Context-free weakening approach
1. { ,!a}={ }=>{  , a} // input
2. {a,!b}={ }=>{!a, b}
3. {b,!c}={1}=>{!b, c}
4. {c,!d}={1}=>{!c, d}
5. {d,!e}={ }=>{!d, e}
6. {e,  }={ }=>{!e,  } // output

identified all "invisible changes" that may EVER happen:
a: T<->F
b: T<--F
d: T-->F
e: T<->F

perform weakening:
rules:
... just participating
3. {b,!c}={1}=>{!b, c}
4. {c,!d}={1}=>{!c, d}
... weaken inputs
3. {!c}={1}=>{!b, c}
4. {c}={1}=>{!c, d}
... weaken outputs
3. {!c}={1}=>{b?, c}
4. {c}={1}=>{!c, d?}
... prune further
// never check b, d
3. {!c}={1}=>{c}
4. {c}={1}=>{!c}
states:
	proto start: {!a, !b, !c, !d, !e}
	my start: {!c}

api does:
[3;4]*

ok so that works

## Test 2
Protocol RBA rules:
1. {a}={1}=>{!a,b,c}
2. {b,!c}={ }=>{!b}
3. {b}={2}=>{a}
4. {!b}={3}=>{a}
(observe: once 1, never 2. no 2 until 1)

identified all "invisible changes" that may EVER happen:
b: T<--F

rules:
... just participating
1. {a}={1}=>{!a,b,c}
3. {b}={2}=>{a}
4. {!b}={3}=>{a}
... remove arbs
... weaken inputs
1. {a}={1}=>{!a,b,c}
3. {b}={2}=>{a}
4. { }={3}=>{a}
... weaken outputs
1. {a}={1}=>{!a,b,c}
3. {b}={2}=>{a}
4. { }={3}=>{a}
... prune further
// never check c
1. {a}={1}=>{!a,b}
3. {b}={2}=>{a}
4. { }={3}=>{a}
states:
	protocol: {!a, !b, c}
	start: {!a, b?, c}

api does:
[{3,4}; 1]*
eg: 3,1,4,1,4,1,4,1,3,1,4,1...

real solution:
[4;[1;3]* ]
eg: 4,1,3,1,3,1,3,1,3...

problem here is the weakening step. we assume just because rule2 MIGHT make b false,
it is ALWAYS able to make b false.

## Test3
modification: don't weaken GLOBALLY.
rules:
1. {a}={1}=>{b} // generic over c
2. {c}={ }=>{!b} // not participating
3. {!b}={2}=>{a} // generic over c
4. {b}={2}=>{a} // generic over c

identify (conditional) invisible changes:
when c :: b T-->F

... just participating
fork rules that WOULD be weakened according to a generic

1. {a}={1}=>{b} // generic over c. output b may be weakened
3. {!b}={2}=>{a} // not weakened
4. {b}={2}=>{a} // generic over c. input may be weakened
===>

1. {a,c}={1}=>{?b} // c specified. output b weakened
1. {a,!c}={1}=>{b} // c specified. no weakening
3. {!b}={2}=>{a} // not weakened
4. {c}={2}=>{a} // c specified. input weakened
4. {b,!c}={2}=>{a} // c specified. no weakening.

... clean
NOTE: 	input: generic over all unspecified. require specified 
		in output: preserve state EXCEPT for overwriting values with either true, false or ?


1. {a,c}={1}=>{?b}
2. {a,!c}={1}=>{b}
3. {!b}={2}=>{a}
4. {c}={2}=>{a,c}
5. {b,!c}={2}=>{a,!c}

states:
	global: {!a, !b, !c}
	local: {!a, !b, !c}

api does:
named:
S0 : {!a, !b, !c}
S1 : {a, !b, !c}
S2 : {a, b, !c}


# Observations:
1. the deeper I dig, the more this degenerates to full enumeration a la CA.
2. the resulting CA is also in a sense not minimal either, as we ultimately don't 
	want to distinguish states which differ only in memory cells. really what 
	matters is which ports can fire and which states are reachable with which transitions.

example:
states: {x,y,z} ports: {0,1}

x-{0}->y
y-{0}->y
y-{1}->z
z-{0}->z
z-{1}->z

we observe that y and z arent really distinguishable. can do something very
similar to DFA minimization algorithm.
x-{0}->y
y-{0}->y
y-{1}->y
is much better

new problem: the notion of "weakening" is flawed. we still would have to 
generate a big old CA to keep track of the internal steps. Lots of work!
I observe that true CA minimization is not as simple as contracting 
two nighbouring states. eg: fuse a and b.

a-->b   a-->{b,c}-->d
|   |
v   v
c-->d

# Correct RBA projection algorithm:
while new states:
1. internal steps
2. branch to neighbouring states with port-transition
minimize:
1. "color" nodes according to outgoing ports
2. categorize nodes according to coloring
3. fragment a partition into distinguishable halves (some transition takes them to different destination groups)
4. parts --> nodes in new automaton.

problem: creating CA requires enumerating all states in a sense. we want a smarter way of representing "groups" of states which we don't care about distinguishing

# observation
RBA is essentially useful because you can just greedily apply rules as you encounter them at runtime.
This doesn't work for us since the type system cannot prevent you from compiling something that may have issues at runtime.

the main issue is that it seems senseless to fully enumerate memory cells AND THEN
minimizing according to firing ports. is there a way we can represent states
in terms of RULES THAT CAN FIRE or PORTS INVOLVED or sth?

clearly even the token API (in CA form) can blow up given a small RBA formulation
eg: emulate binary counting using memory cells.
[00000] is 6 empty cells. 
rules:
[xxxx0] ={1}=> [xxxx1]
[xxx01] ={1}=> [xxx10]
[xx011] ={1}=> [xx100]
[x0111] ={1}=> [x1000]
[01111] ={1}=> [10000]
[11111] ={2}=> [00000]

would lead to API:
s0 -{1}-> s1 -{1}-> s2 ... -{2}-> s31

in short: given N memory cells, we are able to express a CA with quadratic transitions
using a linear number of rules. TLDR we can't always avoid exponential blowup even in an 
intermediate representation, because what we may be representing may _truly_ be 
exponential.

## Return-value iterator
we observe that while we CAN have an explosion in CA, there will always be
a repetitive sequence somewhere inflating the count.
Instead, we allow strings of (otherwise identical) states to be represented in 
a special way that allows the atomic to ignore HOW MANY repeats there are, just
knowing that A will happen repeatedly until B happens finally (ending the loop).
```rust
let mut r = Rep::<X,Y>::new(x);
loop {
    match r.next() {
        Reps::More(a, next) => {
            r = next;
            work(a);
        },
        Reps::End(b) => break b,
    }
}

/// CAN BE WRAPPED TO BECOME NICE LIKE:
let mut r = Rep::<X,Y>::new(x);
let b = r.until(|a| {
	// do something with a?
});
```


if we can do both: explicitly enumerate states AND LOOP thingy
generate on the fly? sequence as we go?

-----------------------------------------------

# Goal:
given ? in {CA, RBA}, and a port-set, generate an API such that:
MUST HAVE:
1. Correct: never the case that the atomic will omit a potential port 
2. represent the _set_of_possibly_next-firing_ports_ at any moment

SHOULD HAVE:
1. lazy evaluation. don't generate a value to be PUT which gets discarded instead 
2. minimal unreachable branches
3. simple to use: complexity of API should not be exponential WRT the # {states in CA, rules in RBA}
4. simple to compile: time and space of API should not be exponential WRT # {states in CA, rules in RBA}

COULD HAVE:
1. allow the user to introduce redundancy to simplify their implementation:
	ie: {a}--0,1-->{a} covers {a}--0-->{b}--1-->{a}  
2. avoid computation at runtime that can be performed at compile time of either {API, Rust}

------------------
CA works nicely.
eg:
```
start: a
{a}--0-->{b,c}
 |    
  `--#-->{b,a}
{b}--1-->{c}
   `-0-->{b}
{c}--0-->{a}
```
becomes
```rust
fn atomic(mut a: State<A>, p0: Port<N0>, p1: Port<N1>) -> ! {
    loop {
    	let c = a.advance(|opts| match opts {
    		P0C(x) => p0.act(x),
    		P1C(x) => p1.act(x),
    		P0B(x) => {
    			let mut b = p0.act(x);
    			let c = loop {
    				b = b.advance(|opts| match opts {
	    				P0B => p0.act(x),
	    				P1C => break p1.act(x),
	    			});
    			};
    			c
    		},
    	});
    	// State<C> coerces into Coupon<N0, State<A>> because its the only variant
    	a = p0.act(c.into());
    }

    loop {
    	let x;
    	let maybe_value = None;
    	.advance(|opts| match opts {
    		? => maybe_value = Some(p0.get(x)),
    		? => p0.put(maybe_value.take().or(), x),
    	}
    }
}
```

-----
RBA does NOT work nicely. a small set of rules can produce a very large token automaton.
questions:
1. do we let it explode?
2. can we avoid explosion by introducing a small weakening?
	eg: a--0-->b--0-->c--0-->d--0-->e becomes a--0^N-->e


------------------------
# Techniques so far:
## CA-extreme:
1. types fully enumerate concrete states
2. user doesnt consider memory
3. perfect representation = no wasted branches + complete user control
4. exploding state space
5. very brittle IMPLS

## CA-extreme + REP transitions with arbitrary length
1. ?? may curtail explosion
2. ?? not sure how to generate
3. reduced control. number of iterations unknown
4. can represent lengths with 

## CA-extreme + REP transitions with runtime-known length
...
1. REPs can't cover cases where length is indeterminate
2. still explode at API compile time, but doesnt show up in APIs
3. user cannot statically reason about rep length

## CA-extreme + REP transitions with statically-known length
...
1. REPs can't cover cases where length is indeterminate
2. still explode at API compile time, but doesnt show up in APIs
3. very clunky length encoding


## RBA-extreme:
1. no concept of state.
2. always ready for any port firing => many wasted branches
3. runtime overhead for call-response
4. not brittle IMPLS


## Rule-encoding-types
1. allows weaken() operations to represent anything between CA and RBA
2. minimal compile-time jazz
3. !! no clear way to represent silent transitions !!
4. shows memory cells to user (ugly)
5. 


--------------------------

new idea: use the Token<A,B,C> representation. treat it like a TRS.
avoid silent transitions by preprocessing the ruleset ahead of time to get rid of them.

new problem: we need to ensure that the user covers only the rules that they need to.
solutions:
1. one enum def with a variant for every rule.
    
2. powerset of RULES. you are given a unique descrimenant for every rule set, with one branch per rule that tneds up being applied

3. powerset of PORTS, but with DEST states being represented as SETS.


----------
interesting idea:
Coupons can contain the current concrete state of the system, allowing you
to arbitrarily strengthen them again