API will generate struct


AtomicAb<P,T> where P: Proto<T> {
	type Interface = (Putter<T,P>, Getter<T,P>),
	type SafeInterface = (Safe<Putter<T,P>>, Safe<Getter<T,P>>)
	type 

	fn build(Self::Interface, ) -> 
}