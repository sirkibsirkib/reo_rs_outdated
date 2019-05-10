
## The Java
PRO:
1. put/get returns ASAP

CON:
1. every rule must be checked every time
2. no parallelism possible
3. put / get / check all contend for the same single lock


## With BitSets
PRO:
1. when no ports arrive, no work is done
2. 

CON:
1. memory work can delay a putter / getter from returning
	eg: {A=>M}, {M=>M}
		A.put() never returns, the thread is hijacked.


## With ByteVecs
PRO:
1. guards are often determined to be false outside the CR
2. parallelism possible

CON:
1. rule bias. longer rules may starve
2. constant-factor increase in subset checks and locking time


## BitSet + ProtoThread
1. put / get kick the proto thread

## Lazy portthreads
1. every enter() is parameterized by a goal. work is performed until the goal is met.
	rules that would satisfy the goal are biased?
	eg:
		a->m
		m->m
		m->b
		a.put() exits after first rule
		b.get() blocks until m->b