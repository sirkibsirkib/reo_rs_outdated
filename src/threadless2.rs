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
if #getters = 0: data is dropped and NOBODY moves it
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
    #[inline]
    pub fn port_id_to_mem_idx(&self, id: usize) -> Option<usize> {
        id.checked_sub(self.num_ports())
    }
    fn arrive(&self, arrive_set_id: usize) {
        self.arrive_locked(arrive_set_id, &mut self.ready.lock())
    }
    fn arrive_locked(&self, arrive_set_id: usize, ready: &mut MutexGuard<BitSet>) {
        ready.insert(arrive_set_id);
        for g in self.guards.iter() {
            if ready.is_superset(&g.firing_set) && (g.data_const)() {
                // condition met. FIRE!

                // update ready set
                ready.difference_with(&g.firing_set);

                for a in g.actions.iter() {
                    let num_port_getters = a.to.iter().filter(|&&x| self.is_port_id(x)).count();
                    println!("... num port getters {}", num_port_getters);
                    if let Some(p_mem_idx) = self.port_id_to_mem_idx(a.from) {
                        // MEM putter with index mem_idx
                        println!("... mem putter with memidx {}", p_mem_idx);
                        if a.to.is_empty() {
                            // no getters whatsoever. MUST drop this myself
                            println!("... 0 gets. dropping {}", p_mem_idx);
                            unsafe {
                                (*self.mem.get())[p_mem_idx].drop_contents();
                            }
                        } else {
                            // 1+ getters. contents are NOT dropped
                            println!("... {} gets", a.to.len());
                            let contents_ptr = unsafe { (*self.mem.get())[p_mem_idx].expose_ptr() };
                            println!("{} port getters", num_port_getters);
                            if num_port_getters == 0 {
                                // 0 getters are Ports. no messaging. one memcell moves
                                for (is_first, &g_id) in a.to.iter().with_first() {
                                    let g_mem_idx = self.port_id_to_mem_idx(g_id).unwrap();
                                    let memcell: &mut MemCell =
                                        unsafe { &mut (*self.mem.get())[g_mem_idx] };
                                    if is_first {
                                        unsafe { memcell.move_from_ptr(contents_ptr) };
                                    } else {
                                        unsafe { memcell.clone_from_ptr(contents_ptr) };
                                    }
                                    self.arrive_locked(g_id, ready); // RECURSIVE CALL
                                }
                            } else {
                                // 1+ getters are Ports
                                let mut leader_port_getter = None;
                                for &g_id in a.to.iter() {
                                    if let Some(g_mem_idx) = self.port_id_to_mem_idx(g_id) {
                                        // mem getter. ALWAYS clone
                                        unsafe {
                                            (*self.mem.get())[g_mem_idx]
                                                .clone_from_ptr(contents_ptr);
                                        }
                                        self.arrive_locked(g_id, ready); // RECURSIVE CALL
                                    } else {
                                        // port getter
                                        use MetaMsg::*;
                                        let msg = if let Some(leader_getter) = leader_port_getter {
                                            // follower port
                                            MemFollowerClone {
                                                leader_getter,
                                                mem_p_id: a.from,
                                            }
                                        } else {
                                            // I am the leader
                                            leader_port_getter = Some(g_id);
                                            MemLeaderMove {
                                                mem_p_id: a.from,
                                                wait_sum: num_port_getters - 1,
                                            }
                                        };
                                        self.meta_send[g_id].send(msg).unwrap();
                                    }
                                }
                            }
                        }
                    } else {
                        // PORT putter
                        println!("{} port putter", num_port_getters);
                        use MetaMsg::*;
                        let put_datum_ptr = unsafe {
                            // for MEMCELL getters
                            std::mem::transmute((*self.put_ptrs.get())[a.from].0)
                        };
                        if num_port_getters == 0 && !a.to.is_empty() {
                            // a MEMcell is the mover
                            self.meta_send[a.from].send(PutterWaitFor(1)).unwrap();
                            self.meta_send[a.from].send(IMovedIt).unwrap();
                            for (is_first, &g_id) in a.to.iter().with_first() {
                                let g_mem_idx = self.port_id_to_mem_idx(g_id).unwrap();
                                let memcell: &mut MemCell =
                                    unsafe { &mut (*self.mem.get())[g_mem_idx] };
                                if is_first {
                                    unsafe { memcell.move_from_ptr(put_datum_ptr) };
                                } else {
                                    unsafe { memcell.clone_from_ptr(put_datum_ptr) };
                                }
                                self.arrive_locked(g_id, ready); // RECURSIVE CALL
                            }
                        } else {
                            // a PORT is the mover
                            self.meta_send[a.from].send(PutterWaitFor(num_port_getters)).unwrap();
                            let mut was_moved = false;
                            for &g_id in a.to.iter() {
                                if let Some(g_mem_idx) = self.port_id_to_mem_idx(g_id) {
                                    // mem getter. ALWAYS clone
                                    unsafe {
                                        (*self.mem.get())[g_mem_idx].clone_from_ptr(put_datum_ptr);
                                    }
                                    self.arrive_locked(g_id, ready); // RECURSIVE CALL
                                } else {
                                    // port getter
                                    use MetaMsg::*;
                                    let msg = if was_moved {
                                        // follower port
                                        PortClone { src_putter: a.from }
                                    } else {
                                        // I am the leader
                                        was_moved = true;
                                        PortMove { src_putter: a.from }
                                    };
                                    self.meta_send[g_id].send(msg).unwrap();
                                }
                            }
                        }
                    }
                }
            }
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
                let mem_idx = self.port.shared.port_id_to_mem_idx(mem_p_id).unwrap();
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
                let mem_idx = self.port.shared.port_id_to_mem_idx(mem_p_id).unwrap();
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
        let mut decs = 0;
        let mut was_moved = false;
        let mut wait_for = std::usize::MAX;
        while wait_for != decs {
            match self.port.meta_recv.recv().unwrap() {
                MetaMsg::PutterWaitFor(x) => wait_for = x,
                MetaMsg::IMovedIt => decs += 1,
                MetaMsg::IClonedIt => {
                    if was_moved {
                        panic!("two getters moved it!");
                    }
                    was_moved = true;
                    decs += 1;
                }
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
