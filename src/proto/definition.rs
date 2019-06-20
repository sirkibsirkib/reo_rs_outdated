use crate::proto::reflection::TypeInfo;
use crate::proto::traits::Proto;
use crate::proto::{MemoSpace, PoGeSpace, PoPuSpace, PortInfo, PortRole};
use crate::proto::{ProtoActive, ProtoAll, ProtoR, ProtoW, Space};
use crate::proto::{RunAction, RunRule};
use crate::LocId;
use hashbrown::HashMap;
use std::any::TypeId;

use crate::bitset::BitSet;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ProtoDef {
    pub rules: Vec<RuleDef>,
}

#[derive(Debug, Clone)]
pub struct RuleDef {
    pub guard: Formula,
    pub actions: Vec<ActionDef>,
}

#[derive(Debug, Clone)]
pub struct ActionDef {
    pub putter: usize,
    pub getters: Vec<LocId>,
}

#[derive(Debug, Clone)]
pub enum Formula {
    True,
    And(Vec<Formula>),
    Or(Vec<Formula>),
    None(Vec<Formula>),
    Eq(LocId, LocId),
}

#[derive(Debug, Copy, Clone)]
pub enum ProtoBuildErr {
    UnknownType { loc_id: LocId },
    SynchronousFiring { loc_id: LocId },
    LocCannotGet { loc_id: LocId },
    LocCannotPut { loc_id: LocId },
}

type IsInit = bool;
struct ProtoBuilder {
    proto_def: &'static ProtoDef,
    mem_data: Vec<u8>,
    contents: HashMap<LocId, (*mut u8, IsInit, TypeInfo)>,
}
impl ProtoBuilder {
    pub fn new(proto_def: &'static ProtoDef) -> Self {
        Self {
            proto_def,
            mem_data: vec![],
            contents: HashMap::default(),
        }
    }
    pub fn uninit_memory<T: 'static>(&mut self, id: LocId) {
        assert!(!self.contents.contains_key(&id));
        let t: T = unsafe {
            std::mem::MaybeUninit::uninit().assume_init()
        };
        let b = Box::new(t);
        let b: *mut u8 = unsafe {
            std::mem::transmute(b)
        };
        self.contents.insert(id, (b, false, TypeInfo::new::<T>()));
    }
    pub fn init_memory<T: 'static>(&mut self, id: LocId, t: T) {
        assert!(!self.contents.contains_key(&id));
        let b = Box::new(t);
        let b: *mut u8 = unsafe {
            std::mem::transmute(b)
        };
        self.contents.insert(id, (b, true, TypeInfo::new::<T>()));
    }
    pub fn finish(self, loc_info: &HashMap<LocId, LocInfo>) -> Result<ProtoAll, ProtoBuildErr> {
        /* here we construct a proto according to the specification.
        Failure may be a result of:
        1. The type for a used LocId is not derivable.
        2. A LocId is associated with a type which is not provided in the TypeInfo map.
        */

        // TODO safe unwinding

        let memory_bits = self.contents.iter().filter_map(|(&k,v)| {
            if v.1 {
                Some(k)
            } else {
                None
            }
        }).collect();
        let free_mems = {
            let mut m = HashMap::default();
            for (id, (ptr, is_free, _info)) in self.contents.iter() {
                if *is_free {
                    let type_id = loc_info.get(id).unwrap().type_info.type_id;
                    m.entry(type_id).or_insert_with(|| vec![]).push(*ptr);
                }
            }
            m
        };
        let ready  = loc_info.iter().filter_map(|(&k, v)| {
            if v.kind.is_mem() {
                Some(k)
            } else {
                None
            }
        }).collect();
        let unclaimed_ports = loc_info.iter().filter_map(|(&k, v)| {
            let role = match v.kind {
                LocKind::PortPutter => PortRole::Putter,
                LocKind::PortGetter => PortRole::Getter,
                LocKind::Memory => return None,
            };
            let info = PortInfo {
                role,
                type_id: v.type_info.type_id
            };
            Some((k, info))
        }).collect();
        let type_id_2_info: HashMap<_, _> = loc_info
            .values()
            .map(|loc_info| (loc_info.type_info.type_id, loc_info.type_info.clone()))
            .collect();
        let spaces = loc_info
            .iter()
            .map(|(id, loc_info)| match loc_info.kind {
                LocKind::PortPutter => Space::PoPu(PoPuSpace::new(
                    type_id_2_info
                        .get(&loc_info.type_info.type_id)
                        .unwrap()
                        .clone(),
                )),
                LocKind::PortGetter => Space::PoGe(PoGeSpace::new()),
                LocKind::Memory => Space::Memo({
                    let ptr: *mut u8 = self.contents.get(id).unwrap().0;
                    let info = type_id_2_info
                        .get(&loc_info.type_info.type_id)
                        .unwrap()
                        .clone();
                    MemoSpace::new(ptr, info)
                }),
            })
            .collect();

        let rules = self.build_rules(&loc_info)?;



        println!("{:?}", (&rules, &memory_bits, &ready));
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

    fn build_rules(
        &self,
        loc_info: &HashMap<LocId, LocInfo>,
    ) -> Result<Vec<RunRule>, ProtoBuildErr> {
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
                let p_type = loc_info.get(&p).unwrap().kind;
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
                    let g_type = loc_info.get(&g).unwrap().kind;
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
            let c = loc_info.keys().copied().max().unwrap_or(0);
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


// TODO TEST
impl Drop for ProtoBuilder {
    fn drop(&mut self) {
        for (_k, (ptr, is_init, type_info)) in self.contents.iter() {
            let ptr: *mut u8 = *ptr;
            unsafe {
                if *is_init {
                    // 1. drop the contents
                    type_info.drop_fn.execute(ptr);
                }
                // 2. drop the box itself
                let b: Box<()> = std::mem::transmute(ptr);
                drop(b);
            }
        }
    }
}


#[derive(Debug)]
pub struct LocInfo {
    kind: LocKind,
    type_info: Arc<TypeInfo>,
}
impl LocInfo {
    fn new<T: 'static>(kind: LocKind) -> Self {
        Self {
            kind,
            type_info: Arc::new(TypeInfo::new::<T>())
        }
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

macro_rules! rule {
    ( $formula:expr ; $( $putter:tt => $( $getter:tt  ),* );*) => {{
        RuleDef {
            guard: $formula,
            actions: vec![
                $(
                ActionDef {
                    putter: $putter,
                    getters: vec![
                        $(
                            $getter
                        ),*
                    ],
                }
                ),*
            ],
        }
    }};
}

lazy_static::lazy_static! {
    static ref FIFO_DEF: ProtoDef = ProtoDef {
        rules: vec![
            rule![Formula::True; 0=>1,3,4 ; 6=>2,3],
        ]
    };
}



struct Fifo3;
impl Proto for Fifo3 {
    fn definition() -> &'static ProtoDef {
        &FIFO_DEF
    }
    fn instantiate() -> Arc<ProtoAll> {
        let mem = ProtoBuilder::new(Self::definition());
        use LocKind::*;
        let loc_info = map!{
            0 => LocInfo::new::<u32>(PortPutter),
            1 => LocInfo::new::<u32>(PortGetter),
        };
        Arc::new(mem.finish(&loc_info).unwrap())
    }
}

#[test]
fn toottle() {
    let x = Fifo3::instantiate();
}