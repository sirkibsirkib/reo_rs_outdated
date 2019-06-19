

use std::marker::PhantomData;
use std::sync::Arc;
use crate::LocId;
use hashbrown::HashMap;
use std::any::TypeId;

pub enum ProtoVerifyError {

}

#[derive(Debug, Copy, Clone)]
pub struct ProtoDef {
    pub rules: &'static [RuleDef],
}
impl ProtoDef {
    pub fn verify() -> Result<(), ProtoVerifyError> {
        // TODO
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RuleDef {
    pub guard: Formula, 
    pub actions: &'static [ActionDef],
}

#[derive(Debug, Copy, Clone)]
pub struct ActionDef {
    pub putter: usize,
    pub getters: &'static [usize],
}

type Formulae = &'static [Formula];
#[derive(Debug, Copy, Clone)]
pub enum Formula {
    True,
    And(Formulae),
    Or(Formulae),
    None(Formulae),
    Eq(LocId, LocId),
}


type Filled = bool;
struct ProtoBuilder<P: Proto> {
    phantom: PhantomData<P>,
    proto_def: ProtoDef,
    mem_bytes: Vec<u8>,
    contents: HashMap<*mut u8, (Filled, TypeId)>,
}
impl<P: Proto> ProtoBuilder<P> {
    fn new(proto_def: ProtoDef) -> Self {
        Self {
            phantom: PhantomData::default(),
            proto_def,
            mem_bytes: vec![],
            contents: HashMap::default(),
        }
    }
    fn finish(self) -> Result<ProtoAll<P>, ()> {
        Ok(ProtoAll {
            p: Default::default(),
            mem_bytes: self.mem_bytes,
        })
    }
}

struct ProtoAll<P: Proto> {
    p: PhantomData<P>,
    mem_bytes: Vec<u8>,
}

trait Proto: Sized {
    const PROTO_DEF: ProtoDef;
    fn instantiate() -> Arc<ProtoAll<Self>>;
}

struct Fifo3;
impl Proto for Fifo3 {
    const PROTO_DEF: ProtoDef = ProtoDef{ rules: &[
        RuleDef { guard: Formula::True,
            actions: &[
                ActionDef { putter:0, getters: &[1,2] },
            ]
        }
    ]};
    fn instantiate() -> Arc<ProtoAll<Self>> {
        let mem = ProtoBuilder::new(Self::PROTO_DEF);
        Arc::new(mem.finish().expect("Bad Reo-generated code"))
    }
} 