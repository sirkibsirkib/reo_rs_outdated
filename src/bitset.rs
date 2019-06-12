use itertools::izip;
use std::fmt;

#[derive(Default, Clone)]
pub struct BitSet {
    pub(crate) data: Vec<usize>,
}

pub struct SparseIter<'a> {
    a: &'a BitSet,
    n: Option<usize>,
    maj: usize,
    min: usize,
}
impl<'a> Iterator for SparseIter<'a> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        loop {
            if self.min == BitSet::BITS_PER_CHUNK {
                self.min = 0;
                self.maj += 1;
                self.n = self.a.data.get(self.maj).cloned();
            }
            match self.n {
                Some(x) => {
                    let val = (1 << self.min) & x;
                    self.min += 1;
                    if val != 0 {
                        return Some(self.maj * BitSet::BITS_PER_CHUNK + (self.min - 1));
                    }
                }
                None => return None,
            }
        }
    }
}

pub struct AndIter<'a, 'b> {
    a: &'a BitSet,
    b: &'b BitSet,
    n: Option<usize>,
    maj: usize,
    min: usize,
}
impl<'a, 'b> AndIter<'a, 'b> {
    fn fetch_chunk(a: &BitSet, b: &BitSet, chunk_idx: usize) -> Option<usize> {
        a.data
            .get(chunk_idx)
            .and_then(|x| b.data.get(0).map(|y| x & y))
    }
}
impl<'a, 'b> Iterator for AndIter<'a, 'b> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        loop {
            if self.min == BitSet::BITS_PER_CHUNK {
                self.min = 0;
                self.maj += 1;
                self.n = Self::fetch_chunk(self.a, self.b, self.maj);
            }
            match self.n {
                Some(x) => {
                    let val = (1 << self.min) & x;
                    self.min += 1;
                    if val != 0 {
                        return Some(self.maj * BitSet::BITS_PER_CHUNK + self.min);
                    }
                }
                None => return None,
            }
        }
    }
}

impl BitSet {
    // INVARIANT: NO TRAILING ZERO CHUNKS
    const BYTES_PER_CHUNK: usize = std::mem::size_of::<usize>();
    const BITS_PER_CHUNK: usize = Self::BYTES_PER_CHUNK * 8;

    pub fn from_set_iter<I: Iterator<Item = usize>>(it: I) -> Self {
        let mut me = BitSet::default();
        for i in it {
            me.set_to(i, true);
        }
        me
    }

    pub fn from_chunks<I: Iterator<Item = usize>>(it: I) -> Self {
        let data = it.collect();
        let mut me = Self { data };
        me.strip_trailing_zeroes();
        me
    }
    pub fn get_chunk(&self, vec_idx: usize) -> usize {
        self.data.get(vec_idx).cloned().unwrap_or(0)
    }
    pub fn iter_chunks(&self) -> impl Iterator<Item = usize> + '_ {
        self.data.iter().cloned()
    }
    pub fn iter_sparse(&self) -> SparseIter {
        SparseIter {
            a: self,
            n: self.data.get(0).cloned(),
            maj: 0,
            min: 0,
        }
    }
    pub fn is_empty(&self) -> bool {
        for &x in self.data.iter() {
            if x != 0 {
                return false;
            }
        }
        true
    }
    pub fn iter_and<'a, 'b: 'a>(&'a self, other: &'b Self) -> AndIter {
        AndIter {
            a: self,
            b: other,
            n: AndIter::fetch_chunk(self, other, 0),
            maj: 0,
            min: 0,
        }
    }
    pub fn from_chunk(chunk: usize) -> Self {
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
        let chunks = Self::len_needed_for_capacity(min_capacity);
        // let chunks = min_capacity + 1
        Self {
            data: std::iter::repeat(0).take(chunks).collect(),
        }
    }
    pub fn capacity(&self) -> usize {
        self.data.capacity() * Self::BITS_PER_CHUNK
    }
    pub fn set_to(&mut self, idx: usize, val: bool) -> bool {
        let mask = 1 << (idx % Self::BITS_PER_CHUNK);
        let chunk_idx = idx / Self::BITS_PER_CHUNK;
        let chunk = match (val, self.data.get_mut(chunk_idx)) {
            (false, None) => return false,
            (true, None) => {
                while self.data.len() <= chunk_idx {
                    self.data.push(0);
                }
                &mut self.data[chunk_idx]
            }
            (_, Some(c)) => c,
        };
        let was_set: bool = (*chunk & mask) != 0;
        if val {
            *chunk |= mask;
        } else {
            *chunk &= !mask;
        }
        was_set
    }
    pub fn test(&self, idx: usize) -> bool {
        let mask = 1 << (idx % Self::BITS_PER_CHUNK);
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
    pub fn len_needed_for_capacity(cap: usize) -> usize {
        if cap % Self::BITS_PER_CHUNK == 0 {
            cap / Self::BITS_PER_CHUNK
        } else {
            (cap / Self::BITS_PER_CHUNK) + 1
        }
    }
    pub fn pad_trailing_zeroes_to_capacity(&mut self, cap: usize) {
        let chunks = Self::len_needed_for_capacity(cap);
        self.pad_trailing_zeroes(chunks);
    }
    pub fn pad_trailing_zeroes(&mut self, len: usize) {
        while self.data.len() < len {
            self.data.push(0);
        }
    }
    pub fn strip_trailing_zeroes(&mut self) {
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
        for (s, &o) in izip!(self.data.iter_mut(), other.data.iter()) {
            *s &= !o
        }
        // self.strip_trailing_zeroes();
    }
    pub fn or_with(&mut self, other: &Self) {
        for (s, &o) in izip!(self.data.iter_mut(), other.data.iter()) {
            *s |= o
        }
        if other.data.len() > self.data.len() {
            self.data.extend_from_slice(&other.data[..]);
            // self.strip_trailing_zeroes();
        }
    }
}

impl fmt::Debug for BitSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        for b in self.data.iter().rev().take(1) {
            write!(f, "{:b}", b)?;
        }
        for b in self.data.iter().rev().skip(1) {
            write!(f, ".{:b}", b)?;
        }
        write!(f, "]")
    }
}
