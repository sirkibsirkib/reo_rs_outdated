
# STATE OF AFFAIRS:
1. the proto-threadless version can achieve zero-copying.
2. we can easily implement something that HOLDS UP the putter with a PEEK operation
3. putter and getter can have totally different types (getter/putter themselves don't enforce)
	the compiler would only generate protocols that have compatible types
	* explore the idea of _non-destructive conversions_
4. current small downside: getters leave the barrier early I guess. is this even an issue? (could we even do better?)

# SMALL TODO:
1. what happens when a putter / getter is DROPPED? 
	* eg: some putter may be blocking
