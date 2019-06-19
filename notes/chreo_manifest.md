focus: clear separation between coordination and computation.
coordination:
* has the (static) concept of composed components
* Reo concerns itself ONLY with the definition of a protocol's interface,
	its state and its transition system (behavior)
* each protocol instance is fundamentally static, (compiled once and then imported into an application)
* defined and constructed out of composed reo components, each of which is white-box

computation:
* has the (dynamic) concept of _grouping_, which is not compositional. ports can be grouped.
* the concrete implementations of computation code only exists in the main