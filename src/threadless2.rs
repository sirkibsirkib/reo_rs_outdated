use std::fmt::Debug;
use std::borrow::Borrow;
use bit_set::BitSet;
use crossbeam::Receiver;
use crossbeam::Sender;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::sync::Arc;

struct Action {
    from: usize,
    to: Vec<usize>,
}
impl Action {
    fn new(from: usize, to: impl Iterator<Item = usize>) -> Self {
        Self {
            from,
            to: to.collect(),
        }
    }
}

struct GuardCmd {
    firing_set: BitSet,
    data_const: &'static dyn Fn() -> bool,
    actions: Vec<Action>,
}
impl GuardCmd {
    fn new(
        firing_set: BitSet,
        data_const: &'static dyn Fn() -> bool,
        actions: Vec<Action>,
    ) -> Self {
        Self {
            firing_set,
            data_const,
            actions,
        }
    }
}

#[derive(Copy, Clone)]
struct StackPtr(*mut ());
impl StackPtr {
    const NULL: Self = StackPtr(std::ptr::null_mut());
}
impl<T> From<*mut T> for StackPtr {
    fn from(p: *mut T) -> Self {
        StackPtr(unsafe { std::mem::transmute(p) })
    }
}
impl<T> Into<*mut T> for StackPtr {
    fn into(self) -> *mut T {
        unsafe { std::mem::transmute(self.0) }
    }
}

struct ProtoShared {
    ready: Mutex<BitSet>,
    guards: Vec<GuardCmd>,
    put_ptrs: UnsafeCell<Vec<StackPtr>>,
    meta_send: Vec<Sender<MetaMsg>>,
    // TODO id2guards
    // TODO dead set?
}
impl ProtoShared {
    fn arrive(&self, id: usize) {
        let mut ready = self.ready.lock();
        ready.insert(id);
        for g in self.guards.iter() {
            if ready.is_superset(&g.firing_set) && (g.data_const)() {
                if (g.data_const)() {
                    ready.difference_with(&g.firing_set);
                    for a in g.actions.iter() {
                        let num_getters = a.to.len();
                        self.meta_send[a.from]
                            .send(MetaMsg::SetWaitSum(num_getters))
                            .unwrap();
                        for &t in a.to.iter().take(1) {
                            self.meta_send[t].send(MetaMsg::MoveFrom(a.from)).unwrap();
                        }
                        for &t in a.to.iter().skip(1) {
                            self.meta_send[t].send(MetaMsg::CloneFrom(a.from)).unwrap();
                        }
                    }
                }
            }
        }
    }
}

struct PortCommon {
    shared: Arc<ProtoShared>,
    id: usize,
    meta_recv: Receiver<MetaMsg>,
}

pub struct Getter<T>
where
    T: TryClone,
{
    port: PortCommon,
    _port_type: PhantomData<T>,
}
impl<T> Getter<T>
where
    T: TryClone,
{
    fn new(port: PortCommon) -> Self {
        Self {
            port,
            _port_type: PhantomData::default(),
        }
    }
    pub fn get_borrowed<'a>(&'a mut self) -> Result<RefHandle<'a, T>, ()> {
        //// PUTTER HAS ACCESS
        self.port.shared.arrive(self.port.id);
        //// GETTERS HAVE ACCESS
        Ok(match self.port.meta_recv.recv().unwrap() {
            MetaMsg::CloneFrom(src_id) | MetaMsg::MoveFrom(src_id) => {
                let data = self.ref_from(src_id);
                RefHandle {
                    data,
                    putter_id: src_id,
                    getter: self,
                }
            }
            wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
        })
        //// PUTTER HAS ACCESS
    }
    pub fn get_weaker<X: CloneFrom<T>>(&mut self) -> Result<X, ()> {
        //// PUTTER HAS ACCESS
        self.port.shared.arrive(self.port.id);
        //// GETTERS HAVE ACCESS
        Ok(match self.port.meta_recv.recv().unwrap() {
            MetaMsg::CloneFrom(src_id) | MetaMsg::MoveFrom(src_id) => {
                let d = self.other_clone_from(src_id);
                self.port.shared.meta_send[src_id]
                    .send(MetaMsg::IClonedIt)
                    .unwrap();
                d
            }
            wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
        })
        //// PUTTER HAS ACCESS
    }
    pub fn get(&mut self) -> Result<T, ()> {
        //// PUTTER HAS ACCESS
        self.port.shared.arrive(self.port.id);
        //// GETTERS HAVE ACCESS
        Ok(match self.port.meta_recv.recv().unwrap() {
            MetaMsg::MoveFrom(src_id) => {
                let d = self.move_from(src_id);
                self.port.shared.meta_send[src_id]
                    .send(MetaMsg::IMovedIt)
                    .unwrap();
                d
            }
            MetaMsg::CloneFrom(src_id) => {
                let d = self.clone_from(src_id);
                self.port.shared.meta_send[src_id]
                    .send(MetaMsg::IClonedIt)
                    .unwrap();
                d
            }
            wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
        })
        //// PUTTER HAS ACCESS
    }

    #[inline]
    fn ref_from(&self, id: usize) -> &T {
        unsafe {
            let stack_ptr: StackPtr = (*self.port.shared.put_ptrs.get())[id];
            let p: *mut T = stack_ptr.into();
            &*p
        }   
    }

    #[inline]
    fn move_from(&self, id: usize) -> T {
        let stack_ptr: StackPtr = unsafe { (*self.port.shared.put_ptrs.get())[id] };
        let p: *mut T = stack_ptr.into();
        unsafe { std::mem::replace(&mut *p, std::mem::uninitialized()) }
    }

    #[inline]
    fn clone_from(&self, id: usize) -> T {
        self.ref_from(id).try_clone()
    }

    #[inline]
    fn other_clone_from<X: CloneFrom<T>>(&self, id: usize) -> X {
        let stack_ptr: StackPtr = unsafe { (*self.port.shared.put_ptrs.get())[id] };
        let p: *mut T = stack_ptr.into();
        let rp: &T = unsafe { &*p };
        CloneFrom::clone_from(rp)
    }
}

#[derive(Debug)]
enum MetaMsg {
    SetWaitSum(usize),
    MoveFrom(usize),
    CloneFrom(usize),
    IMovedIt,
    IClonedIt,
}

unsafe impl<T> Send for Putter<T> where T: TryClone {}
// unsafe impl<T> Sync for Putter<T> where T: TryClone {}
unsafe impl<T> Send for Getter<T> where T: TryClone {}
// unsafe impl<T> Sync for Getter<T> where T: TryClone {}
pub struct Putter<T>
where
    T: TryClone,
{
    port: PortCommon,
    _port_type: PhantomData<T>,
}
impl<T> Putter<T>
where
    T: TryClone,
{
    fn new(port: PortCommon) -> Self {
        Self {
            port,
            _port_type: PhantomData::default(),
        }
    }
    pub fn put(&mut self, mut datum: T) -> Result<(), T> {
        //// PUTTER HAS ACCESS
        let r: *mut T = &mut datum;
        unsafe { (*self.port.shared.put_ptrs.get())[self.port.id] = r.into() };
        self.port.shared.arrive(self.port.id);
        //// GETTERS HAVE ACCESS
        let mut decs = 0;
        let mut was_moved = false;
        let mut wait_for = std::usize::MAX;
        while wait_for != decs {
            match self.port.meta_recv.recv().unwrap() {
                MetaMsg::SetWaitSum(x) => wait_for = x,
                MetaMsg::IMovedIt => decs += 1,
                MetaMsg::IClonedIt => {
                    if was_moved {
                        panic!("two getters moved it!");
                    }
                    was_moved = true;
                    decs += 1;
                },
                wrong_meta => panic!("putter wasn't expecting {:?}", wrong_meta),
            }
        }
        //// PUTTER HAVE ACCESS
        unsafe { (*self.port.shared.put_ptrs.get())[self.port.id] = StackPtr::NULL };
        if was_moved {
            std::mem::forget(datum);
        }
        Ok(())
    }
}

macro_rules! usize_iter_literal {
    ($array:expr) => {
        $array.iter().cloned()
    };
}

pub fn new_proto() -> (Putter<[u32; 32]>, Getter<[u32; 32]>) {
    const NUM_PORTS: usize = 2;
    const NUM_PUTTERS: usize = 1;
    fn guard_0_data_const() -> bool {
        true
    }
    let ready = Mutex::new(BitSet::new());
    let guards = vec![GuardCmd::new(
        bitset! {0,1},
        &guard_0_data_const,
        vec![Action::new(0, usize_iter_literal!([1]))],
    )];
    let put_ptrs = UnsafeCell::new(
        std::iter::repeat(StackPtr::NULL)
            .take(NUM_PUTTERS)
            .collect(),
    );
    let mut meta_send = Vec::with_capacity(NUM_PORTS);
    let mut meta_recv = Vec::with_capacity(NUM_PORTS);
    for _ in 0..NUM_PORTS {
        let (s, r) = crossbeam::channel::bounded(NUM_PORTS);
        meta_send.push(s);
        meta_recv.push(r);
    }
    let shared = Arc::new(ProtoShared {
        ready,
        guards,
        put_ptrs,
        meta_send,
    });
    (
        Putter::new(PortCommon {
            shared: shared.clone(),
            id: 0,
            meta_recv: meta_recv.remove(0), //remove vec head
        }),
        Getter::new(PortCommon {
            shared: shared.clone(),
            id: 1,
            meta_recv: meta_recv.remove(0), //remove vec head
        }),
    )
}

pub trait TryClone {
    fn try_clone(&self) -> Self;
}
impl<T> TryClone for T
where
    T: Clone,
{
    fn try_clone(&self) -> Self {
        self.clone()
    }
}

pub trait CloneFrom<T> {
    fn clone_from(t: &T) -> Self;
}
impl<T> CloneFrom<T> for () {
    fn clone_from(_t: &T) -> Self {}
}
// impl<T> CloneFrom<T> for T {
//     fn clone_from(t: &T) -> T { t.clone() }
// }

trait GetterExt {
    fn finish_borrow(&self, putter_id: usize);
}
impl<T> GetterExt for Getter<T> where T: TryClone {
    fn finish_borrow(&self, putter_id: usize) {
        self.port.shared.meta_send[putter_id]
            .send(MetaMsg::IClonedIt)
            .unwrap();
    }
}



impl<'a, T> Debug for RefHandle<'a, T> where T: Debug {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.data.fmt(f)
    }
}
pub struct RefHandle<'a, T> {
    data: &'a T,
    putter_id: usize,
    getter:  &'a dyn GetterExt,
}
impl<'a, T> Drop for RefHandle<'a, T> {
    fn drop(&mut self) {
        self.getter.finish_borrow(self.putter_id)
    }
}
impl<'a, T> AsRef<T> for RefHandle<'a, T> {
    fn as_ref(&self) -> &T {
        self.data
    }
}