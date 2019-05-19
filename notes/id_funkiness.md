every port ID has a readiness that comes in three states, but the interpretations 
differ based on what type of port it is:
1. Putter-Ports:
	* Not Ready
	* Ready to Put
	* Tentatively Ready Group-Leader
2. Getter-Ports
	* Not Ready
	* Ready to Get
	* Tentatively Ready Group-Leader
3. Memory Cells
	* Not Ready
	* Ready to Put
	* Ready to Get

for both, we require two bits, and each has an invalid fourth state:
1. Putter/Getter-Ports:
	(ready, tentative)
	01 is invalid (tentative implies ready)
2. Memory Cells
	(putter-ready, getter-ready)
	11 is invalid (types of readiness have mutually-exclusive requirements)

Dispite the similarities between port- and memory-bit representations, they don't
lend themselves well to having the same layout in memory, as their bits are checked 
in different circumstances:
1. rule-guards are often compared to the ready-set for difference. 
	tentative-readiness is then compared to the same guards. as such, it is 
	pratical for guards, port-readiness, and port-tentative-readiness to 
	be represented as _separate_ bitsets for easy difference operations.
	eg:
		guard:  0001 0100 0000
		ready:	0101 0100 0111
		tenta:  0000 0100 0010
	"rule is ready to commit. id=5 must be made non-tenative"

2. memory-readiness of GETTER and PUTTER components are expressed separately.
	Guards must thus express the requirement of a memory-cell as a _ternary_ 
	variable, requiring two bits in the guard itself (or two guards with one bit each)

TL;DR the bulk of the work of these bits is to faciliate the:
	`if ready.is_superset(rule.guard) {...}` check. As such, we design the layout
	around this: things that DO need to be expressed separately go into the guard.
	(namely, memoryPUTTER and memoryGETTER), but things that do not need to be distinguished
	by the guard are represented as one bit (port-tentativeness doesn't need to be represented).

As a result we have bitsets:
	ready, tentatively_ready, guard
	all of which have separate bits for both memory cell bits.


## Final representations:
### External
this is what the Reo compiler will generate (closer to concept)

0 <-- id space --> 
[port-putters][port-getters][memory-ids]

### Internal
this is what the proto structure will actually use internally

#### SpaceStructs

`
0 <-- id space --> 
[port-putters][port-getters][memory-ids]
 | | | | | | | | | | | | | | | | | | | |
 | | | | | | | | | | | | | | | | | | | |
[port-putters][port-getters][memory-ids]
`

#### ReadySet

`
0 <-- id space --> 
[port-putters][port-getters][memory-ids]
 | | | | | | | | | | | | | | |\|\|\|\|\|
 | | | | | | | | | | | | | | | | | | | |
 | | | | | | | | | | | | | | | |\|\|\|\|\
 | | | | | | | | | | | | | | | | | | | | \
 | | | | | | | | | | | | | | | | |\|\|\|\ \
 | | | | | | | | | | | | | | | | | | | | \ \
 | | | | | | | | | | | | | | | | | |\|\|\ \ \
 | | | | | | | | | | | | | | | | | | | | \ \ \
 | | | | | | | | | | | | | | | | | | |\|\ \ \ \
 | | | | | | | | | | | | | | | | | | | | \ \ \ \
 | | | | | | | | | | | | | | | | | | | |\ \ \ \ \
 | | | | | | | | | | | | | | | | | | | | \ \ \ \ \ 
[port-putters][port-getters][memory-put][memory-get]
`

## Another trade-off:
how to do split the work between the Reo compiler and the reo lib?
EG: Reo compiler does more work:
+ better runtime performance
+ reo lib is more flexible
- more unsafe functions in reo lib's API
- reo compiler is more complex and obtuse

what does the compiler have to PROBABLY do anyway?
1. make sure IDs are consecutive eg: {1,6,8} => [0,1,2]

what would be the best for safety?
1. reo generates something like:
	rule!{
		0 => 1,2,3,4;
		2 => 2,4;
	}