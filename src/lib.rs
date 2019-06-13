#![feature(specialization)]
use std::sync::Arc;

/// generalizes over port and memory cell "name"
pub type LocId = usize;
pub type RuleId = usize;
pub type ProtoHandle = Arc<proto::ProtoAll>;

#[macro_use]
pub mod helper;
pub mod bitset;
pub mod proto;
pub mod rbpa;
pub mod tokens;
