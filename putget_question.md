# PUTTER
1. put
	// guaranteed to put
2. try put
	// allowed to express a TIMEOUT
	// will FAIL if either:
		1. timed out before it was inspected
		2. didn't time out. peer called try_refuse() instead of get()


# GETTER 
1. get
2. try get
2. try refuse
3. timeout get
	will fail if the peer cannot react in time