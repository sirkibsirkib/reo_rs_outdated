pub struct BitSet {
	data: Vec<usize>,
}
impl BitSet {
	const BYTES_PER_CHUNK: usize = std::mem::size_of::<usize>();
	const BITS_PER_CHUNK: usize = Self::BYTES_PER_CHUNK * 8;

	pub fn new(min_capacity: usize) -> Self {
		Self {
			data: vec![],
		}
	}
	pub fn capacity(&self) -> usize {
		self.data.capacity() * Self::BITS_PER_CHUNK
	}
	pub fn set(&mut self, idx: usize) {
		let mask = idx % Self::BITS_PER_CHUNK;
		let chunk_idx = idx / Self::BITS_PER_CHUNK;
		while self.data.len() <= chunk_idx {
			self.data.push(0);
		}
		self.data[chunk_idx] |= mask;
	}
	pub fn is_subset(&self, other: &Self) -> bool {
		if self.data.len() < other.data.len() {
			return false; // INVARIANT: NO TRAILING ZERO CHUNKS
		}
		for (&s, &o) in self.data.iter().zip(other.data.iter()) {
			let either = s|o;
			let o_not_s = either & !s;
			if o_not_s != 0 {
				return false
			}
		}
		true
	}
	pub fn try_difference_with(&mut self, other: &Self) {
		for (s, &o) in self.data.iter_mut().zip(other.data.iter()) {
			*s &= o
		}
		// restore invariant
		while let Some(x) = self.data.pop() {
			if x != 0 {
				// whoops! wasn't zero
				self.data.push(x);
				return;
			}
		}
	}
}