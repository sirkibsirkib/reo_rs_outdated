let getters = count GET-ports and release GET_SIGNAL ports
if 0 getters {
	cleanup memcell / release putter	
} else {
	send (TRUE, mem)
}


enum GetCase {
	AnotherMover,
	MoverIsMe,
	NoMover

}


getter {
	if COPY:
		let rule_id = RECV():
		let datum = copy()
		if last, cleanup memcell
	else:	
		let (case, rule_id): (GetCase, RuleId) = RECV();
		match case {
			GetCase::AnotherMover => {
				let datum = clone();
				let am_last = fetch_sub(1);
				if last: notify mover
			},
			GetCase::MoverIsMe => {
				wait for notify
				release putter / cleanup memcell
			},
			GetCase::NoMover => {
				let datum = clone();
				let am_last = fetch_sub(1);
				if last: release putter / cleanup memcell
			},
		}
	}
}
