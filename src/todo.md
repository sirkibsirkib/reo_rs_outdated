1. overhaul bitsets in `reo_rs`. We simply want two bitsets:
	ready: ports + mems
	state: mems
	tentative: ports

2. encode assignments into proto Rule objects. have the proto itself 
	flip the state bits THE MOMENT IT COMMITS TO A RULE

3. generated API once-off code
-- grouping / ungrouping jazz
-- Interface type
-- figure out what is generic enough to end up in `reo_rs`


4. generated API <==> reo_rs code each transition code
-- how do we evaluate which variant is matched?
	msg: (port_id, &MemBits)
-- what is the control flow? WHEN exactly does this variant get chosen? pass a callback? 
