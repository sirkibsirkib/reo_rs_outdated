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

// trait to cut down boilerplate in the rest of the lib
pub(crate) trait PutterSpace {
    fn set_ptr(&self, ptr: *mut u8);
    fn get_ptr(&self) -> *mut u8;
    fn get_type_info(&self) -> &TypeInfo;
    fn get_mover_sema(&self) -> &Semaphore;
    fn get_cloner_countdown(&self) -> &AtomicUsize;
    unsafe fn get_datum_from<D>(&self, case: DataGetCase, out_ptr: *mut u8, finish_fn: D)
    where
        D: Fn(bool),
    {
        let src = self.get_ptr();
        let type_info = self.get_type_info();

        if type_info.is_copy {
            // MOVE HAPPENS HERE
            src.copy_to(out_ptr, type_info.bytes);
            let was = self.get_cloner_countdown().fetch_sub(1, Ordering::SeqCst);
            if was == case.last_countdown() {
                finish_fn(false);
            }
        } else {
            if case.i_move() {
                if case.mover_must_wait() {
                    self.get_mover_sema().acquire();
                }
                // MOVE HAPPENS HERE
                src.copy_to(out_ptr, type_info.bytes);
                finish_fn(false);
            } else {
                // CLONE HAPPENS HERE
                type_info.clone_fn.execute(src, out_ptr);
                let was = self.get_cloner_countdown().fetch_sub(1, Ordering::SeqCst);
                if was == case.last_countdown() {
                    if case.someone_moves() {
                        self.get_mover_sema().release();
                    } else {
                        finish_fn(true);
                    }
                }
            }
        }
    }
}
impl PutterSpace for PoPuSpace {
    fn set_ptr(&self, ptr: *mut u8) {
        self.ptr.store(ptr, Ordering::SeqCst);
    }
    fn get_ptr(&self) -> *mut u8 {
        self.ptr.load(Ordering::SeqCst)
    }
    fn get_cloner_countdown(&self) -> &AtomicUsize {
        &self.cloner_countdown
    }
    fn get_type_info(&self) -> &TypeInfo {
        &self.type_info
    }
    fn get_mover_sema(&self) -> &Semaphore {
        &self.mover_sema
    }
}
impl PutterSpace for MemoSpace {
    fn set_ptr(&self, ptr: *mut u8) {
        self.ptr.store(ptr, Ordering::SeqCst);
    }
    fn get_ptr(&self) -> *mut u8 {
        self.ptr.load(Ordering::SeqCst)
    }
    fn get_cloner_countdown(&self) -> &AtomicUsize {
        &self.cloner_countdown
    }
    fn get_type_info(&self) -> &TypeInfo {
        &self.type_info
    }
    fn get_mover_sema(&self) -> &Semaphore {
        &self.mover_sema
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