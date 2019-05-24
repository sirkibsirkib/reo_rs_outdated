use super::*;

#[derive(Debug, Copy, Clone)]
pub enum PortGroupError {
    EmptyGroup,
    MemId(LocId),
    SynchronousWithRule(RuleId),
}
#[derive(Default)]
pub struct PortGroupBuilder {
    core: Option<(LocId, Arc<ProtoAll>)>,
    members: BitSet,
}

pub struct PortGroup {
    p: Arc<ProtoAll>,
    leader: LocId,
    disambiguation: HashMap<RuleId, LocId>,
}
impl PortGroup {
    /// block until the protocol is in this state
    unsafe fn await_state(&self, state_pred: BitSet) {
        // TODO check the given state pred is OK? maybe unnecessary since function is internal
        {
            let w = self.p.w.lock();
            if w.active.ready.is_superset(&state_pred) {
                return; // already in the desired state
            }
        } // release lock
        let space = self.p.r.get_space(self.leader);
        match space {
            SpaceRef::PoPu(po_pu_space) => po_pu_space.dropbox.recv_nothing(),
            SpaceRef::PoGe(po_ge_space) => po_ge_space.dropbox.recv_nothing(),
            _ => panic!("BAD ID"),
        }
        // I received a notification that the state is ready!
    }
    unsafe fn new(p: &Arc<ProtoAll>, id_set: &BitSet) -> Result<PortGroup, PortGroupError> {
        use PortGroupError::*;
        let mut w = p.w.lock();
        // 1. check all loc_ids correspond with ports (not memory cells)
        for id in id_set.iter_sparse() {
            if !p.r.loc_is_port(id) {
                return Err(MemId(id));
            }
        }
        // 1. check that NO rule contains multiple ports in the set
        for (rule_id, rule) in w.rules.iter().enumerate() {
            if rule.guard_ready.iter_and(id_set).count() > 1 {
                return Err(SynchronousWithRule(rule_id));
            }
        }
        match id_set.iter_sparse().next() {
            Some(leader) => {
                let mut disambiguation = HashMap::new();
                // 2. change occurrences of any port IDs in the set to leader
                for (rule_id, rule) in w.rules.iter_mut().enumerate() {
                    if let Some(specific_port) = rule.guard_ready.iter_and(id_set).next() {
                        disambiguation.insert(rule_id, specific_port);
                        rule.guard_ready.set_to(specific_port, false);
                        rule.guard_ready.set(leader);
                    }
                }
                Ok(PortGroup {
                    p: p.clone(),
                    leader,
                    disambiguation,
                })
            }
            None => Err(EmptyGroup),
        }
    }
    pub fn ready_wait_determine_commit(&self) -> LocId {
        let space = self.p.r.get_space(self.leader);
        {
            let mut w = self.p.w.lock();
            w.ready_tentative.set(self.leader);
            w.active.ready.set(self.leader);
        }
        let rule_id = match space {
            SpaceRef::PoPu(po_pu_space) => po_pu_space.dropbox.recv(),
            SpaceRef::PoGe(po_ge_space) => po_ge_space.dropbox.recv(),
            _ => panic!("BAD ID"),
        };
        *self.disambiguation.get(&rule_id).expect("SHOULD BE OK")
    }
}
impl Drop for PortGroup {
    // TODO ensure you can't change leaders or something whacky
    fn drop(&mut self) {
        let mut w = self.p.w.lock();
        for (rule_id, rule) in w.rules.iter_mut().enumerate() {
            if let Some(&specific_port) = self.disambiguation.get(&rule_id) {
                if self.leader != specific_port {
                    rule.guard_ready.set_to(self.leader, false);
                    rule.guard_ready.set(specific_port);
                }
            }
        }
    }
}
