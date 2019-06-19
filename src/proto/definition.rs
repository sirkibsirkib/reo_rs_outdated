use super::*;

/// Defines an abstract protocol action. Describes one data-movement of 1 putter to N getters
#[derive(derive_new::new, Debug)]
pub struct ActionDef {
    pub putter: LocId,
    pub getters: &'static [LocId],
}

/// Defines an abstract rule comprised of some actions that fire atomically
#[derive(derive_new::new, Debug)]
pub struct RuleDef {
    pub guard_pred: GuardPred,
    pub actions: Vec<ActionDef>,
}
/// Defines the entirety of a protocol, describing the LocId space and types
#[derive(Debug)]
pub struct ProtoDef {
    pub port_info: Vec<PortInfo>,
    pub mem_types: Vec<TypeId>,
    pub rule_defs: Vec<RuleDef>,
    pub type_info: HashMap<TypeId, Arc<TypeInfo>>,
}
impl ProtoDef {
    pub fn build(&self) -> Result<ProtoAll, ProtoBuildErr> {
        let rules = self.build_rules()?;
        for (i, r) in rules.iter().enumerate() {
            println!("{} => {:?}", i, r);
        }
        let (mem_data, spaces, free_mems, ready, memory_bits) = self.build_core();

        let r = ProtoR {
            mem_data,
            spaces,
            rules,
        };
        let unclaimed_ports = self.port_info.iter().copied().enumerate().collect();
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
    fn build_core(
        &self,
    ) -> (
        Vec<u8>, // buffer
        Vec<Space>,
        // Vec<MemoSpace>,
        // Vec<PoPuSpace>,
        HashMap<TypeId, Vec<*mut u8>>,
        BitSet,
        BitSet,
    ) {
        let mut memory_bits = BitSet::default();
        let mem_id_start = self.port_info.len();
        let mut capacity = 0;
        let mut offsets_n_typeids = vec![];
        let mut ready = BitSet::default();
        let mut free_mems = HashMap::default();
        for (mem_id, info) in self.mem_types.iter().enumerate().map(|(i, id)| {
            (
                i + mem_id_start,
                self.type_info.get(id).expect("unknown type"),
            )
        }) {
            memory_bits.set_to(mem_id, false);
            ready.set_to(mem_id, true);
            let rem = capacity % info.align.max(1);
            if rem > 0 {
                capacity += info.align - rem;
            }
            offsets_n_typeids.push((capacity, info.type_id));
            capacity += info.bytes.max(1); // make pointers unique even with 0-byte data
        }

        // meta-offset used to ensure the start of the vec alsigns to 64-bits (covers all cases)
        // almost always unnecessary
        let mut buf: Vec<u8> = Vec::with_capacity(capacity + 8);
        let mut meta_offset: isize = 0;
        while (unsafe { buf.as_ptr().offset(meta_offset) }) as usize % 8 != 0 {
            meta_offset += 1;
        }
        unsafe {
            buf.set_len(capacity);
        }
        let port_iter = self.port_info.iter().map(|port_info| {
            let type_info = self
                .type_info
                .get(&port_info.type_id)
                .expect("Missed a type")
                .clone();
            match port_info.role {
                PortRole::Putter => Space::PoPu(PoPuSpace::new(type_info)),
                PortRole::Getter => Space::PoGe(PoGeSpace::new()),
            }
        });
        let memo_iter = offsets_n_typeids
            .into_iter()
            .map(|(offset, type_id)| unsafe {
                let ptr: *mut u8 = buf.as_mut_ptr().offset(offset as isize + meta_offset);
                free_mems.entry(type_id).or_insert(vec![]).push(ptr);
                let type_info = self.type_info.get(&type_id).expect("Missed a type").clone();
                Space::Memo(MemoSpace::new(ptr, type_info))
            });
        let spaces = port_iter.chain(memo_iter).collect();
        let c = self.loc_id_range().end;
        ready.pad_trailing_zeroes_to_capacity(c);
        memory_bits.pad_trailing_zeroes_to_capacity(c);
        (
            buf,
            spaces,
            // memo_spaces,
            // po_pu_spaces,
            free_mems,
            ready,
            memory_bits,
        )
    }
    fn build_rules(&self) -> Result<Vec<RunRule>, ProtoBuildErr> {
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (_rule_id, rule_def) in self.rule_defs.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut guard_full = BitSet::default();
            let mut actions = vec![];
            let mut assign_vals = BitSet::default();
            let mut assign_mask = BitSet::default();

            for action_def in rule_def.actions.iter() {
                let mut mg = vec![];
                let mut pg = vec![];

                let p = action_def.putter;
                let putter_type = self.get_putter_type(p).ok_or(LocCannotPut { loc_id: p })?;
                if guard_ready.test(p) {
                    return Err(SynchronousFiring { loc_id: p });
                }
                if putter_type == LocType::Mem {
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
                    let getter_type = self.get_getter_type(g).ok_or(LocCannotGet { loc_id: g })?;
                    match getter_type {
                        LocType::Port => &mut pg,
                        LocType::Mem => &mut mg,
                    }
                    .push(g);
                    if getter_type == LocType::Mem {
                        guard_full.set_to(g, false); // getter must be empty!
                        assign_vals.set_to(g, true); // getter becomes full!
                        assign_mask.set_to(g, true); // getter memory fullness changes!
                    }
                    let was_set = guard_ready.set_to(g, true);
                    if was_set {
                        // oh no! this getter was already involved in the firing
                        if g == p && LocType::Mem == putter_type {
                            // nevermind. its OK for memory to put and get to itself
                            guard_full.set_to(g, true); // getter must be full (because its also the putter)!
                            assign_mask.set_to(g, false); // getter memory fullness DOES NOT change (Full -> Full)!
                            assign_vals.set_to(g, false);
                        } else {
                            return Err(SynchronousFiring { loc_id: g });
                        }
                    }
                }
                actions.push(match putter_type {
                    LocType::Port => Action::PortPut { putter: p, mg, pg },
                    LocType::Mem => Action::MemPut { putter: p, mg, pg },
                });
            }
            let c = self.loc_id_range().end;
            guard_ready.pad_trailing_zeroes_to_capacity(c);
            guard_full.pad_trailing_zeroes_to_capacity(c);
            assign_vals.pad_trailing_zeroes_to_capacity(c);
            assign_mask.pad_trailing_zeroes_to_capacity(c);
            rules.push(RunRule {
                guard_ready,
                guard_full,
                guard_pred: rule_def.guard_pred.clone(),
                assign_vals,
                assign_mask,
                actions,
            });
        }
        Ok(rules)
    }
    fn get_putter_type(&self, id: LocId) -> Option<LocType> {
        if self.loc_is_po_pu(id) {
            Some(LocType::Port)
        } else if self.loc_is_mem(id) {
            Some(LocType::Mem)
        } else {
            None
        }
    }
    fn get_getter_type(&self, id: LocId) -> Option<LocType> {
        if self.loc_is_po_ge(id) {
            Some(LocType::Port)
        } else if self.loc_is_mem(id) {
            Some(LocType::Mem)
        } else {
            None
        }
    }
    fn loc_is_po_pu(&self, id: LocId) -> bool {
        self.port_info
            .get(id)
            .map(|x| x.role == PortRole::Putter)
            .unwrap_or(false)
    }
    fn loc_is_po_ge(&self, id: LocId) -> bool {
        self.port_info
            .get(id)
            .map(|x| x.role == PortRole::Getter)
            .unwrap_or(false)
    }
    fn loc_is_mem(&self, id: LocId) -> bool {
        let r = self.loc_id_range().end;
        let l = r - self.mem_types.len();
        l <= id && id < r
    }
    pub fn loc_id_range(&self) -> Range<LocId> {
        let r = self.port_info.len() + self.mem_types.len();
        0..r
    }
    pub fn validate(&self) -> ProtoDefValidationResult {
        self.check_data_types_match()?;
        self.check_rule_guards()
    }
    fn check_data_types_match(&self) -> ProtoDefValidationResult {
        use ProtoDefValidationError::*;
        for rule_def in self.rule_defs.iter() {
            for action in rule_def.actions.iter() {
                let putter_tid = self.type_for(action.putter);
                for &g in action.getters.iter() {
                    if putter_tid != self.type_for(g) {
                        return Err(ActionTypeMismatch);
                    }
                }
            }
        }
        Ok(())
    }
    fn type_for(&self, id: LocId) -> Option<TypeId> {
        self.port_info
            .get(id)
            .map(|x| x.type_id)
            .or_else(|| self.mem_types.get(id - self.port_info.len()).copied())
    }

    fn check_rule_guards(&self) -> ProtoDefValidationResult {
        let mut putters = HashSet::<LocId>::default();
        for rule_def in self.rule_defs.iter() {
            putters.extend(rule_def.actions.iter().map(|a| a.putter));
            self.check_guard_pred(&rule_def.guard_pred, &putters)?;
            putters.clear();
        }
        Ok(())
    }
    fn check_guard_pred(
        &self,
        pred: &GuardPred,
        putters: &HashSet<LocId>,
    ) -> ProtoDefValidationResult {
        use GuardPred::*;
        use ProtoDefValidationError::*;
        match pred {
            True => (),
            None(x) | And(x) | Or(x) => {
                for a in x.iter() {
                    self.check_guard_pred(a, putters)?;
                }
            }
            Eq(a, b) => {
                if !putters.contains(a) || !putters.contains(b) {
                    return Err(GuardReasonsOverAbsentData);
                }
                if self.type_for(*a) != self.type_for(*b) {
                    return Err(GuardEqTypeMismatch);
                }
            }
        };
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum LocType {
    Port,
    Mem,
}

#[derive(Debug, Copy, Clone)]
pub enum ProtoBuildErr {
    SynchronousFiring { loc_id: LocId },
    LocCannotGet { loc_id: LocId },
    LocCannotPut { loc_id: LocId },
}

pub type ProtoDefValidationResult = Result<(), ProtoDefValidationError>;

#[derive(Debug, Copy, Clone)]
pub enum ProtoDefValidationError {
    GuardReasonsOverAbsentData,
    ActionOnNonexistantId,
    GuardEqTypeMismatch,
    ActionTypeMismatch,
}
