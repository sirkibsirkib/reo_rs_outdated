use crate::bitset::BitSet;
use crate::proto::{PortInfo, ProtoAll, ProtoW, Space};
use crate::LocId;
use crossbeam::channel::Select;
use hashbrown::HashMap;
use itertools::izip;
use parking_lot::MutexGuard;
use std::sync::Arc;

pub struct PortGroup {
    proto: Arc<ProtoAll>,
    members: BitSet,
    member_info: HashMap<LocId, PortInfo>,
    members_indexed: Vec<LocId>,
}

pub enum PortGroupError {
    PortAlreadyClaimed(LocId),
}
impl PortGroup {
    pub fn new(proto: &Arc<ProtoAll>, members: BitSet) -> Result<Self, PortGroupError> {
        use PortGroupError::*;
        let proto = proto.clone();
        let mut w = proto.w.lock();
        let mut member_info = HashMap::default();
        for id in members.iter_sparse() {
            match w.unclaimed_ports.remove(&id) {
                Some(info) => member_info.insert(id, info),
                None => {
                    for (id, info) in member_info.into_iter() {
                        w.unclaimed_ports.insert(id, info);
                    }
                    return Err(PortAlreadyClaimed(id));
                }
            };
        }
        drop(w);
        Ok(Self {
            proto,
            member_info,
            members_indexed: members.iter_sparse().collect(),
            members,
        })
    }

    pub fn deliberate(&mut self) -> (LocId, LockedProto) {
        // step 1: prepare for callback (does not require lock)
        let mut sel = Select::new();
        for (expected_index, &id) in self.members_indexed.iter().enumerate() {
            let r = match self.proto.r.get_space(id) {
                Some(Space::PoPu(space)) => &space.dropbox.r,
                Some(Space::PoGe(space)) => &space.dropbox.r,
                _ => unreachable!(),
            };
            let index = sel.recv(r);
            assert_eq!(index, expected_index);
        }

        // step 2: lock proto and flag readiness
        let mut w = self.proto.w.lock();
        let ProtoW {
            ready_tentative,
            active,
            ..
        } = &mut w as &mut ProtoW;
        for (tenta, ready, &members) in izip!(
            ready_tentative.data.iter_mut(),
            active.ready.data.iter_mut(),
            self.members.data.iter()
        ) {
            *tenta |= members;
            *ready |= members;
        }
        drop(w);

        // step 3: await callback
        let oper = sel.select();
        let id: LocId = *self
            .members_indexed
            .get(oper.index())
            .expect("UNEXPECTED INDEX");

        let w = self.proto.w.lock();
        (
            id,
            LockedProto {
                w,
                members: &self.members,
            },
        )
    }
}
impl Drop for PortGroup {
    fn drop(&mut self) {
        let mut w = self.proto.w.lock();
        for (&id, &info) in self.member_info.iter() {
            w.unclaimed_ports.insert(id, info);
        }
    }
}

pub struct LockedProto<'a> {
    w: MutexGuard<'a, ProtoW>,
    members: &'a BitSet,
}
impl LockedProto<'_> {
    pub fn get_memory_bits(&self) -> &BitSet {
        &self.w.memory_bits
    }
}
impl Drop for LockedProto<'_> {
    fn drop(&mut self) {
        let ProtoW {
            ready_tentative,
            active,
            ..
        } = &mut self.w as &mut ProtoW;
        for (tenta, ready, &members) in izip!(
            ready_tentative.data.iter_mut(),
            active.ready.data.iter_mut(),
            self.members.data.iter()
        ) {
            *tenta |= members;
            *ready |= members;
        }
    }
}
