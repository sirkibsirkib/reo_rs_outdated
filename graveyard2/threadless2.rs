use parking_lot::MutexGuard;
use std::fmt::Debug;
// use std::borrow::Borrow;
use bit_set::BitSet;
use crossbeam::Receiver;
use crossbeam::Sender;
use parking_lot::Mutex;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Action {
    from: usize,
    to: Vec<usize>,
}
impl Action {
    pub fn new(from: usize, to: impl Iterator<Item = usize>) -> Self {
        Self {
            from,
            to: to.collect(),
        }
    }
}

pub struct GuardCmd {
    firing_set: BitSet,
    data_const: &'static dyn Fn() -> bool,
    actions: Vec<Action>,
}
impl GuardCmd {
    pub fn new(
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
pub struct StackPtr(*mut ());
impl StackPtr {
    pub const NULL: Self = StackPtr(std::ptr::null_mut());
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

pub struct MemCell {
    data: Vec<u8>,
    id: usize,
    outstanding_gets: Mutex<usize>,
}
type VoidPtr = *const ();
impl MemCell {
    pub fn new<T: Sized>(id: usize) -> Self {
        let bytes = std::mem::size_of::<T>();
        Self {
            id,
            data: std::iter::repeat(0).take(bytes).collect(),
            outstanding_gets: Mutex::new(0),
        }
    }
    pub unsafe fn drop_contents(&self) {
        unimplemented!()
    }
    pub unsafe fn move_from_ptr(&self, src_ptr: VoidPtr) {
        unimplemented!()
    }
    pub unsafe fn clone_from_ptr(&self, src_ptr: VoidPtr) {
        unimplemented!()
    }
    pub unsafe fn expose_ptr(&self) -> VoidPtr {
        unimplemented!()
    }
}

trait WithWithFirst<T>: Iterator<Item = T> + Sized {
    fn with_first(self) -> WithFirst<Self, T> {
        WithFirst {
            iter: self,
            first: true,
        }
    }
}
impl<I, T> WithWithFirst<T> for I where I: Iterator<Item = T> + Sized {}

pub struct WithFirst<I, T>
where
    I: Iterator<Item = T> + Sized,
{
    iter: I,
    first: bool,
}
impl<I, T> Iterator for WithFirst<I, T>
where
    I: Iterator<Item = T> + Sized,
{
    type Item = (bool, T);
    fn next(&mut self) -> Option<Self::Item> {
        if self.first {
            self.first = false;
            self.iter.next().map(|x| (true, x))
        } else {
            self.iter.next().map(|x| (false, x))
        }
    }
}

pub struct ProtoShared {
    pub ready: Mutex<BitSet>,
    pub guards: Vec<GuardCmd>,
    pub put_ptrs: UnsafeCell<Vec<StackPtr>>,
    pub meta_send: Vec<Sender<MetaMsg>>,
    pub mem: UnsafeCell<Vec<MemCell>>,
}

/*
// dropped in circuit if NO getters
// otherwise, 1 mover and N-1 cloners
//

1. is the datum DROPPED IN CIRCUIT?
2. are 1+ port-getters involved?
3. is the putter a mem or a port?


let dropped_in_circ = a.to.is_empty();
let nonzero_port_getters =
*/

impl ProtoShared {
    #[inline]
    pub fn num_ports(&self) -> usize {
        self.meta_send.len()
    }
    #[inline]
    pub fn is_port_id(&self, id: usize) -> bool {
        id < self.meta_send.len()
    }
    pub fn mem_id_p2g(&self, id: usize) -> usize {
        let len = (*self.mem.get()).len();
        let mem_idx = match id.checked_sub(self.num_ports()) {
            Some(x) if x < len => x,
            x => panic!("mem_id_p2g BAD {:?}", x),
        };
        mem_idx + len
    }
    pub fn mem_id_g2p(&self, id: usize) -> usize {
        let len = (*self.mem.get()).len();
        let mem_idx = match id.checked_sub(self.num_ports()) {
            Some(x) if x < (2 * len) && len <= x => x,
            x => panic!("mem_id_p2g BAD {:?}", x),
        };
        mem_idx - len
    }
    #[inline]
    pub fn put_port_to_memslot(&self, id: usize) -> Option<usize> {
        id.checked_sub(self.num_ports())
    }
    #[inline]
    pub fn get_port_to_memslot(&self, id: usize) -> Option<usize> {
        let len = unsafe { (*self.mem.get()).len() };
        id.checked_sub(self.num_ports())
            .and_then(|x| x.checked_sub(len))
    }

    fn arrive(&self, arrive_set_id: usize) {
        let ready = &mut self.ready.lock();
        ready.insert(arrive_set_id);
        'redo: loop {
            for g in self.guards.iter() {
                if !ready.is_superset(&g.firing_set) || !(g.data_const)() {
                    continue;
                }

                // update ready set
                ready.difference_with(&g.firing_set);
                for a in g.actions.iter() {


                    let num_port_getters = a.to.iter().filter(|&&x| self.is_port_id(x)).count();
                    // let datum_ptr = match self.put_port_to_memslot(a.from) {
                    //     Some(m) => (*self.mem.get())[m].expose_ptr(),
                    //     None => (*self.put_ptrs.get())[a.from].0,
                    // };

                    let maybe_mem_p = self.put_port_to_memslot(a.from);
                    if let (Some(mem_p_slot), 0) = (maybe_mem_p, num_port_getters) {
                        // NO LATER CALL
                        let datum_ptr = unsafe { (*self.mem.get())[mem_p_slot].expose_ptr() };
                        let mover_id = a.to.iter().find(|&&x| self.get_port_to_memslot(x).unwrap()==mem_p_slot)
                        .or_else(|| a.to.iter().next());

                        if let Some(&some_mover_id) = mover_id {
                            for &g_id in a.to.iter() {
                                let g_slot = self.get_port_to_memslot(g_id).unwrap();
                                let me: &mut MemCell = unsafe { &mut (*self.mem.get())[g_slot] };
                                if g_id == some_mover_id {
                                    if g_slot != mem_p_slot {
                                        unsafe { me.move_from_ptr(datum_ptr) }
                                    } else {
                                        // special case! move to itself! no action required
                                    }
                                } else {
                                    unsafe { me.clone_from_ptr(datum_ptr) }
                                }
                            }
                        } else {
                            // no getters at all! drop
                            unsafe { (*self.mem.get())[mem_p_slot].drop_contents() }
                        }
                        continue 'redo;
                    }

                    // there will be a later call. no need to call REDO
                }
            }
            break;
        }
    }

    #[inline]
    fn follow_stack_ptr(&self, id: usize) -> StackPtr {
        unsafe { (*self.put_ptrs.get())[id] }
    }
}

pub struct PortCommon {
    pub shared: Arc<ProtoShared>,
    pub id: usize,
    pub meta_recv: Receiver<MetaMsg>,
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
    pub fn new(port: PortCommon) -> Self {
        Self {
            port,
            _port_type: PhantomData::default(),
        }
    }
    // pub fn get_borrowed<'a>(&'a mut self) -> Result<RefHandle<'a, T>, ()> {
    //     //// PUTTER HAS ACCESS
    //     self.port.shared.arrive(self.port.id);
    //     //// GETTERS HAVE ACCESS
    //     Ok(match self.port.meta_recv.recv().unwrap() {
    //         MetaMsg::PortMove{src_putter} | MetaMsg::PortClone{src_putter} => {
    //             let data = self.ref_from(src_putter);
    //             RefHandle {
    //                 data,
    //                 putter_id: src_putter,
    //                 getter: self,
    //             }
    //         }
    //         wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
    //     })
    //     //// PUTTER HAS ACCESS
    // }
    // pub fn get_weaker<X: CloneFromPortPutter<T>>(&mut self) -> Result<X, ()> {
    //     //// PUTTER HAS ACCESS
    //     self.port.shared.arrive(self.port.id);
    //     //// GETTERS HAVE ACCESS
    //     Ok(match self.port.meta_recv.recv().unwrap() {
    //         MetaMsg::PortMove{src_putter} | MetaMsg::PortClone{src_putter} => {
    //             let d = self.other_clone_from(src_p`utter);
    //             self.port.shared.meta_send[src_putter]
    //                 .send(MetaMsg::IClonedIt)
    //                 .unwrap();
    //             d
    //         }
    //         wrong_meta => panic!("getter wasn't expecting {:?}", wrong_meta),
    //     })
    //     //// PUTTER HAS ACCESS
    // }
    pub fn get(&mut self) -> Result<T, ()> {
        use MetaMsg::*;
        //// PUTTER HAS ACCESS
        self.port.shared.arrive(self.port.id);
        //// GETTERS HAVE ACCESS
        let m = self.port.meta_recv.recv().unwrap();
        println!("get got {:?}", m);
        Ok(match m {
            PortMove { src_putter } => {
                let d = self.move_from(src_putter);
                self.port.shared.meta_send[src_putter]
                    .send(MetaMsg::IMovedIt)
                    .unwrap();
                d
            }
            PortClone { src_putter } => {
                let d = self.clone_from(src_putter);
                self.port.shared.meta_send[src_putter]
                    .send(MetaMsg::IClonedIt)
                    .unwrap();
                d
            }
            MemLeaderMove { mem_p_id, wait_sum } => {
                for _ in 0..wait_sum {
                    match self.port.meta_recv.recv().unwrap() {
                        IClonedIt => {}
                        wrong_meta => panic!("getter leader got {:?}", wrong_meta),
                    }
                }
                let mem_idx = self.port.shared.put_port_to_memslot(mem_p_id).unwrap();
                unsafe {
                    let ptr = (*self.port.shared.mem.get())[mem_idx].expose_ptr();
                    let ptr2: *mut T = std::mem::transmute(ptr);
                    std::mem::replace(&mut *ptr2, std::mem::uninitialized()) // move
                }
            }
            MemFollowerClone {
                mem_p_id,
                leader_getter,
            } => {
                let mem_idx = self.port.shared.put_port_to_memslot(mem_p_id).unwrap();
                unsafe {
                    let ptr = (*self.port.shared.mem.get())[mem_idx].expose_ptr();
                    let ptr2: &T = std::mem::transmute(ptr);
                    let d = ptr2.try_clone();
                    self.port.shared.meta_send[leader_getter]
                        .send(MetaMsg::IClonedIt)
                        .unwrap();
                    d
                }
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
    fn other_clone_from<X: CloneFromPortPutter<T>>(&self, id: usize) -> X {
        let stack_ptr: StackPtr = unsafe { (*self.port.shared.put_ptrs.get())[id] };
        let p: *mut T = stack_ptr.into();
        let rp: &T = unsafe { &*p };
        CloneFromPortPutter::clone_from(rp)
    }
}

#[derive(Debug)]
pub enum MetaMsg {
    PutterWaitFor(usize),
    PortMove {
        src_putter: usize,
    },
    PortClone {
        src_putter: usize,
    },
    MemFollowerClone {
        leader_getter: usize,
        mem_p_id: usize,
    },
    MemLeaderClone {
        mem_p_id: usize,
        wait_sum: usize,
    },
    MemLeaderMove {
        mem_p_id: usize,
        wait_sum: usize,
    },
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
    pub fn new(port: PortCommon) -> Self {
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
        let mut wait_for = match self.port.meta_recv.recv().unwrap() {
            MetaMsg::PutterWaitFor(x) => x,
            wrong_meta => panic!("putter wasn't expecting {:?}", wrong_meta),
        };
        let was_moved: bool = (0..wait_for).any(|_| match self.port.meta_recv.recv().unwrap() {
            MetaMsg::IMovedIt => true,
            MetaMsg::IClonedIt => false,
            wrong_meta => panic!("putter wasn't expecting {:?}", wrong_meta),
        });
        //// PUTTER HAVE ACCESS
        unsafe { (*self.port.shared.put_ptrs.get())[self.port.id] = StackPtr::NULL };
        if was_moved {
            std::mem::forget(datum);
        } // else drop at end of func
        Ok(())
    }
}

macro_rules! iter_literal {
    ($array:expr) => {
        $array.iter().cloned()
    };
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

pub trait CloneFromPortPutter<T> {
    fn clone_from(t: &T) -> Self;
}
impl<T> CloneFromPortPutter<T> for () {
    fn clone_from(_t: &T) -> Self {}
}
// impl<T> CloneFromPortPutter<T> for T {
//     fn clone_from(t: &T) -> T { t.clone() }
// }

trait GetterExt {
    fn finish_borrow(&self, putter_id: usize);
}
impl<T> GetterExt for Getter<T>
where
    T: TryClone,
{
    fn finish_borrow(&self, putter_id: usize) {
        self.port.shared.meta_send[putter_id]
            .send(MetaMsg::IClonedIt)
            .unwrap();
    }
}

impl<'a, T> Debug for RefHandle<'a, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.data.fmt(f)
    }
}
pub struct RefHandle<'a, T> {
    data: &'a T,
    putter_id: usize,
    getter: &'a dyn GetterExt,
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
