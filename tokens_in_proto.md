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
1. {a}={1}=>{b}
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

enum OptsS0 {
	R
}