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