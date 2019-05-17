
Simple idea:
1. getters announce their intention to move (or not) by marking a flag in their PoGe space.
2. the protocol traverses the PoGe spaces anyways to send messages. in this traversal,
	the protocol determines who moves (if anyone).
	the protocol tailors messages to getters


# Port-putter
the protocol traverses getters. it decides which getter will move (if any).

## CASE A: N>=1 cloners and 1 mover
getter-messages carry three bits of information:
	`(you_move:bool, other_role_exists:bool, rule_id:u62)`
	the first 2 flags are represented as `(1 << 63)` and `(1 << 62)` respectively.
	NEW ASSUMPTION: always `Rule_Id < (1 << 62)`

1. proto inits `putter.clone_countdown` to #cloners.
	proto prepares (but does not send) putter msg `1` ("someone_moving==true")
	proto sends `(0, 1, rule_id)` to each cloner
	proto sends `(1, 1, rule_id)` to the mover
2. getters that receive `(1, 0, rule_id)` know they are cloners
	they clone first and store the result
	then they perform `x = fetch_sub(1)` on `putter.clone_countdown` 
	if this cloner was last:
		(1 mover case!)
		this getter releases `putter.mover_sema`
3. the getter that receives `(1, 1, rule_id)` has been designated the mover,
	they acquire `putter.mover_sema`
	they wake up, perform the move
	they send the prepared message to the putter
4. the putter receives a prepared message of `1` and knows the datum has been moved

## CASE B: 0 cloners and 1 mover

1. <!-- proto inits `putter.clone_countdown` to #cloners. -->
	proto prepares (but does not send) putter msg `1` ("someone_moving==true")
	<!-- proto sends `(0, 1, rule_id)` to each cloner -->
	proto sends `(1, 0, rule_id)` to the mover
3. the getter that receives `(1, 0, rule_id)` has been designated the mover,
	they conclude that there are no cloners
	they perform the move
	they send the prepared message to the putter
4. the putter receives a prepared message of `1` and knows the datum has been moved
	putter returns

## CASE C: N>=1 cloners and 0 movers

1. proto inits `putter.clone_countdown` to #cloners.
	proto prepares (but does not send) putter msg `0` ("someone_moving==false")
	proto sends `(0, 0, rule_id)` to each cloner
2. getters that receive `(0, 0, rule_id)` know they are cloners
	they clone first and store the result
	then they perform `x = fetch_sub(1)` on `putter.clone_countdown` 
	if this cloner was last:
		(no movers case!)
		this getter sends the prepared message to the putter
4. the putter receives a prepared message of `0` and knows the datum has NOT been moved
	putter returns the datum or drops it. idc

# Mem-Putter
in this case, there is no putter waiting. 
Everything is the same, except there is no putter-msg-dropbox
the one that usually wakes the putter instead frees the memory. this involves:
1. dropping the memory if there was no mover
2. marking the memory as free