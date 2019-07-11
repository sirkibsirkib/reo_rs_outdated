# Questions: what kind of formulas can we get? How do we even interpret them?
1. f(a=b)=c where {a,b,c} are ports.
	do a and b fire here or not?

1. a=* where a is a port. so a does not fire?

2. a!=b & c=d.    I guess this rule can happen WITHOUT firing a and b? 
					Same true for a=b? (a:= silent, b:= silent)?

# We can extract OR operations right? Pretty much anywhere
(a=b | c=d) becomes two rules:
1. a=b
2. c=d

f(g(a=b | c=d))
1. f(g(a=b))
2. f(g(c=d))

# We can delete rules that necessitate contradictions
a=b & a=* & b!=*

# We can simplify the rules using typical operations like using DeMorgan's
!(a=b & c=d)
->
a!=b | c!=d 