use super::*;
use crate::proto::traits::FuncDefPromise;
use crate::proto::traits::MemFillPromise;

#[derive(Debug, Clone)]
pub struct BehaviourDef {
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

#[derive(Debug, Clone, PartialEq)]
pub enum Formula {
    True,
    And(Vec<Formula>),
    Or(Vec<Formula>),
    None(Vec<Formula>),
    ValueEq(Term, Term),
    MemIsNull(LocId),
    TermVal(Term),
    FuncDeclaration { name: &'static str, args: Vec<Term> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Boolean(Box<Formula>),
    Value(LocId),
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
    FunctionUndefined {
        name: &'static str,
    },
    FunctionUsedWithWrongArity {
        name: &'static str,
        used_arity: usize,
    },
}

pub struct FuncDef {
    pub(crate) ret_info: Arc<TypeInfo>,
    pub(crate) param_info: Vec<Arc<TypeInfo>>,
    pub(crate) fnptr: fn(), // bogus type
}

pub struct ProtoBuilder {
    mem_storage: Storage,
    init_mems: HashMap<LocId, *mut u8>,
    func_defs: HashMap<&'static str, FuncDef>,
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
    pub behaviour: BehaviourDef,
    pub loc_kinds: HashMap<LocId, LocKind>,
}

impl ProtoBuilder {
    pub fn new() -> Self {
        Self {
            mem_storage: Default::default(),
            func_defs: Default::default(),
            init_mems: Default::default(),
        }
    }
    pub(crate) fn define_func(&mut self, name: &'static str, func_def: FuncDef) {
        assert!(self.func_defs.insert(name, func_def).is_none())
    }
    pub(crate) fn define_init_memory<T: 'static>(&mut self, id: LocId, t: T) {
        let ptr = self.mem_storage.move_value_in(t);
        let was = self.init_mems.insert(id, ptr);
        assert!(was.is_none());
    }
    pub fn finish<P: Proto>(mut self) -> Result<ProtoAll, ProtoBuildErr> {
        use ProtoBuildErr::*;
        let typeless_proto_def = P::typeless_proto_def();
        let max_loc_id = Self::max_loc_id(typeless_proto_def);
        let mut memory_bits: BitSet = typeless_proto_def
            .loc_kinds
            .iter()
            .filter(|(_, &loc_kinds)| loc_kinds == LocKind::MemInitialized)
            .map(|(&id, _)| id)
            .collect();
        memory_bits.pad_trailing_zeroes_to_capacity(max_loc_id);

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

        let mut spaces = (0..=max_loc_id)
            .map(|id| {
                Ok(if let Some(k) = typeless_proto_def.loc_kinds.get(&id) {
                    match k {
                        LocKind::PortPutter => {
                            Space::PoPu(PoPuSpace::new({ id_2_info(&id).clone() }))
                        }
                        LocKind::PortGetter => Space::PoGe(PoGeSpace::new()),
                        LocKind::MemInitialized => Space::Memo({
                            // TODO promise
                            let type_info = id_2_info(&id).clone();
                            let promise = MemFillPromise {
                                type_id_expected: type_info.type_id,
                                loc_id: id,
                                builder: &mut self,
                            };
                            P::fill_memory(id, promise);
                            if let Some(ptr) = self.init_mems.get(&id) {
                                MemoSpace::new(*ptr, type_info)
                            } else {
                                return Err(MemoryFillPromiseBroken { loc_id: id });
                            }
                        }),
                        LocKind::MemUninitialized => Space::Memo({
                            let ptr: *mut u8 = std::ptr::null_mut();
                            let type_info = id_2_info(&id).clone();
                            MemoSpace::new(ptr, type_info)
                        }),
                    }
                } else {
                    Space::Unused
                })
            })
            .collect::<Result<Vec<Space>, ProtoBuildErr>>()?;

        let rules = self.build_rules::<P>(&id_2_type_id, &mut spaces)?;
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
        &mut self,
        id_2_type_id: &HashMap<LocId, TypeId>,
        spaces: &mut Vec<Space>,
    ) -> Result<Vec<RunRule>, ProtoBuildErr> {
        let typeless_proto_def = P::typeless_proto_def();
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (rule_id, rule_def) in typeless_proto_def.behaviour.rules.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut guard_full = BitSet::default();
            let mut actions = vec![];
            let mut assign_vals = BitSet::default();
            let mut assign_mask = BitSet::default();

            for (action_id, action_def) in rule_def.actions.iter().enumerate() {
                let mut mg = smallvec::smallvec![];
                let mut pg = smallvec::smallvec![];

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
                actions.push(RunAction { putter: p, mg, pg });
            }
            let c = Self::max_loc_id(typeless_proto_def);
            guard_ready.pad_trailing_zeroes_to_capacity(c);
            guard_full.pad_trailing_zeroes_to_capacity(c);
            assign_vals.pad_trailing_zeroes_to_capacity(c);
            assign_mask.pad_trailing_zeroes_to_capacity(c);

            self.define_all_funcs_in::<P>(&rule_def.guard);
            let (guard_pred, temp_mems) =
                Self::calc_guard(id_2_type_id, &rule_def.guard, &actions, spaces);
            rules.push(RunRule {
                guard_ready,
                guard_full,
                temp_mems,
                guard_pred,
                assign_vals,
                assign_mask,
                actions,
            });
        }
        Ok(rules)
    }

    fn define_all_funcs_in<P: Proto>(&mut self, f: &Formula) -> Result<(), ProtoBuildErr> {
        use Formula::*;
        let clos = |me: &mut Self, fs: &Vec<Formula>| {
            fs.iter()
                .map(|f| me.define_all_funcs_in::<P>(f))
                .collect::<Result<(), ProtoBuildErr>>()
        };
        let term = |me: &mut Self, t: &Term| match t {
            Term::Boolean(f) => me.define_all_funcs_in::<P>(&f),
            Term::Value(_) => Ok(()),
        };
        Ok(match f {
            True | MemIsNull(_) => (),
            And(fs) | Or(fs) | None(fs) => clos(self, fs)?,
            ValueEq(a, b) => {
                term(self, a)?;
                term(self, b)?;
            }
            TermVal(a) => term(self, a)?,
            FuncDeclaration { name, .. } => {
                let promise = FuncDefPromise {
                    builder: self,
                    name,
                };
                if P::def_func(name, promise).is_none() {
                    return Err(ProtoBuildErr::FunctionUndefined { name });
                }
            }
        })
    }

    fn calc_guard(
        _id_2_type_id: &HashMap<LocId, TypeId>,
        data_constraint: &Formula,
        _actions: &[RunAction],
        spaces: &mut Vec<Space>,
    ) -> (Formula, Vec<TempMemRunnable>) {
        let f = data_constraint.clone();
        let t = vec![];
        (f, t)
    }

    fn runnify_formulae<P: Proto>(
        &self,
        f: &[Formula],
        spaces: &mut Vec<Space>,
        temp_mems: &mut Vec<TempRuleFunc>,
    ) -> Result<Vec<Formula>, ProtoBuildErr> {
        f.iter()
            .map(|e| self.runnify_formula::<P>(e, spaces, temp_mems))
            .collect()
    }

    fn runnify_formula<P: Proto>(
        &self,
        f: &Formula,
        spaces: &mut Vec<Space>,
        temp_mems: &mut Vec<TempRuleFunc>,
    ) -> Result<Formula, ProtoBuildErr> {
        use Formula::*;
        use ProtoBuildErr::*;
        let mut r = |fs| self.runnify_formulae::<P>(fs, spaces, temp_mems);
        Ok(match f {
            True | ValueEq(_, _) | MemIsNull(_) => f.clone(), // stop condtion
            TermVal(t) => match t {
                Term::Boolean(f) => TermVal(Term::Boolean(Box::new(
                    self.runnify_formula::<P>(f, spaces, temp_mems)?,
                ))),
                Term::Value(_) => f.clone(),
            },
            And(fs) => And(r(fs)?),
            Or(fs) => Or(r(fs)?),
            None(fs) => None(r(fs)?),
            FuncDeclaration { name, args } => {
                // 1 ensure the function has been defined by user
                if let Some(func_def) = self.func_defs.get(name) {
                    if args.len() != func_def.param_info.len() {
                        return Err(FunctionUsedWithWrongArity {
                            name,
                            used_arity: args.len(),
                        });
                    }
                    let fixed_subterms: Vec<Term> = args
                        .iter()
                        .zip(func_def.param_info.iter())
                        .map(|(a, p)| {
                            Ok(match a {
                                Term::Boolean(f) => Term::Boolean(Box::new(
                                    self.runnify_formula::<P>(f, spaces, temp_mems)?,
                                )),
                                Term::Value(loc_id) => Term::Value(*loc_id),
                            })
                        })
                        .collect::<Result<Vec<Term>, ProtoBuildErr>>()?;
                    unimplemented!()
                } else {
                    return Err(FunctionUndefined { name });
                }
                // pub struct FuncDef {
                //     ret_info: Arc<TypeInfo>,
                //     param_info: Vec<Arc<TypeInfo>>,
                //     fnptr: fn(), // bogus type
                // }
            }
        })
    }
}

trait TempAllocator {
    fn new_temp(&mut self, type_info: &Arc<TypeInfo>) -> LocId;
}
impl TempAllocator for Vec<Space> {
    fn new_temp(&mut self, type_info: &Arc<TypeInfo>) -> LocId {
        let t = Space::Temp(TempSpace::new(type_info.clone()));
        for (i, s) in self.iter_mut().enumerate() {
            match s {
                Space::Unused => {
                    *s = t;
                    return i;
                }
                _ => (),
            }
        }
        self.push(t);
        self.len() - 1
    }
}
