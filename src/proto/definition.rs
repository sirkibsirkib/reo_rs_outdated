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
    UnknownType { loc_id: LocId },
    SynchronousFiring { loc_id: LocId },
    LocCannotGet { loc_id: LocId },
    LocCannotPut { loc_id: LocId },
}

pub struct ProtoBuilder {
    proto_def: &'static ProtoDef,
    mem_storage: Storage,
    mem_defs: HashMap<LocId, Option<*mut u8>>,
}
impl ProtoBuilder {
    pub fn new(proto_def: &'static ProtoDef) -> Self {
        Self {
            proto_def,
            mem_storage: Default::default(),
            mem_defs: Default::default(),
        }
    }
    unsafe fn define_init_memory<T: 'static>(&mut self, id: LocId, t: T) {
        let ptr = self.mem_storage.move_value_in(t);
        let was = self.mem_defs.insert(id, Some(ptr));
        assert!(was.is_none());
    }
    pub fn uninit_memory<T: 'static>(&mut self, id: LocId) {
        let was = self.mem_defs.insert(id, None);
        assert!(was.is_none());
    }
    pub fn finish(self, loc_info: &HashMap<LocId, LocInfo>) -> Result<ProtoAll, ProtoBuildErr> {
        /* here we construct a proto according to the specification.
        Failure may be a result of:
        1. The type for a used LocId is not derivable.
        2. A LocId is associated with a type which is not provided in the TypeInfo map.
        */
        let memory_bits = self
            .mem_defs
            .iter()
            .filter_map(|(&k, v)| if v.is_some() { Some(k) } else { None })
            .collect();
        let ready = loc_info
            .iter()
            .filter_map(|(&k, v)| if v.kind.is_mem() { Some(k) } else { None })
            .collect();
        let unclaimed_ports = loc_info
            .iter()
            .filter_map(|(&k, v)| {
                let role = match v.kind {
                    LocKind::PortPutter => PortRole::Putter,
                    LocKind::PortGetter => PortRole::Getter,
                    LocKind::Memory => return None,
                };
                let info = PortInfo {
                    role,
                    type_id: v.type_info.type_id,
                };
                Some((k, info))
            })
            .collect();
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
                    let ptr = self
                        .mem_defs
                        .get(id)
                        .unwrap()
                        .unwrap_or(std::ptr::null_mut());
                    let info = type_id_2_info
                        .get(&loc_info.type_info.type_id)
                        .unwrap()
                        .clone();
                    MemoSpace::new(ptr, info)
                }),
            })
            .collect();

        println!("OK");

        let rules = self.build_rules(&loc_info)?;

        println!("{:?}", (&rules, &memory_bits, &ready));
        let r = ProtoR { spaces, rules };
        let w = Mutex::new(ProtoW {
            memory_bits,
            active: ProtoActive {
                ready,
                storage: self.mem_storage,
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
                let p_type = loc_info.get(&p).ok_or(UnknownType{loc_id:p})?.kind;
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
                    let g_type = loc_info.get(&g).ok_or(UnknownType{loc_id:g})?.kind;
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

#[derive(Debug)]
pub struct LocInfo {
    kind: LocKind,
    type_info: Arc<TypeInfo>,
}
impl LocInfo {
    fn new<T: 'static>(kind: LocKind) -> Self {
        Self {
            kind,
            type_info: Arc::new(TypeInfo::new::<T>()),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum LocKind {
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

struct IdkProto;
impl Proto for IdkProto {
    fn definition() -> &'static ProtoDef {
        lazy_static::lazy_static! {
            static ref LAZY: ProtoDef = ProtoDef {
                rules: vec![
                    rule![Formula::True; 0=>1],
                    rule![Formula::True; 0=>1],
                ]
            };
        }
        &LAZY
    }
    fn loc_info() -> &'static HashMap<LocId, LocInfo> {
        lazy_static::lazy_static! {
            static ref LAZY: HashMap<LocId, LocInfo> = {
                use LocKind::*;
                map! {
                    0 => LocInfo::new::<u32>(PortPutter),
                    1 => LocInfo::new::<u32>(PortGetter),
                }
            };
        }
        &LAZY
    }
}

#[test]
fn instantiate_fifo3() {
    let _x = IdkProto::instantiate();
}
