use std::fmt;
use itertools::izip;

#[derive(Default)]
pub struct BitSet {
    data: Vec<usize>,
}

impl BitSet {
    // INVARIANT: NO TRAILING ZERO CHUNKS
    const BYTES_PER_CHUNK: usize = std::mem::size_of::<usize>();
    const BITS_PER_CHUNK: usize = Self::BYTES_PER_CHUNK * 8;

    pub fn from_usizes<I: Iterator<Item = usize>>(it: I) -> Self {
        let data = it.collect();
        let mut me = Self { data };
        me.strip_trailing_zeroes();
        me
    }
    pub fn get_chunk(&self, vec_idx: usize) -> usize {
        self.data.get(vec_idx).cloned().unwrap_or(0)
    }
    pub fn iter_chunks(&self) -> impl Iterator<Item=usize> + '_ {
        self.data.iter().cloned()
    }
    pub fn from_usize(chunk: usize) -> Self {
        if chunk == 0 {
            Self { data: vec![] }
        } else {
            Self { data: vec![chunk] }
        }
    }
    pub fn into_usizes(self) -> Vec<usize> {
        self.data
    }

    pub fn with_capacity(min_capacity: usize) -> Self {
        let chunks = if min_capacity.is_power_of_two() {
            min_capacity
        } else {
            min_capacity + 1
        } / 64;
        // let chunks = min_capacity + 1
        Self {
            data: std::iter::repeat(0).take(chunks).collect(),
        }
    }
    pub fn capacity(&self) -> usize {
        self.data.capacity() * Self::BITS_PER_CHUNK
    }
    pub fn set(&mut self, mut idx: usize) -> bool {
        idx += 1;
        let mask = idx % Self::BITS_PER_CHUNK;
        let chunk_idx = idx / Self::BITS_PER_CHUNK;
        while self.data.len() <= chunk_idx {
            self.data.push(0);
        }
        let chunk = &mut self.data[chunk_idx];
        let was_set: bool = (*chunk & mask) != 0;
        *chunk |= mask;
        was_set
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
    pub fn intersects_with(&self, other: &Self) -> bool {
        for (&a, &b) in izip!(self.data.iter(), other.data.iter()) {
            if a & b != 0 {
                return true;
            }
        }
        false
    }
    pub fn is_superset(&self, other: &Self) -> bool {
        if self.data.len() < other.data.len() {
            return false;
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
    fn strip_trailing_zeroes(&mut self) {
        // restore invariant
        while let Some(x) = self.data.pop() {
            if x != 0 {
                // whoops! wasn't zero
                self.data.push(x);
                return;
            }
        }
    }
    pub fn difference_with(&mut self, other: &Self) {
        for (s, &o) in self.data.iter_mut().zip(other.data.iter()) {
            *s &= !o
        }
        self.strip_trailing_zeroes();
    }
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

pub mod adaptors {
    use crate::bitset::BitSet;

    pub trait BitSetIter: Sized {
        fn next_chunk(&mut self) -> Option<usize>;
        fn iter_set(mut self) -> BitWalker<Self> {
            BitWalker {
                next_chunk: self.next_chunk(),
                chunk_idx_offset: 0,
                next_idx_in_chunk: 0,
                t: self,
            }
        }
        fn is_superset(mut self, mut other: Self) -> bool {
            loop {
                match [self.next_chunk(), other.next_chunk()] {
                    [_, None] => return true,
                    [None, Some(mut b)] => {
                        // a ran out first. check if b is nulls
                        loop {
                            if b != 0 {
                                return false;
                            }
                            b = match other.next_chunk() {
                                None => return true,
                                Some(b) => b,
                            }
                        }
                    }
                    [Some(a), Some(b)] => {
                        // return false if there is a bit in b not in a
                        if (b & !a) != 0 {
                            return false;
                        }
                    }
                }
            }
        }
    }
    pub struct BitWalker<T: BitSetIter> {
        next_chunk: Option<usize>,
        chunk_idx_offset: usize,
        next_idx_in_chunk: usize,
        t: T,
    }
    impl<T: BitSetIter> Iterator for BitWalker<T> {
        type Item = usize;
        fn next(&mut self) -> Option<usize> {
            loop {
                match self.next_chunk {
                    None => return None,
                    Some(x) => {
                        self.next_idx_in_chunk += 1;
                        if self.next_idx_in_chunk >= BitSet::BITS_PER_CHUNK {
                            self.next_idx_in_chunk = 0;
                            self.next_chunk = self.t.next_chunk();
                            self.chunk_idx_offset += BitSet::BITS_PER_CHUNK;
                        }
                        let i = self.next_idx_in_chunk - 1;
                        let mask = 1 << i;
                        if x & mask != 0 {
                            // was true!
                            return Some(self.chunk_idx_offset + i);
                        }
                    }
                }
            }
        }
    }

    #[derive(Debug)]
    pub struct Identity<'a> {
        next_chunk_idx: usize,
        bitset: &'a BitSet,
    }
    impl<'a> Identity<'a> {
        pub fn new(bitset: &'a BitSet) -> Self {
            Identity {
                next_chunk_idx: 0,
                bitset,
            }
        }
    }
    impl<'a> std::convert::From<&'a BitSet> for Identity<'a> {
        fn from(bitset: &'a BitSet) -> Self {
            Identity::new(bitset)
        }
    }
    impl<'a> BitSetIter for Identity<'a> {
        fn next_chunk(&mut self) -> Option<usize> {
            let got = self.bitset.data.get(self.next_chunk_idx).cloned();
            if got.is_some() {
                self.next_chunk_idx += 1;
            }
            got
        }
    }

    #[derive(derive_new::new, Debug)]
    pub struct Repeat {
        what: usize,
    }
    impl BitSetIter for Repeat {
        fn next_chunk(&mut self) -> Option<usize> {
            if self.what == 0 {
                None
            } else {
                Some(self.what)
            }
        }
    }

    #[derive(Debug, derive_new::new)]
    pub struct Or<A: BitSetIter, B: BitSetIter>(A, B);
    impl<A: BitSetIter, B: BitSetIter> BitSetIter for Or<A, B> {
        fn next_chunk(&mut self) -> Option<usize> {
            match [self.0.next_chunk(), self.1.next_chunk()] {
                [x, None] | [None, x] => x,
                [Some(x), Some(y)] => Some(x | y),
            }
        }
    }

    #[derive(Debug, derive_new::new)]
    pub struct And<A: BitSetIter, B: BitSetIter>(A, B);
    impl<A: BitSetIter, B: BitSetIter> BitSetIter for And<A, B> {
        fn next_chunk(&mut self) -> Option<usize> {
            match [self.0.next_chunk(), self.1.next_chunk()] {
                [x, None] | [None, x] => x,
                [Some(x), Some(y)] => Some(x & y),
            }
        }
    }

    #[test]
    pub fn bitset_tests() {
        let a = BitSet::from_usize(0b0000001);
        let b = BitSet::from_usize(0b0000010);
        let c = BitSet::from_usize(0b0000011);
        let ia = Identity::new(&a);
        let ib = Identity::new(&b);
        let ic = Identity::new(&c);
        for i in And(Or(ia, ib), ic).iter_set() {
            println!("i={:?}", i);
            milli_sleep!(100)
        }
    }
}
