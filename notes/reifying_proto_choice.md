Q: how does the exchange between some atomic and protocol object go down?

I am the proto:
I see: 
ready:     [00011]
tentative: [11000]

then its safe to do:
guard:     [10001]

the atomic needs to know:
1. which rule is invoked.

the protocol needs to know:
1. which ports are tentatively ready

```
A 						P
advance()
>>> multi_ready |= {1,2,3} >>> 
					ports {1,2,3} waiting "at 1"
...
					fire(rule=4)
					ready ^= {1,2}
<<<<< fired rule 4 (w/port 3) <<<<<
port4.put()
```

observe:
1. protocol does not need to know atomic grouping
	as far as the proto is concerned, it could change dynamically
2. protocol ONLY needs to send the atomic an index for the fired RULE
3. ??


alternative:
```
A 						P
register_group({1,2,3})
>>>> group_register {1,2,3} @ 1 >>>>>
						"ok. 1,2,3" all communicate using 1.
>>>> group_ready 1 >>>>>>>>>>>>>> (happens if 2+ possible)
					ports {1,2,3} are all ready
...
					fire(rule=4)
					ready ^= group1
<<<<< fired rule 4 (w/port 3) <<<<<
port4.put()
...

>>>>>>>> 2.put() >>>>>>>
					just 2 ready
					fire(rule=2)
					group1 not ready. No need to ^= or communicate back
```
1. atomics MUST register with the protocol
2. atomics just need to flag ready