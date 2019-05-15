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
	move into one location, clone into the others


## WHAT WE KNOW
1. [ASSUME] putters and getters are NEVER created any way other than instantiate()
--> unique port-ids
--> 
2. one thread per cr at a time
--> one thread traversing rules per time


complications:
1. cloning port-data requires knowing the concrete type
--> this means firing an action cannot be entirely inside reo-lib-land, can it?

ideally we want:
1. minimal unsafe type-wrangling
2. minimal code duplication
3. 


# what all proto structures have in common:
1. mems, putters, getters, mems^
2. ready set
3. having rules

# where all protos differ
1. what the rules are
2. 