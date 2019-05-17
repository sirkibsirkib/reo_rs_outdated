what we know:
1. it is possible to write to a u8 without affecting neighbouring u8s (lock-free)
2. each rule statically knows how many getters to expect
3. every rule has exactly 1 putter
4. the time taken to complete a get + put should be SHORT
5. small-constant time operations are SHORT
6. an operation can be small-constant time if its lock-free
7. its OK to spinlock when the wait time is SHORT
8. every rule involves 1+ {putter, getter}s
9. a previously blocked rule cannot become unblocked until 1+ {putter,getter}s in the guard become ready
10. at the start of put / get, I know my ready-byte is FALSE
11. a putter cannot write his datum until he knows nobody is reading the prior one
12. its usually not safe to write a byte PARTIALLY if there is a data race.


--------------------
protocol:
	[byte for _ in (putters + getters)]
	[(ptr, waiting) for _ in putters]

waiting:
	

ignore cache:
the ReadyBytes has an element for every PortId which can be effectively seen 
as two bits
[ready, not_blocked]

put_or_get():
	spin while 
	enter()
	spin while ready[id] == [-,0];
	if ready[id] == []

enter():
	for rule in rules:
		for (r,g) in zip!(ready, rule.guard):
			if g == [1,1] && r != [1,1]:
				continue rules
		// rule is ready!
		ready[rule.putter] = [0,0]
		for getter in rule.getters:
			ready[getter] = [0,1]
			tell getter to get from rule.putter

