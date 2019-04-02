## PUTTER
1. put(T) -> Result<(),T>
2. try_put(T, Option<Duration>) -> Result<(),(bool,T)>
3. try_grant_refusal() -> bool; // TODO ??? 
	// has no effect but to cause try_refuse() to succeed if its currently waiting.

## GETTER
1. get() -> Result<T,()>
2. peek() -> Result<&T,()>
TODO ?? try_get() ie: with timeout?
3. try_refuse() -> Result<bool,T>
4. try_accept_if(Fn(&T)->bool) -> Result<T,()> // DERIVED METHOD
5. try_peek(Option<Duration>) -> Result<&T,bool>

get() after peek() will always succeed immediately.
get() will always eventually succeed**
put() will always eventually succeed**
try_put() will succeed IF getter calls peek() OR get before timeout
try_put() will fail IF getter calls try_refuse()

// note: a successful PEEK or TRY PEEK _ensures_ the data will be _sent_
// note: 

** UNLESS peer is dropped!!

PROTO METHOD:
put() sets WRITABLE flag for peer
try_put() sets READABLE flag for peer
get() sets WRITABLE flag for peer
try_get() sets READABLE flag for peer
peek() sets WRITABLE flag for peer
try_peek() sets READABLE flag for peer.

IDEA: protocol knows that interacting with a port with WRITABLE flag will result in commitment.
	while interacting with ports with READABLE flag will not result in commitment. (peer can become committed)
	operations exist to RELEASE all the ones with READABLE flags.


protocol will wait until WRITABLE | READABLE <= READY {
	then it will call try_peek on all incoming to LOCK them into the transaction.
	if successful, it will call PUT on all outgoing to LOCK them into transaction.
	otherwise, will call try_put on outgoing which are guaranteed to succeed. 
}


// protocol will do its best to accept data from PUTS and OFFERS
// note that without timeout, peek() has no negative effect.
// 


we want to FAIL an offer() if NOBODY accepts it.
## CASE 1:
1. offer
2. get
3. refuse

this will result in the offer SUCCEEDING
## CASE 2:
1. offer
2. refuse
3. refuse


the fundamental problem here is that the protocol and its peers communicate using the same 
fundamental primitives, but requires a difference in authority with its peers:
1. the protocol needs to be able to space out PEEKS and GETS arbitrarily long
2. the protocol needs its downstream getter NOT to peek / get for arbitrarily long.


at its essence, the problem is that whether data flows is determined by its downstream getters
AND their choice to get requires access to upstream put-data.

a situation is unsatisfactory if there exists some composition of protocol components
that result in LIVELOCK or DEADLOCK.

eg:
	1. takes from 0, puts on 1 OR takes from 2 and puts on 3
	2. peeks on 1. may take from 1 and put on 4 on some condition.
scenatio:
	put 0, 


a protocol consists of 1+ synchronous regions.
it is possible to fragment such a protocol so as to separate these regions, making new protocols.
each protocol runs with 1 thread.


with only PUT GET and PEEK
data flows only in ONE direction. this is workable since even if data gets stuck
halfway through the protocol