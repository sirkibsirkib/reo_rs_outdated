
# Open problems:
1. how does the stitcher work?
2. when does this safe atomic API get generated?
3. When the atomic API gets generated, where does it get the protocol automaton?
-- part of the protocol trait?
4. how do we handle early-generated atomics?
5. concrete and generic type arguments on treo code
6. how do we represent atomic APIS with arbitrary ports compiled EARLY?
-- eg: max(a?, b?, c!)
7. how does the protocol get represented anyway?
-- first thought: make_proto() returns just a bunch of ports (related by proto)

