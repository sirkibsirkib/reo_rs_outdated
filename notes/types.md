in the event of a replicator. what do we do?

Properties:
* [NEVER READ]: never inspected for contents
* [NEVER MODI]: never written to
* [NEVER USED]: NEVER MODI & NEVER READ
* [CIRCUIT ONLY]: never enters a component (type can be changed)
* [SMALL]: shallow representation smaller than some predefined N bytes
* [COPY-SAFE]: using two shallow copies does not incur data races
* [CLONE]: supports an operation for performing a deep copy, creating a new indep object 
* [API CHANGABLE]: either CIRCUIT ONLY or language is Garbage-collected  ?? (TODO)

================
a) is the object [NEVER USED] and [API CHANGABLE]?
	> collapse type to Unit
b) is the object [NEVER USED] and [CIRCUIT ONLY]?
	> collapse type to Unit
c) [NEVER READ]?
	> uninitialized memory (investigate Unit->uninitlized conversion)
d) is type [SMALL] and [COPY-SAFE]?
	> (shallow) memcpy
e) is type [NEVER MODI] and [API CHANGABLE]?
	> translate to Arc<T>
f) is the type [CLONE]?
	> make a clone (deep-copy)
g) panic


Limitations:
(a,b,d) inhibited by black-box components
(d) requires API change for non-GC languages (may not be possible)

////////////////////
1. must handle any case
2. try make it transparent T->T
3. in cases where its not possible: HERE IS A CONFLICT
	* user must explicitly fix the thing


////////////////////////////////
We define PROPERTIES. They may be in three states:
	HAS,
	HASNT,
	UNKNOWN

1. Linear
	HAS if type is not allowed to be dropped in-circuit, otherwise HASNT

1. Clonable
	HAS if circuit is replicated. flows up+down

1. Read
	HAS if input type for blackbox component and is accessed. 
	flows upstream	

1. Written
	HAS if input type for blackbox component and is written to.
	flows upstream	

1. FixedTypeSig
	HAS if input or output type for blackbox component
	flows upstream

1. SmallShallow
	HAS only if type is cheap to MOVE

--------

Steps:
1. if !(Read) && !(Write) && !(FixedTypeSig) && !(Linear):
	change type to UNIT and plug in a transformation step

1. if !(Read) && !(Linear):
	leave uninit memory and pass bogus data
1. 


