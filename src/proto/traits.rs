use super::*;
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
                let putter = x.putter;
                let _ = w.unclaimed_ports.remove(&id);
                if putter {
                    GotPutter(Putter {
                        p: self.clone(),
                        id,
                        phantom: PhantomData::default(),
                    })
                } else {
                    GotGetter(Getter {
                        p: self.clone(),
                        id,
                        phantom: PhantomData::default(),
                    })
                }
            } else {
                TypeMismatch
            }
        } else {
            NotUnclaimed
        }
    }
}

pub struct WithFirstIter<T: Iterator> {
    t: T,
    b: bool,
}
impl<T: Iterator> Iterator for WithFirstIter<T> {
    type Item = (bool, T::Item);
    fn next(&mut self) -> Option<Self::Item> {
        let was = self.b;
        self.b = false;
        self.t.next().map(|x| (was, x))
    }
}

pub trait WithFirst: Sized + Iterator {
    fn with_first(self) -> WithFirstIter<Self>;
} 
impl<T: Iterator + Sized> WithFirst for T {
    fn with_first(self) -> WithFirstIter<Self> {
        WithFirstIter {
            t: self,
            b: true,
        }
    }
} 