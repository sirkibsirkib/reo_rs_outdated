The problem is such:
1. reo protocols are stateful in terms of the values present at their
	Locations.
-- memcells are the most obvious, where some memcell of type T has either None
	or any value in Some(T).
-- ports 


1. the protocol is a constraint relation over which values can be synchronously
	observed at any timestep. port-values reason about SUCCEEDING puts / gets.

state persists over time only in memory cells, thus any persistent state in reo
is expressed entirely using the contents of these memory cells.



There is a funamental question that polarizes the possible solution space 
for the implementation of Reo protocols further:
> Do we precisely precompute the possible transitions in every state?


we use this example to illustrate the differences:
r0: p => m0
r1: m0 => m1
r2: m1 => g

if YES, we head down the road of constraint automata. We focus on which transitions
are available next and the contents of memory cells disappear in the names of states.
every state describes precisely what is in all memory cells. 
nice: every state remembers _precisely_ which transitions can be next.


if NO, we head down the road of rule-based form. We split up the representation of
state into one variable per memory cell and _re-discover it_ continuously. 
nice: the logical transitions map 1-to-1 to our concrete transitions, and 
	checking state is LINEAR with the number of memory cells.


lets assume the RBA approach for now. It has the fundamental shortcoming:
in every state, we do not (statically) know which transitions are possible next.

the fundamental challenge of Reo (in this context) is that Reo protocols facilitate
the definition of rules that are constrained by an arbitrary predicate over the entire
protocol state. For an arbitrary protocol, there is no meaningful way to 
implicitly know whether a rule is ready to fire.

the RBA representation does not overcome this problem in general, but rather
leverages the assumption that _the rules are sparse_ in which states they reason about.
Otherwise phrased: observe that the RBA is no better than the CA if no rule is
appliccable in more than 1 reachable state. (why? because RBA gains advantage
in collapsing these repretitions. if there is nothing to collapse, there's no benefit)

let's continue the assumption of the RBA. we have the issue that you have to check
EVERYTHING EVERY TIME. This is not completely horrible, as we assume that 
there aren't that many rules to begin with. 

also note that the CA's ability to precompute certain branches is ALSO limited
because predicates can represent values in terms of runtime data and each other
(ie. rule 0 requires that put1==put2) 


