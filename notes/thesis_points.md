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
