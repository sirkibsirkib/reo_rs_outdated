import sys


class Gcmd:
	def __init__(this, name, guard, ports, actset):
		this.name = name
		this.guard = guard
		this.ports = ports
		this.actset = actset
	def __repr__(this):
		return "`{}`  {} ==[{}]==> {}".format(this.name, this.guard, this.ports, this.actset)
		
def sat(state, guard):
	for k,v in guard.items():
		if state[k] != v:
			return False
	return True
	
def apply(state, actset):
	for k,v in actset.items():
		state[k] = v
		

states = {(False, False, False)}
opts = {}
states_todo = states.copy()

while len(states_todo) > 0:
	n = states_todo.pop()
	print("Processing {}".format(n))
