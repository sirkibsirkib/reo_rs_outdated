use super::*;
use ClaimResult as Cr;
use GroupAddError as Gae;

pub struct PortGroup {
    maybe_proto: Option<Arc<ProtoAll>>,
    members: BitSet,
    member_info: HashMap<LocId, PortInfo>,
    members_indexed: Vec<LocId>,
}

/// Compiles down to 2 pointer-follows and a sete
// fn proto_handle_eq(a: &ProtoHandle, b: &ProtoHandle) -> bool {
//     let cst = |x: &ProtoHandle| {
//         let x: &ProtoAll = &x;
//         x as *const ProtoAll
//     };
//     std::ptr::eq(cst(a), cst(b))
// }

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

    pub fn add_putter<D: Decimal, T>(
        &mut self,
        handle: &ProtoHandle,
        id: LocId,
    ) -> Result<Grouped<D, Putter<T>>, GroupAddError> {
        let m = match handle.claim::<T>(id) {
            Cr::GotGetter(_) => return Err(Gae::GotGetterExpectedPutter),
            Cr::GotPutter(p) => p,
            Cr::NotUnclaimed => return Err(Gae::NotUnclaimed),
            Cr::TypeMismatch => return Err(Gae::TypeMismatch),
        };
        let p = self.maybe_proto.get_or_insert_with(|| handle.clone());
        if !Arc::ptr_eq(p, &m.c.p) {
            return Err(Gae::DifferentProtoInstance);
        }
        Ok(Grouped::from_putter(m))
    }

    pub fn add_getter<D: Decimal, T>(
        &mut self,
        handle: &ProtoHandle,
        id: LocId,
    ) -> Result<Grouped<D, Getter<T>>, GroupAddError> {
        let m = match handle.claim::<T>(id) {
            Cr::GotGetter(g) => g,
            Cr::GotPutter(_) => return Err(Gae::GotPutterExpectedGetter),
            Cr::NotUnclaimed => return Err(Gae::NotUnclaimed),
            Cr::TypeMismatch => return Err(Gae::TypeMismatch),
        };
        let p = self.maybe_proto.get_or_insert_with(|| handle.clone());
        if !Arc::ptr_eq(p, &m.c.p) {
            return Err(Gae::DifferentProtoInstance);
        }
        Ok(Grouped::from_getter(m))
    }

    pub fn deliberate(&mut self) -> (LocId, LockedProto) {
        // step 1: prepare for callback (does not require lock)
        let mut sel = crossbeam::channel::Select::new();
        let proto: &ProtoHandle = self.maybe_proto.as_ref().expect("NO PROTO??");
        for (expected_index, &id) in self.members_indexed.iter().enumerate() {
            let r = match proto.r.get_space(id) {
                Some(Space::PoPu(space)) => &space.dropbox.r,
                Some(Space::PoGe(space)) => &space.dropbox.r,
                _ => unreachable!(),
            };
            let index = sel.recv(r); // add recv() of this msgdropbox to sel.
            assert_eq!(index, expected_index); // sanity check.
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
        ready_tentative.set_to(id, true); // THIS port will discover their tentative flag is set.

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

#[derive(Debug, Copy, Clone)]
pub enum GroupAddError {
    DifferentProtoInstance,
    GotGetterExpectedPutter,
    GotPutterExpectedGetter,
    NotUnclaimed,
    TypeMismatch,
}
