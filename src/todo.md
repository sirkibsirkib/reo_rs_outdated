1. consider the 'static protocol description.
	the idea:	we can now use a constant to describe the protocol's SHAPE
		(shape has all protocol information except type reification)
-- see how it would change Proto trait
-- investigate Serde. Need it ONLY for the API jazz

2. finish up the crossover of api and reo_rs stuff:
-- a state should receive a determine() function that works as expected.
-- what does the API function need? (State<..>, Interface, ??) &mut PortGroup?
maybe something more opaque. "Determiner?" yeah sounds good.

3. eventually switch over from importing protocols into API thingy into reading them from
a file given as arg. Maybe bincode? maybe RON? maybe support both idk.

4. update the reo_rs interface according to whatever happens with static protocol defs

5. copy out the Reo template jazz I did already. Throw away Reo. re-clone the repo

6. do some more writing