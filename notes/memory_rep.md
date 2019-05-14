all protocols have some things in common. Namely, they share the concept
of Putter, Getter, and so on. However, some details of the protocol are more specific,
namely ACTIONS associated with rules, how data moves between stuff.


# Kinds of Actions

1. Mem-stay
	conditions:
		p is mem
		g contains p
	data is ONLY cloned, and retained at the call-site

2. Mem-swap
	conditions
		p is mem
		g contains some q where q is mem
	box-swap between M1 and M2

3. move
	conditions
		(none)
