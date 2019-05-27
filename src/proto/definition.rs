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
    pub po_pu_infos: Vec<TypeInfo>,
    pub po_ge_types: Vec<TypeId>,
    pub mem_infos: Vec<TypeInfo>,
    pub rule_defs: Vec<RuleDef>,
}


#[derive(Debug)]
pub struct Rbpa {
    rules: Vec<RbpaRule>
}
#[derive(Debug)]
pub struct RbpaRule {
    port: Option<LocId>,
    guard: HashMap<LocId, Option<bool>>,
    assign: HashMap<LocId, Option<bool>>,
}

#[derive(Debug, Copy, Clone)]
pub enum RbpaBuildErr {
    SynchronousFiring { loc_ids: [LocId; 2], rule_id: RuleId },
}

#[derive(Debug, Copy, Clone)]
pub enum ProtoBuildErr {
    SynchronousFiring { loc_id: LocId },
    LocCannotGet { loc_id: LocId },
    LocCannotPut { loc_id: LocId },
}
impl ProtoDef {
    pub fn new_rbpa(&self, port_set: &HashSet<LocId>) -> Result<Rbpa, RbpaBuildErr> {
        let mut rules = vec![];
        for (rule_id, rule_def) in self.rule_defs.iter().enumerate() {
            use RbpaBuildErr::*;
            let mut guard = HashMap::default();
            let mut assign = HashMap::default();
            let mut port: Option<LocId> = None;
            let mut clos = |id: LocId| {
                if port_set.contains(&id) {
                    if let Some(was) = port.replace(id) {
                        return Some(SynchronousFiring { loc_ids: [was, id], rule_id })
                    }
                }
                None
            };
            for action in rule_def.actions.iter() {
                if let Some(err) = clos(action.putter) {
                    return Err(err)
                }
                guard.insert(action.putter, Some(false));
                assign.insert(action.putter, Some(true));
                for &getter in action.getters.iter() {
                    if let Some(err) = clos(getter) {
                        return Err(err)
                    }
                    guard.entry(getter).or_insert(Some(true));
                    assign.insert(getter, Some(true));
                }
            }
            rules.push(RbpaRule {
                port,
                guard,
                assign
            });
        }
        Ok(Rbpa { rules })
    }
    pub fn build(&self) -> Result<ProtoAll, ProtoBuildErr> {
        let rules = self.build_rules()?;
        let (mem_data, me_pu, po_pu, free_mems, ready) = self.build_core();
        let po_ge = (0..self.po_ge_types.len())
            .map(|_| PoGeSpace::new())
            .collect();
        let r = ProtoR {
            mem_data,
            me_pu,
            po_pu,
            po_ge,
        };
        let unclaimed_ports = self
            .po_pu_infos
            .iter()
            .enumerate()
            .map(|(i, type_info)| {
                let id = i;
                let type_id = type_info.type_id;
                let upi = UnclaimedPortInfo {
                    putter: true,
                    type_id,
                };
                (id, upi)
            })
            .chain(self.po_ge_types.iter().enumerate().map(|(i, &type_id)| {
                let id = self.po_pu_infos.len() + i;
                let upi = UnclaimedPortInfo {
                    putter: false,
                    type_id,
                };
                (id, upi)
            }))
            .collect();
        let w = Mutex::new(ProtoW {
            rules,
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
        Vec<MemoSpace>,
        Vec<PoPuSpace>,
        HashMap<TypeId, Vec<*mut u8>>,
        BitSet,
    ) {
        let mem_get_id_start =
            self.mem_infos.len() + self.po_pu_infos.len() + self.po_ge_types.len();
        let mut capacity = 0;
        let mut offsets_n_typeids = vec![];
        let mut mem_type_info: HashMap<TypeId, Arc<TypeInfo>> = self
            .po_pu_infos
            .iter()
            .map(|&info| (info.type_id, Arc::new(info)))
            .collect();
        let mut ready = BitSet::default();
        let mut free_mems = HashMap::default();
        for (mem_id, info) in self.mem_infos.iter().enumerate() {
            ready.set(mem_id + mem_get_id_start); // set GETTER
            let rem = capacity % info.align.max(1);
            if rem > 0 {
                capacity += info.align - rem;
            }
            offsets_n_typeids.push((capacity, info.type_id));
            mem_type_info
                .entry(info.type_id)
                .or_insert_with(|| Arc::new(*info));
            capacity += info.bytes.max(1); // make pointers unique even with 0-byte data
        }
        // println!("CAP IS {:?}", capacity);

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
        let memo_spaces = offsets_n_typeids
            .into_iter()
            .map(|(offset, type_id)| unsafe {
                let ptr: *mut u8 = buf.as_mut_ptr().offset(offset as isize + meta_offset);
                free_mems.entry(type_id).or_insert(vec![]).push(ptr);
                let type_info = mem_type_info.get(&type_id).expect("Missed a type").clone();
                MemoSpace::new(ptr, type_info)
            })
            .collect();
        let po_pu_spaces = self
            .po_pu_infos
            .iter()
            .map(|info| {
                let info = mem_type_info
                    .get(&info.type_id)
                    .expect("Missed a type")
                    .clone();
                PoPuSpace::new(info)
            })
            .collect();
        (buf, memo_spaces, po_pu_spaces, free_mems, ready)
    }
    fn build_rules(&self) -> Result<Vec<RunRule>, ProtoBuildErr> {
        use ProtoBuildErr::*;
        let mut rules = vec![];
        for (_rule_id, rule_def) in self.rule_defs.iter().enumerate() {
            let mut guard_ready = BitSet::default();
            let mut actions = vec![];
            // let mut seen = HashSet::<LocId>::default();
            for action_def in rule_def.actions.iter() {
                let mut mg = vec![];
                let mut pg = vec![];
                let p = action_def.putter;
                if let Some(g) = self.mem_getter_id(p) {
                    if guard_ready.test(g) {
                        // mem is getter in one action and putter in another
                        return Err(SynchronousFiring { loc_id: p });
                    }
                }
                for &g in action_def.getters.iter() {
                    if self.loc_is_po_ge(g) {
                        pg.push(g);
                        if guard_ready.set_to(g, true) {
                            return Err(SynchronousFiring { loc_id: g });
                        }
                    } else if self.loc_is_mem(g) {
                        mg.push(g);
                        if guard_ready.set_to(self.mem_getter_id(g).expect("BAD MEM ID"), true) {
                            return Err(SynchronousFiring { loc_id: g });
                        }
                    } else {
                        return Err(LocCannotGet { loc_id: g });
                    }
                }
                if guard_ready.set_to(p, true) {
                    return Err(SynchronousFiring { loc_id: p });
                }
                // seen.insert(action_def.putter);
                if self.loc_is_po_pu(p) {
                    actions.push(Action::PortPut { putter: p, mg, pg });
                } else if self.loc_is_mem(p) {
                    actions.push(Action::MemPut { putter: p, mg, pg });
                } else {
                    return Err(LocCannotPut { loc_id: p });
                }
            }
            rules.push(RunRule {
                guard_ready,
                guard_pred: rule_def.guard_pred.clone(),
                actions,
            });
        }
        Ok(rules)
    }
    fn mem_getter_id(&self, id: LocId) -> Option<LocId> {
        if self.loc_is_mem(id) {
            Some(id + self.mem_infos.len())
        } else {
            None
        }
    }
    fn loc_is_po_pu(&self, id: LocId) -> bool {
        id < self.po_pu_infos.len()
    }
    fn loc_can_put(&self, id: LocId) -> bool {
        self.loc_is_po_pu(id) || self.loc_is_mem(id)
    }
    fn loc_can_get(&self, id: LocId) -> bool {
        self.loc_is_po_ge(id) || self.loc_is_mem(id)
    }
    fn loc_is_po_ge(&self, id: LocId) -> bool {
        let r = self.po_pu_infos.len() + self.po_ge_types.len();
        self.po_pu_infos.len() <= id && id < r
    }
    fn loc_is_mem(&self, id: LocId) -> bool {
        let l = self.po_pu_infos.len() + self.po_ge_types.len();
        let r = self.po_pu_infos.len() + self.po_ge_types.len() + self.mem_infos.len();
        l <= id && id < r
    }
    fn validate(&self) -> ProtoDefValidationResult {
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
        let l1 = self.po_pu_infos.len();
        let l2 = self.po_ge_types.len();
        self.po_pu_infos
            .get(id)
            .map(TypeInfo::get_tid)
            .or(self.po_ge_types.get(id - l1).copied())
            .or(self.mem_infos.get(id - l1 - l2).map(TypeInfo::get_tid))
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

type ProtoDefValidationResult = Result<(), ProtoDefValidationError>;

#[derive(Debug, Copy, Clone)]
enum ProtoDefValidationError {
    GuardReasonsOverAbsentData,
    ActionOnNonexistantId,
    GuardEqTypeMismatch,
    ActionTypeMismatch,
}