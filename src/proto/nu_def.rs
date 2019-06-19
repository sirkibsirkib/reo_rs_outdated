use crate::proto::{PortInfo, PortRole};
use crate::proto::{RunRule, RunAction};
use crate::proto::{ProtoAll, ProtoActive, ProtoW, ProtoR, Space};
use crate::LocId;
use hashbrown::{HashMap};
use std::any::TypeId;

use std::sync::Arc;
use parking_lot::Mutex;
use crate::bitset::BitSet;


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

#[derive(Debug, Copy, Clone)]
pub enum ProtoBuildErr {
    SynchronousFiring { loc_id: LocId },
    LocCannotGet { loc_id: LocId },
    LocCannotPut { loc_id: LocId },
}
type Filled = bool;

// TODO ProtoBuilder and ProtoAll are dummy-parameterized
struct ProtoBuilder {
    proto_def: ProtoDef,
    mem_data: Vec<u8>,
    contents: HashMap<LocId, (*mut u8, Filled, TypeId)>,
}
impl ProtoBuilder {
    pub fn new(proto_def: ProtoDef) -> Self {
        Self {
            proto_def,
            mem_data: vec![],
            contents: HashMap::default(),
        }
    }
    pub fn init_memory<T: 'static>(&mut self, t: T) {
        // TODO may do error
        let _ = t;
    }
    pub fn finish(self) -> Result<ProtoAll, ProtoBuildErr> {
        let spaces: Vec<Space> = vec![];
        let loc_kinds = HashMap::<LocId, LocKind>::default();
        let loc_types = HashMap::<LocId, TypeId>::default();
        let memory_bits = self.contents.iter()
            .filter(|(id, (_, filled, _))| *filled)
            .map(|(id, _)| *id)
            .collect();
        let rules = self.build_rules(&loc_kinds)?;
        let ready = loc_kinds
            .iter()
            .filter(|(_,v)| v.is_mem())
            .map(|(k,_)| *k)
            .collect();

        let unclaimed_ports = loc_kinds.iter()
            .filter_map(|(id,t)| if t.is_port() {
                let info = PortInfo {
                    role: match t.is_putter() {
                        true => PortRole::Putter,
                        false => PortRole::Getter,
                    },
                    type_id: *loc_types.get(id).expect("LocID has unbounded type!"),
                };
                Some((*id, info))
            } else {
                None
            })
            .collect();
        let free_mems: HashMap<TypeId, Vec<*mut u8>> = {
            let mut free_mems = HashMap::new();
            for (k, v) in loc_kinds.iter() {

            }
            free_mems
        };  
        let r = ProtoR {
            mem_data: self.mem_data,
            spaces,
            rules,
        };
        let w = Mutex::new(ProtoW {
            memory_bits,
            active: ProtoActive {
                ready,
                free_mems,
                mem_refs: HashMap::default(),
            },
            commitment: None,
            ready_tentative: BitSet::default(),
            awaiting_states: vec![],
            unclaimed_ports,
        });
        Ok(ProtoAll { w, r })
    }

    fn build_rules(&self, loc_kinds: &HashMap<LocId, LocKind>) -> Result<Vec<RunRule>, ProtoBuildErr> {
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (_rule_id, rule_def) in self.proto_def.rules.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut guard_full = BitSet::default();
            let mut actions = vec![];
            let mut assign_vals = BitSet::default();
            let mut assign_mask = BitSet::default();

            for action_def in rule_def.actions.iter() {
                let mut mg = vec![];
                let mut pg = vec![];

                let p = action_def.putter;
                let p_type = loc_kinds.get(&p).unwrap();
                if !p_type.is_putter() {
                    return Err(LocCannotPut { loc_id: p });
                }
                let mem_putter = p_type.is_mem();
                if guard_ready.test(p) {
                    return Err(SynchronousFiring { loc_id: p });
                }
                if mem_putter {
                    guard_full.set_to(p, true); // putter must be full!
                    assign_vals.set_to(p, false); // putter becomes empty!
                    assign_mask.set_to(p, true); // putter memory fullness changes!
                }
                let was = guard_ready.set_to(p, true); // putter is involved!
                if was {
                    // this putter was involved in a different action!
                    return Err(SynchronousFiring { loc_id: p });
                }

                use itertools::Itertools;
                for &g in action_def.getters.iter().unique() {

                    let g_type = loc_kinds.get(&g).unwrap();
                    if !g_type.is_getter() {
                        return Err(LocCannotGet { loc_id: g });
                    }
                    let mem_getter = g_type.is_mem();
                    match mem_getter {
                        false => &mut pg,
                        true => &mut mg,
                    }
                    .push(g);
                    if mem_getter {
                        guard_full.set_to(g, false); // getter must be empty!
                        assign_vals.set_to(g, true); // getter becomes full!
                        assign_mask.set_to(g, true); // getter memory fullness changes!
                    }
                    let was_set = guard_ready.set_to(g, true);
                    if was_set {
                        // oh no! this getter was already involved in the firing
                        if g == p && mem_putter {
                            // nevermind. its OK for memory to put and get to itself
                            guard_full.set_to(g, true); // getter must be full (because its also the putter)!
                            assign_mask.set_to(g, false); // getter memory fullness DOES NOT change (Full -> Full)!
                            assign_vals.set_to(g, false);
                        } else {
                            return Err(SynchronousFiring { loc_id: g });
                        }
                    }
                }
                actions.push(match mem_putter {
                    false => RunAction::PortPut { putter: p, mg, pg },
                    true => RunAction::MemPut { putter: p, mg, pg },
                });
            }
            let c = loc_kinds.keys().copied().max().unwrap_or(0);
            guard_ready.pad_trailing_zeroes_to_capacity(c);
            guard_full.pad_trailing_zeroes_to_capacity(c);
            assign_vals.pad_trailing_zeroes_to_capacity(c);
            assign_mask.pad_trailing_zeroes_to_capacity(c);
            rules.push(RunRule {
                guard_ready,
                guard_full,
                guard_pred: rule_def.guard.clone(),
                assign_vals,
                assign_mask,
                actions,
            });
        }
        Ok(rules)
    }
}

#[derive(Debug, Copy, Clone)]
enum LocKind {
    PortPutter,
    PortGetter,
    Memory,
}
impl LocKind {
    fn is_port(self) -> bool {
        use LocKind::*;
        match self {
            PortPutter | PortGetter => true,
            Memory => false,
        }
    }
    fn is_mem(self) -> bool {
        !self.is_port()
    }
    fn is_putter(self) -> bool {
        use LocKind::*;
        match self {
            PortPutter | Memory => true,
            PortGetter => false,
        }
    }
    fn is_getter(self) -> bool {
        use LocKind::*;
        match self {
            PortGetter | Memory => true,
            PortPutter => false,
        }
    }
}

trait Proto: Sized {
    const PROTO_DEF: ProtoDef;
    fn instantiate() -> Arc<ProtoAll>;
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


    fn instantiate() -> Arc<ProtoAll> {
        let mem = ProtoBuilder::new(Self::PROTO_DEF);
        Arc::new(mem.finish().expect("Bad Reo-generated code"))
    }
}
