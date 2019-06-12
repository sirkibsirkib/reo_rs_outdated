use itertools::izip;
use crate::bitset::BitSet;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]

pub enum Quat {
    FF, FT, TF, TT,
}
impl Quat {

    #[inline]
    fn first_is_true(self) -> bool {
        use Quat::*;
        match self {
            FF | FT => false,
            TF | TT => true, 
        } 
    }

    #[inline]
    fn second_is_true(self) -> bool {
        use Quat::*;
        match self {
            FT | TT => true,
            FF | TF => false,
        } 
    }
}

#[derive(Debug, Default, Clone)]
pub struct QuatSet {
    data: BitSet,
}
impl QuatSet {

    #[inline]
    pub fn set(&mut self, idx: usize, quat: Quat) {
        self.data.set_to(idx * 2, quat.first_is_true());
        self.data.set_to(idx * 2 + 1, quat.second_is_true());
    }
}

pub fn proto_rule_can_fire(memory: &QuatSet, ready: &QuatSet, guard: &QuatSet) -> bool {
    for (&m, &r, &g) in izip!(memory.data.data.iter(), ready.data.data.iter(), guard.data.data.iter()) {
        if (m|r) & g != 0 {
            return false
        }
    }
    let l2 = memory.data.data.len().min(ready.data.data.len());
    if guard.data.data.len() > l2 {
        // ensure all excess guard checks are zero
        !guard.data.data[l2..].iter().any(|&x| x != 0)
    } else {
        true
    }
}

pub fn predicate_describes_state(memory: &QuatSet, pred: &QuatSet) -> bool {
    for (&m, &p) in izip!(memory.data.data.iter(), pred.data.data.iter()) {
        if m & p != 0 {
            return false
        }
    }
    let l2 = memory.data.data.len();
    if pred.data.data.len() > l2 {
        // ensure all excess guard checks are zero
        !pred.data.data[l2..].iter().any(|&x| x != 0)
    } else {
        true
    }
}