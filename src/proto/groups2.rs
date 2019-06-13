use crate::proto::Getter;
use crate::ProtoHandle;
use crate::tokens::decimal::Decimal;
use crate::proto::Putter;
use crate::tokens::Grouped;
use crate::bitset::BitSet;
use crate::proto::{PortInfo, ProtoAll, ProtoW, Space};
use crate::LocId;
use crossbeam::channel::Select;
use hashbrown::HashMap;
use itertools::izip;
use parking_lot::MutexGuard;
use std::sync::Arc;

pub struct PortGroup {
    maybe_proto: Option<Arc<ProtoAll>>,
    members: BitSet,
    member_info: HashMap<LocId, PortInfo>,
    members_indexed: Vec<LocId>,
}

fn proto_handle_eq(a: &ProtoHandle, b: &ProtoHandle) -> bool {
    let cst = |x| x as &ProtoHandle as *const ProtoHandle;
    std::ptr::eq(cst(a), cst(b))
}


#[derive(Debug, Copy, Clone)]
pub struct DifferentProtoInstance;
pub enum PortGroupError {
    PortAlreadyClaimed(LocId),
}
impl PortGroup {
    pub fn new() -> Self {
        Self {
            maybe_proto: None,
            members: Default::default(),
            member_info: Default::default(),
            members_indexed: Default::default(),
        }
    }
    pub fn add_putter<D: Decimal,T>(&mut self, m: Putter<T>) -> Result<Grouped<D, Putter<T>>, DifferentProtoInstance> {
        let p = self.maybe_proto.get_or_insert_with(|| m.c.p.clone());
        if !proto_handle_eq(p, &m.c.p) {
            return Err(DifferentProtoInstance)
        }
        Ok(m.safe_wrap())
    }
    pub fn add_getter<D: Decimal,T>(&mut self, m: Getter<T>) -> Result<Grouped<D, Getter<T>>, DifferentProtoInstance> {
        let p = self.maybe_proto.get_or_insert_with(|| m.c.p.clone());
        if !proto_handle_eq(p, &m.c.p) {
            return Err(DifferentProtoInstance)
        }
        Ok(m.safe_wrap())
    }
    pub fn deliberate(&mut self) -> (LocId, LockedProto) {
        // step 1: prepare for callback (does not require lock)
        let mut sel = Select::new();
        let proto: &ProtoHandle = self.maybe_proto.as_ref().expect("NO PROTO??");
        for (expected_index, &id) in self.members_indexed.iter().enumerate() {
            // register this id's MsgDropbox as part of the selection
            let r = match proto.r.get_space(id) {
                Some(Space::PoPu(space)) => &space.dropbox.r,
                Some(Space::PoGe(space)) => &space.dropbox.r,
                _ => unreachable!(),
            };
            let index = sel.recv(r);
            assert_eq!(index, expected_index);
        }

        // step 2: lock proto and batch-flag readiness and tentativeness
        let mut w = proto.w.lock();
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
            assert_eq!(*ready & members, 0); // NO overlap beforehand
            *ready |= members;
            *tenta |= members;
        }
        drop(w);

        // step 3: await callback
        let oper = sel.select(); // BLOCKS!
        let id: LocId = *self
            .members_indexed
            .get(oper.index())
            .expect("UNEXPECTED INDEX");

        // step 4: protocol is committed. UNSET readiness and tentativeness again.
        let mut w = proto.w.lock();
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

            assert_eq!(*ready & members, members); // COMPLETE overlap beforehand
            *ready &= !members;
            *tenta &= !members;
        }

        // step 5: return which port was committed AND 
        let locked_proto = LockedProto {
            w,
            members: &self.members,
        };
        (id, locked_proto)
    }
}
impl Drop for PortGroup {
    fn drop(&mut self) {
        if let Some(ref proto) = self.maybe_proto {
            // UNCLAIM the contained ports
            let mut w = proto.w.lock();
            for (&id, &info) in self.member_info.iter() {
                w.unclaimed_ports.insert(id, info);
            }
        } else {
            assert!(self.members_indexed.is_empty());
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
