1. threadless protocol doesnt need event loop
2. bitset for readiness
3. Tryput and timing out. "tentative readiness"
4. passive vs active port interactions
5. CA token API. states are tokens. coupons and receipts
6. tokens compile away
7. Decimal number encoding
8. tagging tokens with an extra type to prevent atomics sharing tokens
9. pointer passing from the putters' stacks
10. "acting as the protocol" in threadless mode
11. who drops the data? handling `Panic` etc.
12. rust memory model: Arc, Mutex, Ref, MutRef, etc.
13. stuffed pointers
14. RBA vs CA
15. java passes on the heap. rust analog is Box<T>
16. reo affine types. distinguish clone vs not clone
17. relevant types (added)
18. relevant + affine = linear
19. heterogenous circuit types
20. dynamic conversion to signal type (ie. have `get` and `get_signal` operations available)
21. runtime protocol composition
22. blocking atomic port inadvertently blocks other ports
23. tryput branching
24. token api with nondeterminism. "ask the protocol"
25. token API: eager vs lazy putting
26. RBA API generation: state is now a tuple.
27. dynamic reconfig: sharded rwlock at outermost level can alter the proto
28. RBA api: do we fragment rules or nah?
29. what happens if we DONT fragment rules: we get overly conservative APIs
30. ways of representing group_ready sets
-- a group becoming ready sets N bits. checking readiness involves ONE ready set
-- a group becoming ready sets 1 leader-bit. checking readiness involves N ready sets
-- a group becoming ready sets 1 leader-bit. checking readiness involved ONE ready set BUT guards get changed
31. changing guard ready bits. using a hashmap to remap Ready_bits {g in group} => leader
32. who gets permission to MOVE? at most 1, but which getters are able isnt known
33. byte-vector for ready sets. can set byte to 0xff in parallel. readying gets 8x slower.
34. putter-readiness doubling as a lock
34. using transmute to arbitrarily swap between u8 and AtomicU8 as needed
34. using channels for communication. can do both directions. eg: getters READ from rcvrs[putter_id]
35. PROs n CONs of protocol thread(s)
	PRO: mem=>{} rules don't hold up any port put() / get() operations
	CON: how do you choose the number?
36. thread parking is efficient AF. A park costs around 220ns. unpark() does NOT block.
37. Mem => Mem move can be achieved by swapping box pointers. caveat: move must occur AFTER all clones.
38. EACH RULE MAY DEFINE MULTIPLE DATA-MOVES! can't just lock the "putter", because there may be more.
39. define a locking quorum. chosen such that if everyone locks {your_putters} & {quorum}, there is sufficient mutex
40. is it possible to lock a port NOT in your put set without disaster?
41. we know that there is NO overlap between ports in different ACTIONS of a RULE
----------------------------
WE IGNORE
1. synchronous region decomposition
2. opportunities to traverse bitsets in parallel (opt for compact bitsets instead)


WE ASSUME
1. we want to support large data
2. we don't want to bound the #port IDS
3. CLONE does not alter the contents of the shallowest bytewise representation
-- does NOT apply to highly experimental PIN types. eg for coroutines
4. 

WE SUPPORT
1. passing values on the stack
2. putter->getter stack passing
3. memory->memory values are passed by reference
4. reference-counted indirect memory passing
5. values are allowed to be dropped in the circuit