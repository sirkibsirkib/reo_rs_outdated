investigate type NonNull

Since we can't specialize, instead maybe we can pass around clone and drop 
pointers which are NONE if the type doesnt support this operation
(detected and causes panic at runtime)


we want:
1. threadless
2. crossbeam message channels as barriers
3. pointer passing
4. variable-size state