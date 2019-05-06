# The problem
Two rough versions of the implementation exist:
1. threaded protocol
+ generic ports
+ more intuitively composable
+ more cache-friendly, maybe?

2. threadless protocol
+ no notifications & indirection
+ zero-copy
+ smaller codebase