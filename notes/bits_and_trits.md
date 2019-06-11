protocol does:
- all specified bits in G are CORRECT and READY

atomic does:
- given state s, and predicate p, does s satisfy p?

we observe that p and g are the same, and must have a ternary alphabet: T,F,X.
We need two bits to represent a trit.

INTUITION: instead of bits encoding what they DO have, they encode what they DO NOT have.

HAVING TRUE is encoded with  "10"
HAVING FALSE is encoded with "01"

# Protocol layout:
1. "memory" uses 2 bits per index:
01 -> F
10 -> T
(11 unused)

2. "ready" uses 2 bits per index:
00 -> ready
11 -> unready

IDEA: ready & mem.false == 11 (we have neither true nor false)


# Use
## Atomic state check

"memory" pointer is sent to atomic, "ready" is not.
the memory predicate can be precomputed!

eg: XXXF -> XXXT
can be verified with:
(memory & [00 00 00 01]) == 0
intuition: we use & with what we DON'T want to highlight contradictions. ==0 means none were found.

## Rule Guard check

guard   XXXTF has bits: 00 00 00 10 01
memory  TFTTF has bits: 10 01 10 10 01
ready   YNYYY has buts: 00 11 00 00 00

fails if (memory | ready) & guard != 0:
idea: 


# CONCLUSION:
proto needs:
	"memory" with encoding:
		T -> 10
		F -> 01
	"tentative" with encoding:
		Y -> 11
		N -> 00
	"ready" with encoding:
		Y -> 00
		N -> 11

proto-rule guards / predicates need:
	. with encoding:
		T -> 01
		F -> 10
		X -> 00


## Operations:

fn proto_rule_can_fire(memory, ready, guard):
	(ready | memory) & guard == 0

// idea:	`ready | memory` puts up bits for the bits it MISMATCHES.
//			`guard` is a field describing the configurations it does NOT want to see
				eg: specifying `00` means any state/readiness is fine"
					00 & (01|00) = 00 & 01 =
					00 & (10|00) = 00 & 10 =
					00 & (01|11) = 00 & 11 =
					00 & (10|11) = 00 & 11 =
					00
				eg: specifying `01` means "be true and ready" by saying "don't be false"

fn predicate_describes_state(memory, pred):
	memory & guard == 0


