
# The design

## Goal
A protocol object that is hidden behind a set of Putter and Getter objects,
which have put(T) and get()->T operations respectively.
Reo protocols define how data is allowed to flow between these ports ("synchronously").
a protocol can be understood as a set of Rules, where each rule can be represented as:

{P =C=> {G}} 
where:
	P: one 'putter-like' port (Putter, for now)
	G: a set of 'getter-like' ports. (Getters, for now).
	C: some conditional predicate on the 

for example, an exclusive router might be specified as:
A =

## Proto Data Structure

define an ID-space. 
[putters][mems][getters]
^^^^^^^^^^^^^^1
^^^^^^^^^^^^^^^^^^^^^^^^2

1:these guys also each get a PutterSpace structure.
2:these guys get a ready-byte each AND a MsgDropbox structure
```rust
struct PutterSpace {
	ptr: Ptr,
	ptr_owned: AtomicBool,
	sema: Semaphore,
}
```

```rust
struct MsgDropbox {
	sema: Semaphore,
	message: UnsafeCell<usize>,
}


```

## Ptr: keeping track of data

when type T is larger than 8 bytes, Ptr of T is _indirect_
```
             stack
             |   |
  [.|.|.]----+-T |
  /  |       |   |
 /   T        ````
T
```

when T is no larger than 8 bytes, Ptr of T is _direct_
```
[T|T|T]
```

Ptr is not typed. it simply represents 8 bytes, and is transmuted by
the caller with some T type as needed.
the generated code must be designed carefully to avoid screwing it all up.
This means proto objects are heterogenous.

## Concurrency & Parallelism
every id is associated with a u8-size buffer slot.
conceptually, this u8 is a _status_ byte. 
let us consider just one status bit: 0b1'Ready'. we can check for its
presence or absence easily.

the "status" vector might at some point be [1,1,1,0,0]

putters / getters WRITE 0b1 into their byte when they invoke get(), put(),
without locking, obliterating whatever was there before.
as each port is controlled by 1 thread, no 0b1 => 0b1 obliteration is possible.
after arriving, each port needs to only check whether R rules are now satisfied,
where R is the set of rules requiring that they, themselves are ready.

Putters also have the job of actually TRACKING their newly put data. This is
performed by updating their Ptr to point to their stack, and setting ptr_is_owned
to TRUE.

so the sequence is:
1. set myself to "ready"
2. traverse rules and check if any are satisfied.

observe, at some point, the last port required for a rule to be satisfied 
becomes ready. at least that port will check this rule. However, it is
possible that 2+ threads both conclude some rule R is ready. How do we decide
who performs the firing (ideally without communication)?

observe that every rule involves exactly 1 putter. also observe that 
if a rule is satisfied, it must be that this putter is in state 'ready'.
we are able to use the putter's existing status-byte as a critical region.

if status_vec[putter].swap(0b0) == 0b1 {
	// I am allowed to fire the rule! nobody else can!
}

so what does it mean to FIRE a rule, anyway?
we ultimately need to move data from putter.Ptr to every getter. Observe that
(in the current case where all putters are port-putters) the putter _must_ return
_after_ all getters have finished, for as soon as they return, the destination
of the putter-Ptr is invalid memory!

When firing the rule, the thread communicates information that can be understood as:
1. for every getter, please go fetch the data from _this_ putter
2. the putter must wait until _this many_ getters have finished getting.

here, we leverage the `MsgDropbox` structure. its a lot like a bounded channel with
capacity 1, where instead of FULLNESS (which blocks a 2nd write), writes never block
and we just have to be careful not to perform 2 in a row (we will see later that 
this is OK).

The "protocol thread" sends the message of the putter ID to every getter,
and sends to the putter, the _number_ of getters. 
getters wake up (the semaphore is used to block readers of an empty dropbox),
and the putter wakes up. with everyone ready for action, they all get to work:
1. putter waits for N semaphore signals in their PutterSpace structure
2. getters GET the data from the space of the putter, and then send a semaphore signal.

aside: moving.
rust has this pesky notion of ownership. we need to make sure our data-objects are
DROPPED correctly. 