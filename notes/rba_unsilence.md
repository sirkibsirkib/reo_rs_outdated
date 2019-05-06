eg:

-- RULES:
R1: x0 -1-> 10
R2: 10 -2-> 01
R3: 1x -.-> 00

remove R3.

Q: which transitions can IMMEDIATELY precede R3? {R1}?
Add pair-transitions for every such transition (even possibly itself).

we need to introduce a rule that is [R1,R3]:
x0 -1-> 10 -.-> 00
R1,3: x0 -1-> 00.

Once every such pair-transition has been added, remove the original.
we observe that applying a rule twice is ALWAYS the same as applying it once.

-- RULES:
R1  : x0 -1-> 10
R2  : 10 -2-> 01
R1,3: x0 -1-> 00
DONE

---------------- EG2
RULES:
R1: xx0 -.-> xx1
R2: x01 -.-> x10
R3: 011 -.-> 100
R4: 111 -1-> 000

REMOVE R1:
R2,1: x01 -.-> x10 -.-> x11
R3,1: 011 -.-> 100 -.-> 101
R4,1: 111 -1-> 000 -.-> 001



RULES:
R2  : x01 -.-> x10
R3  : 011 -.-> 100
R4  : 111 -1-> 000
R2,1: x01 -.-> x11
R3,1: 011 -.-> 101
R4,1: 111 -1-> 001

Remove R2. Can precede: {(R3,1), (R4,1)}:
R3,1,2: 011 -.-> 101 -.-> 110
R4,1,2: 111 -1-> 001 -.-> 010

RULES:
R3    : 011 -.-> 100
R4    : 111 -1-> 000
R2,1  : x01 -.-> x11
R3,1  : 011 -.-> 101
R4,1  : 111 -1-> 001
R3,1,2: 011 -.-> 110
R4,1,2: 111 -1-> 010

Remove R3. Can precede: {(R2,1)}
R2,1,3: 001 -.-> 011 -.-> 100

RULES:
R4    : 111 -1-> 000
R2,1  : x01 -.-> x11
R3,1  : 011 -.-> 101
R4,1  : 111 -1-> 001
R3,1,2: 011 -.-> 110
R4,1,2: 111 -1-> 010
R2,1,3: 001 -.-> 100