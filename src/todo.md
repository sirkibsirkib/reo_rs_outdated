1. overhaul bitsets in `reo_rs`. We simply want two bitsets:
	ready: ports + mems
	state: mems
	tentative: ports

2. encode assignments into proto Rule objects. have the proto itself 
	flip the state bits THE MOMENT IT COMMITS TO A RULE

3. figure out the entrypoint to a generated API

4. figure out the interaction between generated API and `reo_rs` per transition
