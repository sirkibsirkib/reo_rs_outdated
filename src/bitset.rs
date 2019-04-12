use std::fmt;

#[derive(Default)]
pub struct BitSet {
    data: Vec<usize>,
}

impl fmt::Debug for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "bitset: [")?;
        for b in self.data.iter().rev().take(1) {
            write!(f, "{:b}", b)?;
        }
        for b in self.data.iter().rev().skip(1) {
            write!(f, ".{:b}", b)?;
        }
        write!(f, "]")
    }
}
impl BitSet {
    const BYTES_PER_CHUNK: usize = std::mem::size_of::<usize>();
    const BITS_PER_CHUNK: usize = Self::BYTES_PER_CHUNK * 8;

    pub fn with_capacity(min_capacity: usize) -> Self {
        let chunks = if min_capacity.is_power_of_two() {
            min_capacity
        } else  {
            min_capacity + 1
        } / 64;
        Self { data: std::iter::repeat(0).take(chunks).collect() }
    }
    pub fn capacity(&self) -> usize {
        self.data.capacity() * Self::BITS_PER_CHUNK
    }
    pub fn set(&mut self, mut idx: usize) {
        idx += 1;
        let mask = idx % Self::BITS_PER_CHUNK;
        let chunk_idx = idx / Self::BITS_PER_CHUNK;
        while self.data.len() <= chunk_idx {
            self.data.push(0);
        }
        self.data[chunk_idx] |= mask;
    }
    pub fn test(&self, mut idx: usize) -> bool {
        idx += 1;
        let mask = idx % Self::BITS_PER_CHUNK;
        let chunk_idx = idx / Self::BITS_PER_CHUNK;
        match self.data.get(chunk_idx) {
            Some(chunk) => chunk & mask != 0,
            None => false,
        }
    }
    pub fn is_superset(&self, other: &Self) -> bool {
        if self.data.len() < other.data.len() {
            return false; // INVARIANT: NO TRAILING ZERO CHUNKS
        }
        for (&s, &o) in self.data.iter().zip(other.data.iter()) {
            let either = s | o;
            let o_not_s = either & !s;
            if o_not_s != 0 {
                return false;
            }
        }
        true
    }
    pub fn difference_with(&mut self, other: &Self) {
        for (s, &o) in self.data.iter_mut().zip(other.data.iter()) {
            *s &= !o
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

#[macro_export]
macro_rules! bitset {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(bitset!(@single $rest)),*]));

    ($($value:expr,)+) => { bitset!($($value),+) };
    ($($value:expr),*) => {
        {
            let _countcap = bitset!(@count $($value),*);
            let mut _the_bitset = BitSet::with_capacity(_countcap);
            $(
                let _ = _the_bitset.set($value);
            )*
            _the_bitset
        }
    };
}

#[macro_export]
macro_rules! map {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(map!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { map!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = map!(@count $($key),*);
            let mut _map = HashMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key, $value);
            )*
            _map
        }
    };
}