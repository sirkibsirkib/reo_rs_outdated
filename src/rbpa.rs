use crate::{proto::nu_def::ProtoDef, LocId, RuleId};
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use std::fmt;

pub type StatePred = HashMap<LocId, bool>;

impl ProtoDef {
    pub fn new_rbpa(&self, port_set: &HashSet<LocId>) -> Result<Rbpa, RbpaBuildErr> {
        let mut rules = vec![];
        let port_ids = 0..self.port_info.len();
        for (rule_id, rule_def) in self.rule_defs.iter().enumerate() {
            use RbpaBuildErr::*;
            let mut guard = HashMap::default();
            let mut assign = HashMap::default();
            let mut port: Option<LocId> = None;
            let mut clos = |id: LocId| {
                if port_set.contains(&id) {
                    if let Some(was) = port.replace(id) {
                        return Some(SynchronousFiring {
                            loc_ids: [was, id],
                            rule_id,
                        });
                    }
                }
                None
            };
            for action in rule_def.actions.iter() {
                if let Some(err) = clos(action.putter) {
                    return Err(err);
                }
                if !port_ids.contains(&action.putter) {
                    guard.insert(action.putter, true);
                    assign.insert(action.putter, false);
                }
                for &getter in action.getters.iter() {
                    if let Some(err) = clos(getter) {
                        return Err(err);
                    }
                    if !port_ids.contains(&getter) {
                        guard.entry(getter).or_insert(false);
                        assign.insert(getter, true);
                    }
                }
            }
            let mut rule = RbpaRule {
                port,
                guard,
                assign,
            };
            rule.normalize();
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
        Ok(Rbpa { rules })
    }
}

#[derive(Debug)]
pub struct Rbpa {
    pub rules: Vec<RbpaRule>,
}
impl Rbpa {
    pub fn normalize(&mut self) {
        let mut buf = vec![];
        while let Some((i, _)) = self
            .rules
            .iter()
            .enumerate()
            .filter(|r| r.1.port.is_none())
            .next()
        {
            let r1 = self.rules.swap_remove(i);
            for rid2 in 0..self.rules.len() {
                let r2 = &self.rules[rid2];
                if let Some(c) = r1.compose(r2) {
                    let mut did_fuse = false;
                    for r3 in self.rules.iter_mut() {
                        if let Some(fused) = r3.fuse(&c) {
                            // println!("({:?}) | ({:?}) = ({:?})", c, r3, &fused);
                            *r3 = fused;
                            did_fuse = true;
                        }
                    }
                    if !did_fuse {
                        buf.push(c);
                    }
                }
            }
            self.rules.append(&mut buf);
            // println!("now am: {:#?}", &self);
        }
        let mut rules = Vec::with_capacity(self.rules.len());
        std::mem::swap(&mut self.rules, &mut rules);
        for r in rules.drain(..) {
            if !self.rules.contains(&r) {
                self.rules.push(r);
            }
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct RbpaRule {
    port: Option<LocId>,
    guard: StatePred,
    assign: StatePred,
}

enum FuseCase {
    LeftSubsumes,
    RightSubsumes,
    PartitionAt(LocId),
    Identical,
}
impl RbpaRule {
    pub fn constrain_guard(&self, guard: &mut StatePred) -> Result<(), LocId> {
        for (&id, &b) in self.guard.iter() {
            let b2 = guard.entry(id).or_insert(b);
            if *b2 != b {
                return Err(id);
            }
        }
        Ok(())
    }
    fn normalize(&mut self) {
        let RbpaRule { guard, assign, .. } = self;
        assign.retain(|k, v| guard.get(k) != Some(v));
    }
    pub fn is_mutex_with(&self, other: &Self) -> bool {
        let ids = self.guard.keys().chain(other.guard.keys()).unique();
        for id in ids {
            match [self.guard.get(id), other.guard.get(id)] {
                [Some(a), Some(b)] if a != b => return true,
                _ => (),
            }
        }
        false
    }
    pub fn has_effect(&self) -> bool {
        !self.assign.is_empty()
    }
    pub fn get_guard(&self) -> &StatePred {
        &self.guard
    }
    pub fn get_port(&self) -> &Option<LocId> {
        &self.port
    }
    pub fn get_assign(&self) -> &StatePred {
        &self.assign
    }
    fn fuse(&self, other: &Self) -> Option<Self> {
        if self.port != other.port {
            return None;
        }

        use FuseCase::*;
        let mut g_case = Identical;
        for id in self.guard.keys().chain(other.guard.keys()).unique() {
            match [self.guard.get(id), other.guard.get(id)] {
                [Some(_), None] => match g_case {
                    RightSubsumes | Identical => g_case = RightSubsumes,
                    _ => return None,
                },
                [None, Some(_)] => match g_case {
                    LeftSubsumes | Identical => g_case = LeftSubsumes,
                    _ => return None,
                },
                [Some(a), Some(b)] if a != b => match g_case {
                    Identical => g_case = PartitionAt(*id),
                    _ => return None,
                },
                [Some(_), Some(_)] => (),
                [None, None] => unreachable!(),
            }
        }

        for id in self.guard.keys().chain(other.guard.keys()).unique() {
            let left = self.assign.get(id).or_else(|| self.guard.get(id));
            let right = other.assign.get(id).or_else(|| other.guard.get(id));
            if left != right {
                return None;
            }
        }

        let guard = match g_case {
            Identical | LeftSubsumes => self.guard.clone(),
            RightSubsumes => other.guard.clone(),
            PartitionAt(id) => {
                let mut x = self.guard.clone();
                let _ = x.remove(&id);
                x
            }
        };

        let mut rule = RbpaRule {
            port: self.port,
            guard,
            assign: self.assign.clone(),
        };
        rule.normalize();
        Some(rule)
    }
    fn compose(&self, other: &Self) -> Option<Self> {
        // can compose if:
        // 1.

        assert!(self.port.is_none());
        let port = other.port;

        let mut guard = self.guard.clone();
        for (id, v1) in other.guard.iter() {
            match [self.guard.get(id), self.assign.get(id)] {
                [None, None] => {
                    // other imposes a new restriction
                    guard.insert(*id, *v1);
                }
                [Some(v2), None] | [_, Some(v2)] => {
                    if v2 != v1 {
                        // clash between output of r1 and input of r2
                        return None;
                    }
                }
            }
        }

        let mut assign = other.assign.clone();
        for (id, v1) in self.assign.iter() {
            match [other.guard.get(id), other.assign.get(id)] {
                [None, None] => {
                    // first rule propagates assignment
                    assign.insert(*id, *v1);
                }
                [Some(v2), None] | [_, Some(v2)] => {
                    if v2 != v1 {
                        // 2nd rule overshadows 1st
                    }
                }
            }
        }
        let mut rule = RbpaRule {
            port,
            guard,
            assign,
        };
        rule.normalize();
        Some(rule)
    }
}
impl fmt::Debug for RbpaRule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buf = vec!['.'; 5];
        for (&k, &v) in self.guard.iter() {
            if buf.len() <= k {
                buf.resize_with(k + 1, || '.');
            }
            buf[k] = if v { 'T' } else { 'F' };
        }
        for b in buf.drain(..) {
            write!(f, "{}", b)?;
        }
        buf.extend(&['.'; 5]);
        match self.port {
            Some(x) => write!(f, " ={}=> ", x),
            None => write!(f, " =.=> "),
        }?;
        for (&k, &v) in self.assign.iter() {
            if buf.len() <= k {
                buf.resize_with(k + 1, || '.');
            }
            buf[k] = if v { 'T' } else { 'F' };
        }
        for b in buf.drain(..) {
            write!(f, "{}", b)?;
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum RbpaBuildErr {
    SynchronousFiring {
        loc_ids: [LocId; 2],
        rule_id: RuleId,
    },
}
