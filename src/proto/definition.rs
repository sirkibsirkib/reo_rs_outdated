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
    mem_storage: Storage,
    init_mems: HashMap<LocId, *mut u8>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LocKindExt {
    PortPutter,
    PortGetter,
    MemInitialized,
    MemUninitialized,
}
impl LocKindExt {
    fn is_port(self) -> bool {
        !self.is_mem()
    }
    fn can_put(self) -> bool {
        use LocKindExt::*;
        match self {
            PortGetter => false,
            _ => true,
        }
    }
    fn can_get(self) -> bool {
        use LocKindExt::*;
        match self {
            PortPutter => false,
            _ => true,
        }
    }
    fn is_mem(self) -> bool {
        use LocKindExt::*;
        match self {
            PortPutter | PortGetter => false,
            MemInitialized | MemUninitialized => true,
        }
    }
}

pub struct TypelessProtoDef {
    pub structure: ProtoDef,
    pub loc_kind_ext: HashMap<LocId, LocKindExt>,
}

impl ProtoBuilder {
    pub fn new() -> Self {
        Self {
            mem_storage: Default::default(),
            init_mems: Default::default(),
        }
    }
    unsafe fn define_init_memory<T: 'static>(&mut self, id: LocId, t: T) {
        let ptr = self.mem_storage.move_value_in(t);
        let was = self.init_mems.insert(id, ptr);
        assert!(was.is_none());
    }
    pub fn finish<P: Proto>(self) -> Result<ProtoAll, ProtoBuildErr> {
        let typeless_proto_def = P::typeless_proto_def();
        let memory_bits: BitSet = typeless_proto_def
            .loc_kind_ext
            .iter()
            .filter(|(_, &loc_kind_ext)| loc_kind_ext == LocKindExt::MemInitialized)
            .map(|(&id, _)| id)
            .collect();

        let ready: BitSet = typeless_proto_def
            .loc_kind_ext
            .iter()
            .filter(|(_, loc_kind_ext)| loc_kind_ext.is_mem())
            .map(|(&id, _)| id)
            .collect();

        let (id_2_type_id, type_id_2_info) = {
            let mut id_2_type_id: HashMap<LocId, TypeId> = Default::default();
            let mut type_id_2_info: HashMap<TypeId, Arc<TypeInfo>> = Default::default();
            for id in typeless_proto_def.loc_kind_ext.keys().copied() {
                let type_info = P::loc_type(id);
                let type_id = type_info.type_id;
                id_2_type_id.entry(id).or_insert(type_id);
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
            .loc_kind_ext
            .iter()
            .filter_map(|(&id, loc_kind_ext)| {
                let role = match loc_kind_ext {
                    LocKindExt::PortPutter => PortRole::Putter,
                    LocKindExt::PortGetter => PortRole::Getter,
                    _ => return None,
                };
                let info = PortInfo {
                    role,
                    type_id: *id_2_type_id.get(&id).unwrap(),
                };
                Some((id, info))
            })
            .collect();

        // fn fill_memory(loc_id: LocId, promise: MemFillPromise) -> MemFillPromiseFulfilled;
        // fn loc_kind_ext(loc_id: LocId) -> LocKindExt;
        // fn loc_type(loc_id: LocId) -> &'static TypeInfo;

        /* here we construct a proto according to the specification.
        Failure may be a result of:
        1. The type for a used LocId is not derivable.
        2. A LocId is associated with a type which is not provided in the TypeInfo map.
        */
        let spaces = typeless_proto_def
            .loc_kind_ext
            .iter()
            .map(|(id, loc_kind_ext)| match loc_kind_ext {
                LocKindExt::PortPutter => Space::PoPu(PoPuSpace::new({ id_2_info(id).clone() })),
                LocKindExt::PortGetter => Space::PoGe(PoGeSpace::new()),
                LocKindExt::MemInitialized => Space::Memo({
                    let ptr: *mut u8 = *self.init_mems.get(id).unwrap();
                    let type_info = id_2_info(id).clone();
                    MemoSpace::new(ptr, type_info)
                }),
                LocKindExt::MemUninitialized => Space::Memo({
                    let ptr: *mut u8 = std::ptr::null_mut();
                    let type_info = id_2_info(id).clone();
                    MemoSpace::new(ptr, type_info)
                }),
            })
            .collect();

        let rules = Self::build_rules::<P>()?;

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

    fn build_rules<P: Proto>() -> Result<Vec<RunRule>, ProtoBuildErr> {
        let typeless_proto_def = P::typeless_proto_def();
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (_rule_id, rule_def) in typeless_proto_def.structure.rules.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut guard_full = BitSet::default();
            let mut actions = vec![];
            let mut assign_vals = BitSet::default();
            let mut assign_mask = BitSet::default();

            for action_def in rule_def.actions.iter() {
                let mut mg = vec![];
                let mut pg = vec![];

                let p = action_def.putter;
                let p_type = typeless_proto_def
                    .loc_kind_ext
                    .get(&p)
                    .ok_or(UnknownType { loc_id: p })?;
                if !p_type.can_put() {
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
                    let g_type = typeless_proto_def
                        .loc_kind_ext
                        .get(&g)
                        .ok_or(UnknownType { loc_id: g })?;
                    if !g_type.can_get() {
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
            let c = typeless_proto_def
                .loc_kind_ext
                .keys()
                .copied()
                .max()
                .unwrap_or(0);
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
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref LAZY: TypelessProtoDef = TypelessProtoDef {
                structure: ProtoDef{ 
                    rules: vec![
                        rule![Formula::True; 0=>1],
                        rule![Formula::True; 0=>1],
                    ]
                },
                loc_kind_ext: map! {
                    0 => LocKindExt::PortPutter,
                    1 => LocKindExt::PortGetter,
                },
            };
        }
        &LAZY
    }
    fn fill_memory(_: LocId, _: MemFillPromise) -> MemFillPromiseFulfilled {
        unimplemented!()
    }
    fn loc_type(loc_id: LocId) -> TypeInfo {
        TypeInfo::new::<u32>()
    }
}

#[test]
fn instantiate_fifo3() {
    let _x = IdkProto::instantiate();
    println!("DONE");
}
