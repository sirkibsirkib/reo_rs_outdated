use super::*;

pub trait EndlessIter {
    fn endless_iter(
        &self,
    ) -> std::iter::Chain<std::slice::Iter<'_, usize>, std::iter::Repeat<&usize>>;
}
impl EndlessIter for Vec<usize> {
    fn endless_iter(
        &self,
    ) -> std::iter::Chain<std::slice::Iter<'_, usize>, std::iter::Repeat<&usize>> {
        self.iter().chain(std::iter::repeat(&0))
    }
}

/// User-facing protocol trait. Reo will generate structures that implement this.
///
/// Contains two important things:
/// 1. Definition of the `ProtoDef` which defines structure, behaviour etc.
/// 2. Defines the interface which allows
///     for the convenient `instantiate_and_claim` function.
pub trait Proto: Sized {
    type Interface: Sized;
    fn proto_def() -> ProtoDef;
    fn instantiate() -> Arc<ProtoAll>;
    fn instantiate_and_claim() -> Self::Interface;
}

pub(crate) trait HasMsgDropBox {
    fn get_dropbox(&self) -> &MsgDropbox;
    fn await_msg_timeout(&self, a: &ProtoAll, timeout: Duration, my_id: LocId) -> Option<usize> {
        println!("getting ... ");
        Some(match self.get_dropbox().recv_timeout(timeout) {
            Some(msg) => msg,
            None => {
                if a.w.lock().active.ready.set_to(my_id, false) {
                    // managed reverse my readiness
                    return None;
                } else {
                    // readiness has already been consumed
                    println!("too late");
                    self.get_dropbox().recv()
                }
            }
        })
    }
}
impl HasMsgDropBox for PoPuSpace {
    fn get_dropbox(&self) -> &MsgDropbox {
        &self.dropbox
    }
}
impl HasMsgDropBox for PoGeSpace {
    fn get_dropbox(&self) -> &MsgDropbox {
        &self.dropbox
    }
}

//////////////// INTERNAL SPECIALIZATION TRAITS for port-data ////////////
pub(crate) trait MaybeClone {
    fn maybe_clone(&self) -> Self;
}
impl<T> MaybeClone for T {
    default fn maybe_clone(&self) -> Self {
        panic!("type isn't clonable!")
    }
}

impl<T: Clone> MaybeClone for T {
    fn maybe_clone(&self) -> Self {
        self.clone()
    }
}

pub(crate) trait MaybeCopy {
    const IS_COPY: bool;
}
impl<T> MaybeCopy for T {
    default const IS_COPY: bool = false;
}

impl<T: Copy> MaybeCopy for T {
    const IS_COPY: bool = true;
}
pub(crate) trait MaybePartialEq {
    fn maybe_partial_eq(&self, other: &Self) -> bool;
}
impl<T> MaybePartialEq for T {
    default fn maybe_partial_eq(&self, _other: &Self) -> bool {
        panic!("type isn't partial eq!")
    }
}
impl<T: PartialEq> MaybePartialEq for T {
    fn maybe_partial_eq(&self, other: &Self) -> bool {
        self.eq(other)
    }
}

pub trait HasUnclaimedPorts {
    fn claim<T: 'static>(&self, id: LocId) -> ClaimResult<T>;
}
impl HasUnclaimedPorts for Arc<ProtoAll> {
    fn claim<T: 'static>(&self, id: LocId) -> ClaimResult<T> {
        use ClaimResult::*;
        let mut w = self.w.lock();
        if let Some(x) = w.unclaimed_ports.get(&id) {
            if x.type_id == TypeId::of::<T>() {
                let role = x.role;
                let _ = w.unclaimed_ports.remove(&id);
                match role {
                    PortRole::Putter => GotPutter(Putter {
                        p: self.clone(),
                        id,
                        phantom: PhantomData::default(),
                    }),
                    PortRole::Getter => GotGetter(Getter {
                        p: self.clone(),
                        id,
                        phantom: PhantomData::default(),
                    }),
                }
            } else {
                TypeMismatch
            }
        } else {
            NotUnclaimed
        }
    }
}

pub trait HasProto {
    fn get_proto(&self) -> &Arc<ProtoAll>;
}
impl<T: 'static> HasProto for Putter<T> {
    fn get_proto(&self) -> &Arc<ProtoAll> {
        &self.p
    }
}
impl<T: 'static> HasProto for Getter<T> {
    fn get_proto(&self) -> &Arc<ProtoAll> {
        &self.p
    }
}
