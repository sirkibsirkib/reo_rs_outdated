Question:
given a putter P, and a set of getters G:

we know ahead of time the size of G.
every one in G can decide YES or NO they want to MOVE.

each getter must MOVE or CLONE
it's not safe to CLONE once someone has MOVED

anyone is able to CLONE, but MOVE is preferable

Solve:
1. P must know if ANY chose MOVE
2. P must know when everyone is done cloning and moving


--------------------------------- SOLUTION
Assume:
1. proto knows the number of getters, N
2. proto has < 2^31 ports per rule
	(2^32 is how much we bump up)
0 - - - - - - Nmax - - - - - 2^32


```rust
MoveGuard(AtomicUsize);
```
Process:
1. N getters call get() and fall asleep on their Dropbox
1. one putter calls put() and falls asleep on their Dropbox
1. one of these ports becomes the proto, and sees everyone is ready.
1. proto initializes the MoveGuard of the putter to value N
1. proto sends putter identity to every getter
1. every getter wakes up and finds the putter they want to get from in their Dropbox
1. every getter calls check() on the MoveGuard of the putter
-- if they wish to move: fetch_add(2^32 - 1)
-- if they dont wish to: fetch_sub(1)
1. every getter checks the returned result 'r' to determine two things:
-- they were the last if r % (2^32) == 1
-- someone had requested move before them if r > 2^31
1. the getter that determines that they are last notifies the putter
1. at most 1 getter exists for which:
-- no previous getter wanted to move
-- they themselves want to move
-- ... this getter waits for the putter
1. putter wakes up and checks if anyone performed move (r > 0). if so, they send 