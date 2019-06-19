use crate::LocId;
use hashbrown::HashMap;
use std::any::TypeId;
use std::marker::PhantomData;
use std::sync::Arc;


#[derive(Debug, Copy, Clone)]
pub struct ProtoDef {
    pub rules: &'static [RuleDef],
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

#[derive(Debug)]
pub enum ProtoVerifyError {
    // TODO
}
type Filled = bool;
struct ProtoBuilder<P: Proto> {
    phantom: PhantomData<P>,
    proto_def: ProtoDef,
    mem_bytes: Vec<u8>,
    contents: HashMap<*mut u8, (Filled, TypeId)>,
}
impl<P: Proto> ProtoBuilder<P> {
    pub fn new(proto_def: ProtoDef) -> Self {
        Self {
            phantom: PhantomData::default(),
            proto_def,
            mem_bytes: vec![],
            contents: HashMap::default(),
        }
    }
    pub fn init_memory<T: 'static>(&mut self, t: T) {
        // TODO may do error
        let _ = t;
    }
    pub fn finish(self) -> Result<ProtoAll<P>, ProtoVerifyError> {
        self.verify()?;
        Ok(ProtoAll {
            p: Default::default(),
            mem_bytes: self.mem_bytes,
        })
    }
    fn verify(&self) -> Result<(), ProtoVerifyError> {
        // TODO
        Ok(())
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

    const PROTO_DEF: ProtoDef = ProtoDef {
        rules: &[RuleDef {
            guard: Formula::True,
            actions: &[ActionDef {
                putter: 0,
                getters: &[1, 2],
            }],
        }],
    };


    fn instantiate() -> Arc<ProtoAll<Self>> {
        let mem = ProtoBuilder::new(Self::PROTO_DEF);
        Arc::new(mem.finish().expect("Bad Reo-generated code"))
    }
}
