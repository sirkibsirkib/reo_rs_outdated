1. the protocol is constructed as a stateful object `ProtoAll`,
	but "port" (x in {`Putter`, `Getter`}) objects are returned.
2. Port objects share the `ProtoAll` through a shared `Arc`.
3. The `ProtoAll` is composed of a "immutable" component `r`, and a mutex-protected
	mutable component `w`.
4. `r` is not strictly immutable, but relies on more esoteric ordering etc. 
	properties to be safe (discussed later).
5. every port has a unique `PortId` integer.
6. the `ProtoAll` owns a u8 buffer which stores the actual data that backs memory cells 