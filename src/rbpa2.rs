use crate::bitset::BitSet;
use crate::proto::ProtoDef;

use hashbrown::HashSet;

use crate::LocId;

/*
purpose of this module:
1. compute projected Rbpa for a group

*/

pub struct Rbpa {
    proto_ports: usize,
    rbpa_rules: Vec<RbpaRule>,
}

#[derive(Debug)]
enum RbpaError {}
impl Rbpa {
    fn new_projected(proto_def: ProtoDef, local_set: HashSet<LocId>) -> Result<Self, RbpaError> {
        unimplemented!()
    }
}

struct RbpaRule {
    // 00 means X
    // 10 means T
    // 01 means F
    guard: BitSet,
}
