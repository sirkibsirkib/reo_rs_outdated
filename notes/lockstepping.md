#Problem

we want atomics to walk in lockstep of the protocol. the problem is that the 
protocol is using a different RBPA to us, and so cannot know what we call our rules.

eg rules walking in lockstep
PROTO: =0=> =2=> =0=> =1=>
ATOMI: =0=> .... =3=> =1=>
in this example, atomic has rule "3" representing proto (2,0).

The issue is that the protocol would not know how to send us a message that
distinguishes [0] from [2,0]. It's all 0 to it. It's infeasible to keep a potentially
infinite list eh?

One thing to note is that this is NOT a problem for deciding which port we must fire.
after all, ATOMIC rules "0" and "3" both obviously involve the same port. 
_however_, it is a problem in helping the atomic to keep track of the protocol state
such that it knows what comes _later_. "3" and "0" might have different implications
on the protocol's state.

# failed solution A: prime decomposition.
Idea is that instead of communicating the current rule_id, the protocol communicates
X, which is the sum of all associated primes for rules applied so far.

eg, assume the mapping from rules to primes is {0=>2, 1=>3, 2=>5}
consider the same instance as before
       +2   +5   +2
PROTO: =0=> =2=> =0=>
                  |
                  proto sends msg: "9" (2+5+2)
ATOMI can figure out that since the last 0-message, X has increased by 7,
and thus it can distinguish "0" from "3".

##BUT, this doesn't always work, obviously
We cannot, for instance, distinguish (=1=> =2=> =0=>) from (=2=> =1=> =0=>).
in the event 1 and 2 are both silent to the atomic, it will have two variants here,
each with the value 3+5+2 == 5+3+2 == 10. 

# Solution B: 