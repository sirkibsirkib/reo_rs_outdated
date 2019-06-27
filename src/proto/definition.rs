use super::*;

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
    UnknownType {
        loc_id: LocId,
    },
    SynchronousFiring {
        loc_id: LocId,
    },
    LocCannotGet {
        loc_id: LocId,
    },
    LocCannotPut {
        loc_id: LocId,
    },
    TypeMismatch {
        rule_id: usize,
        action_id: usize,
        loc_id_putter: LocId,
        loc_id_getter: LocId,
    },
    MemoryFillPromiseBroken {
        loc_id: LocId,
    },
}

pub struct ProtoBuilder {
    mem_storage: Storage,
    init_mems: HashMap<LocId, *mut u8>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LocKind {
    PortPutter,
    PortGetter,
    MemInitialized,
    MemUninitialized,
}
impl LocKind {
    fn can_put(self) -> bool {
        use LocKind::*;
        match self {
            PortGetter => false,
            _ => true,
        }
    }
    fn can_get(self) -> bool {
        use LocKind::*;
        match self {
            PortPutter => false,
            _ => true,
        }
    }
    fn is_mem(self) -> bool {
        use LocKind::*;
        match self {
            PortPutter | PortGetter => false,
            MemInitialized | MemUninitialized => true,
        }
    }
}

pub struct TypelessProtoDef {
    pub structure: ProtoDef,
    pub loc_kinds: HashMap<LocId, LocKind>,
}

impl ProtoBuilder {
    pub fn new() -> Self {
        Self {
            mem_storage: Default::default(),
            init_mems: Default::default(),
        }
    }
    pub fn define_init_memory<T: 'static>(&mut self, id: LocId, t: T) {
        let ptr = self.mem_storage.move_value_in(t);
        let was = self.init_mems.insert(id, ptr);
        assert!(was.is_none());
    }
    pub fn finish<P: Proto>(self) -> Result<ProtoAll, ProtoBuildErr> {
        use ProtoBuildErr::*;
        let typeless_proto_def = P::typeless_proto_def();
        let mut memory_bits: BitSet = typeless_proto_def
            .loc_kinds
            .iter()
            .filter(|(_, &loc_kinds)| loc_kinds == LocKind::MemInitialized)
            .map(|(&id, _)| id)
            .collect();
        memory_bits.pad_trailing_zeroes_to_capacity(Self::max_loc_id(typeless_proto_def));

        let ready: BitSet = typeless_proto_def
            .loc_kinds
            .iter()
            .filter(|(_, loc_kinds)| loc_kinds.is_mem())
            .map(|(&id, _)| id)
            .collect();

        let (id_2_type_id, type_id_2_info) = {
            let mut id_2_type_id: HashMap<LocId, TypeId> = Default::default();
            let mut type_id_2_info: HashMap<TypeId, Arc<TypeInfo>> = Default::default();
            for loc_id in typeless_proto_def.loc_kinds.keys().copied() {
                let type_info = P::loc_type(loc_id).ok_or(UnknownType { loc_id })?;
                let type_id = type_info.type_id;
                id_2_type_id.entry(loc_id).or_insert(type_id);
                type_id_2_info
                    .entry(type_id)
                    .or_insert_with(|| Arc::new(type_info));
            }
            (id_2_type_id, type_id_2_info)
        };

        let id_2_info = |id: &LocId| {
            let type_id = id_2_type_id.get(id).unwrap();
            type_id_2_info.get(type_id).unwrap()
        };

        let unclaimed_ports = typeless_proto_def
            .loc_kinds
            .iter()
            .filter_map(|(&id, loc_kinds)| {
                let role = match loc_kinds {
                    LocKind::PortPutter => PortRole::Putter,
                    LocKind::PortGetter => PortRole::Getter,
                    _ => return None,
                };
                let info = PortInfo {
                    role,
                    type_id: *id_2_type_id.get(&id).unwrap(),
                };
                Some((id, info))
            })
            .collect();

        let mem_refs = self
            .init_mems
            .iter()
            .map(|(&loc_id, &ptr)| (ptr, loc_id))
            .collect();

        let spaces = typeless_proto_def
            .loc_kinds
            .iter()
            .map(|(id, loc_kinds)| {
                let space = match loc_kinds {
                    LocKind::PortPutter => Space::PoPu(PoPuSpace::new({ id_2_info(id).clone() })),
                    LocKind::PortGetter => Space::PoGe(PoGeSpace::new()),
                    LocKind::MemInitialized => Space::Memo({
                        let ptr: *mut u8 = *self.init_mems.get(id).unwrap();
                        let type_info = id_2_info(id).clone();
                        MemoSpace::new(ptr, type_info)
                    }),
                    LocKind::MemUninitialized => Space::Memo({
                        let ptr: *mut u8 = std::ptr::null_mut();
                        let type_info = id_2_info(id).clone();
                        MemoSpace::new(ptr, type_info)
                    }),
                };
                (*id, space)
            })
            .collect();

        let rules = Self::build_rules::<P>(&id_2_type_id)?;

        // println!(
        //     " READY {:#?}",
        //     (&rules, &memory_bits, &ready, &unclaimed_ports, &spaces)
        // );
        let r = ProtoR { spaces, rules };
        let w = Mutex::new(ProtoW {
            memory_bits,
            active: ProtoActive {
                ready,
                storage: self.mem_storage,
                mem_refs,
            },
            commitment: None,
            ready_tentative: BitSet::default(),
            awaiting_states: vec![],
            unclaimed_ports,
        });
        Ok(ProtoAll { w, r })
    }

    fn max_loc_id(typeless_proto_def: &'static TypelessProtoDef) -> LocId {
        typeless_proto_def
            .loc_kinds
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
    }

    fn build_rules<P: Proto>(
        id_2_type_id: &HashMap<LocId, TypeId>,
    ) -> Result<Vec<RunRule>, ProtoBuildErr> {
        let typeless_proto_def = P::typeless_proto_def();
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (rule_id, rule_def) in typeless_proto_def.structure.rules.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut guard_full = BitSet::default();
            let mut actions = vec![];
            let mut assign_vals = BitSet::default();
            let mut assign_mask = BitSet::default();

            for (action_id, action_def) in rule_def.actions.iter().enumerate() {
                let mut mg = vec![];
                let mut pg = vec![];

                let p = action_def.putter;
                let p_kind = typeless_proto_def
                    .loc_kinds
                    .get(&p)
                    .ok_or(UnknownType { loc_id: p })?;
                let p_type = id_2_type_id.get(&p).unwrap();
                if !p_kind.can_put() {
                    return Err(LocCannotPut { loc_id: p });
                }
                let mem_putter = p_kind.is_mem();
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
                    let g_kind = typeless_proto_def
                        .loc_kinds
                        .get(&g)
                        .ok_or(UnknownType { loc_id: g })?;
                    let g_type = id_2_type_id.get(&g).unwrap();
                    if p_type != g_type {
                        return Err(TypeMismatch {
                            rule_id,
                            action_id,
                            loc_id_putter: p,
                            loc_id_getter: g,
                        });
                    }
                    if !g_kind.can_get() {
                        return Err(LocCannotGet { loc_id: g });
                    }
                    let mem_getter = g_kind.is_mem();
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
            let c = Self::max_loc_id(typeless_proto_def);
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

struct IdkProto;
impl Proto for IdkProto {
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref DEF: TypelessProtoDef = TypelessProtoDef {
                structure: ProtoDef{
                    rules: vec![
                        rule![Formula::True; 0=>1],
                    ]
                },
                loc_kinds: map! {
                    0 => LocKind::PortPutter,
                    1 => LocKind::PortGetter,
                },
            };
        }
        &DEF
    }
    fn fill_memory(_: LocId, _p: MemFillPromise) -> Option<MemFillPromiseFulfilled> {
        None
    }
    fn loc_type(loc_id: LocId) -> Option<TypeInfo> {
        Some(match loc_id {
            0 | 1 => TypeInfo::new::<u32>(),
            _ => return None,
        })
    }
}

#[test]
fn instantiate_idk() {
    let _x = IdkProto::instantiate();
    println!("DONE");
}
