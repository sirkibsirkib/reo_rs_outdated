use crate::proto::memory::Storage;
use itertools::izip;

pub mod definition;
mod memory;
use definition::{Formula, LocKind, ProtoBuildErr, ProtoBuilder, TypelessProtoDef};

pub mod reflection;
use reflection::TypeInfo;

pub mod traits;
use traits::{
    DataSource, HasMsgDropBox, HasUnclaimedPorts, MaybeClone, MaybeCopy, MaybePartialEq,
    MemFillPromise, MemFillPromiseFulfilled, Proto,
};

#[cfg(test)]
mod tests;

pub mod groups;

use crate::{
    bitset::BitSet,
    helper::WithFirst,
    tokens::{decimal::Decimal, Grouped},
    LocId, ProtoHandle,
};
use hashbrown::HashMap;
use parking_lot::{Mutex, MutexGuard};
use std::{
    alloc::{self, Layout},
    any::TypeId,
    cell::UnsafeCell,
    convert::TryInto,
    fmt::Debug,
    marker::PhantomData,
    mem::{transmute, MaybeUninit},
    str::FromStr,
    sync::{
        atomic::{AtomicPtr, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use std_semaphore::Semaphore;

/// A coordination point that getters interact with to acquire a datum.
/// Common to memory and port putters.
#[derive(debug_stub_derive::DebugStub)]
pub(crate) struct PutterSpace {
    ptr: AtomicPtr<u8>,
    cloner_countdown: AtomicUsize,
    #[debug_stub = "<Semaphore>"]
    mover_sema: Semaphore,
    type_info: Arc<TypeInfo>,
}
impl PutterSpace {
    fn new(ptr: *mut u8, type_info: Arc<TypeInfo>) -> Self {
        Self {
            ptr: ptr.into(),
            cloner_countdown: 0.into(),
            mover_sema: Semaphore::new(0),
            type_info,
        }
    }
    pub fn overwrite_null_ptr(&self, ptr: *mut u8) {
        let was = self.ptr.swap(ptr, Ordering::SeqCst);
        assert!(was.is_null());
    }
    pub fn remove_ptr(&self) -> *mut u8 {
        self.ptr.swap(std::ptr::null_mut(), Ordering::SeqCst)
    }
    pub fn set_ptr(&self, ptr: *mut u8) {
        self.ptr.store(ptr, Ordering::SeqCst);
    }
    pub fn get_ptr(&self) -> *mut u8 {
        self.ptr.load(Ordering::SeqCst)
    }
}

/// Memory variant of PutterSpace. Contains no additional data but has unique
/// behavior: simulating "Drop" for a ptr that may be shared with other memory cells.

#[derive(Debug)]
struct MemoSpace {
    p: PutterSpace,
}
impl MemoSpace {
    fn new(ptr: *mut u8, type_info: Arc<TypeInfo>) -> Self {
        Self {
            p: PutterSpace::new(ptr, type_info),
        }
    }

    /// invoked from both protocol or last getter.
    pub(crate) fn make_empty(&self, w: &mut ProtoActive, drop_if_last_ref: bool, my_id: LocId) {
        let src = self.p.remove_ptr();
        let refs: &mut usize = w.mem_refs.get_mut(&src).expect("no memrefs?");
        assert!(*refs >= 1);
        *refs -= 1;
        if *refs == 0 {
            w.mem_refs.remove(&src);
            unsafe {
                if drop_if_last_ref {
                    println!("MEM CELL DROPPING");
                    w.storage.drop_inside(src, &self.p.type_info)
                } else {
                    println!("MEM CELL FORGETTING");
                    w.storage.forget_inside(src, &self.p.type_info)
                }
            }
        }
        let was = w.ready.set_to(my_id, true); // I am ready
        assert!(!was);
    }
}

/// Port-variant of PutterSpace. Ptr here points to the putter's stack
/// Also includes a dropbox for receiving coordination messages
#[derive(Debug)]
struct PoPuSpace {
    p: PutterSpace,
    dropbox: MsgDropbox,
}
impl PoPuSpace {
    fn new(type_info: Arc<TypeInfo>) -> Self {
        Self {
            p: PutterSpace::new(std::ptr::null_mut(), type_info),
            dropbox: MsgDropbox::new(),
        }
    }
}

/// Personal coordination space for this getter to receive messages and advertise
/// whether it called get() or get_signal().
#[derive(Debug)]
struct PoGeSpace {
    dropbox: MsgDropbox, // used only by this guy to recv messages
    want_data: UnsafeCell<bool>,
}
impl PoGeSpace {
    fn new() -> Self {
        Self {
            dropbox: MsgDropbox::new(),
            want_data: false.into(),
        }
    }
    fn set_want_data(&self, want_data: bool) {
        unsafe { *self.want_data.get() = want_data }
    }
    fn get_want_data(&self) -> bool {
        unsafe { *self.want_data.get() }
    }
    unsafe fn participate_with_msg(&self, a: &ProtoAll, msg: usize, out_ptr: *mut u8) {
        let (case, putter_id) = DataGetCase::parse_msg(msg);
        match a.r.get_space(putter_id) {
            Some(Space::Memo(space)) => space.acquire_data(out_ptr, case, (a, putter_id)),
            Some(Space::PoPu(space)) => space.acquire_data(out_ptr, case, ()),
            _ => panic!("Bad putter ID!!"),
        }
    }
}

/// portion of the Protocol state that is both:
/// 1. protected by the lock
/// 2. mutably accessed when firing rules
struct ProtoActive {
    ready: BitSet,
    storage: Storage,
    mem_refs: HashMap<*mut u8, usize>,
}

/// Part of protocol Meta-state. Remembers:
/// 1. which rule has been committed to
/// 2. how many tentative ports are outstanding before it can be fired
struct Commitment {
    rule_id: usize,
    awaiting: usize,
}

/// Part of protocol Meta-state. Represents a port-group waiting for a particular
/// state-predicate to be met.
struct StateWaiter {
    state: BitSet,
    whom: LocId,
}

trait DebugPrint {
    fn debug_print(&self);
}
impl<'a, 'b> DebugPrint for (&'a ProtoR, &'b ProtoW) {
    fn debug_print(&self) {
        println!(":: MEMOR: {:?}", &self.1.memory_bits);
        println!(":: READY: {:?}", &self.1.active.ready);
        println!(":: TENTA: {:?}", &self.1.ready_tentative);
    }
}

/// The portion of the protcol that is proected by the lock.
struct ProtoW {
    memory_bits: BitSet,
    active: ProtoActive,
    commitment: Option<Commitment>,
    ready_tentative: BitSet,
    awaiting_states: Vec<StateWaiter>,
    unclaimed_ports: HashMap<LocId, PortInfo>,
}
impl ProtoW {
    fn notify_state_waiters(ready: &BitSet, awaiting_states: &mut Vec<StateWaiter>, r: &ProtoR) {
        awaiting_states.retain(|awaiting_state| {
            let retain = if ready.is_superset(&awaiting_state.state) {
                match r.get_space(awaiting_state.whom) {
                    Some(Space::PoPu(space)) => space.dropbox.send_nothing(),
                    Some(Space::PoGe(space)) => space.dropbox.send_nothing(),
                    _ => panic!("bad state-waiter LocId!"),
                };
                false
            } else {
                true
            };
            retain
        })
    }
    /// "Act as protocol" procedure. Mutable reference ensures 0/1 threads
    /// call this per proto at a time.
    fn enter(&mut self, r: &ProtoR, my_id: LocId) {
        println!("ENTER WITH ID {}", my_id);
        self.active.ready.set_to(my_id, true);
        (r, self as &ProtoW).debug_print();
        match &mut self.commitment {
            Some(commitment) => {
                let i_was_tentative = !self.ready_tentative.set_to(my_id, false);
                if i_was_tentative {
                    commitment.awaiting -= 1;
                    if commitment.awaiting == 0 {
                        // I was the last!
                        let rule = &r.rules[commitment.rule_id];
                        subtract_readiness(&mut self.active.ready, rule);
                        rule.fire(Firer {
                            r,
                            w: &mut self.active,
                        });
                        Self::notify_state_waiters(
                            &self.active.ready,
                            &mut self.awaiting_states,
                            r,
                        );
                        self.commitment = None;
                        self.exhaust_rules(r);
                    }
                }
            }
            None => self.exhaust_rules(r),
        }
    }

    fn exhaust_rules(&mut self, r: &ProtoR) {
        'repeat: loop {
            // keep looping until 0 rules can fire
            for (rule_id, rule) in r.rules.iter().enumerate() {
                let bits_ready = is_ready(&self.memory_bits, &self.active.ready, rule);
                if bits_ready && unsafe { r.eval_formula(&rule.guard_pred) } {
                    // safe if Equal functions are sound
                    println!("FIRING {}: {:?}", rule_id, rule);
                    println!("FIRING BEFORE:");
                    (r, self as &ProtoW).debug_print();
                    println!("refs currently: {:?}", &self.active.mem_refs);

                    let mut num_tenatives = 0;
                    for id in self.active.ready.iter_and(&self.ready_tentative) {
                        num_tenatives += 1;
                        match r.get_space(id) {
                            Some(Space::PoPu(po_pu)) => po_pu.dropbox.send(rule_id),
                            Some(Space::PoGe(po_ge)) => po_ge.dropbox.send(rule_id),
                            _ => panic!("bad tentative!"),
                        }
                    }
                    // assign bits BEFORE the action happens. necessary for the tentative ports
                    assign_memory_bits(&mut self.memory_bits, rule);

                    // tenative ports! must wait for them to resolve
                    if num_tenatives > 0 {
                        self.commitment = Some(Commitment {
                            rule_id,
                            awaiting: num_tenatives,
                        });
                        return;
                    }
                    subtract_readiness(&mut self.active.ready, rule);
                    rule.fire(Firer {
                        r,
                        w: &mut self.active,
                    });

                    println!("FIRING AFTER:");
                    (r, self as &ProtoW).debug_print();
                    println!("refs currently: {:?}", &self.active.mem_refs);
                    println!("----------");

                    Self::notify_state_waiters(&self.active.ready, &mut self.awaiting_states, r);
                    continue 'repeat;
                }
            }
            // only get here if NO rule fired
            println!("EXITING");
            return;
        }
    }
}

fn subtract_readiness(ready: &mut BitSet, rule: &RunRule) {
    // ready.pad_trailing_zeroes(rule.guard_ready.data.len());
    println!(
        "subtracking readiness {:?}",
        (ready.data.len(), rule.guard_ready.data.len())
    );
    for (mr, &gr) in izip!(ready.data.iter_mut(), rule.guard_ready.data.iter()) {
        *mr &= !gr;
    }
}

/// Returns TRUE if the given memory and readiness bitsets satisfy the guard
/// of the provided rule. The guard is able to specify which bits should be
/// ready & true, and which should be ready & false.
fn is_ready(memory: &BitSet, ready: &BitSet, rule: &RunRule) -> bool {
    for (&mr, &mv, &gr, &gv) in izip!(
        ready.data.iter(),
        memory.data.iter(),
        rule.guard_ready.data.iter(),
        rule.guard_full.data.iter(),
    ) {
        let should_be_pos = gr & gv;
        let should_be_neg = gr & !gv;
        let are_pos = mr & mv;
        let are_neg = mr & !mv;
        let false_neg = should_be_pos & !(are_pos);
        let false_pos = should_be_neg & !(are_neg);
        if (false_neg | false_pos) != 0 {
            return false;
        }
    }
    println!(
        "is_ready returning TRUE with lens {:?}",
        (
            ready.data.len(),
            memory.data.len(),
            rule.guard_ready.data.len(),
            rule.guard_full.data.len()
        )
    );
    true
}

/// updates the memory bitset to reflect the effects of applying this rule.
/// rule has (values, mask). where bits of:
/// - (0, 1) signify a bit that will be UNSET in memory
/// - (1, 1) signify a bit that will be SET in memory
/// eg rule with (000111, 101010) will do
///     memory
///  |= 000010
///  &= 010111
fn assign_memory_bits(memory: &mut BitSet, rule: &RunRule) {
    // memory.pad_trailing_zeroes(rule.assign_mask.data.len());
    for (mv, &av, &am) in izip!(
        memory.data.iter_mut(),
        rule.assign_vals.data.iter(),
        rule.assign_mask.data.iter(),
    ) {
        // set trues
        *mv |= av | am;
        // unset falses
        *mv &= av | !am;
    }
}

#[derive(Debug)]
enum Space {
    PoPu(PoPuSpace),
    PoGe(PoGeSpace),
    Memo(MemoSpace),
}

/// Part of the protocol NOT protected by the lock
pub struct ProtoR {
    rules: Vec<RunRule>,
    spaces: HashMap<LocId, Space>,
}
impl ProtoR {
    unsafe fn eval_formula(&self, guard: &Formula) -> bool {
        use definition::Formula::*;
        let f = |formula: &Formula| self.eval_formula(formula);
        match guard {
            True => true,
            None(x) => !x.iter().any(f),
            And(x) => !x.iter().all(f),
            Or(x) => x.iter().any(f),
            Eq(a, b) => self.equal_put_data(*a, *b),
        }
    }

    unsafe fn equal_put_data(&self, a: LocId, b: LocId) -> bool {
        let clos = |id| match self.get_space(id) {
            Some(Space::Memo(space)) => (&space.p.type_info, space.p.get_ptr()),
            Some(Space::PoPu(space)) => (&space.p.type_info, space.p.get_ptr()),
            _ => panic!("NO SPACE PTR"),
        };
        let (i1, p1) = clos(a);
        let (i2, p2) = clos(b);
        assert_eq!(i1.type_id, i2.type_id);
        i1.partial_eq_fn.execute(p1, p2)
    }
    fn send_to_getter(&self, id: LocId, msg: usize) {
        if let Some(Space::PoGe(space)) = self.get_space(id) {
            space.dropbox.send(msg)
        } else {
            panic!("not a getter!")
        }
    }
    fn get_po_pu(&self, id: LocId) -> Option<&PoPuSpace> {
        if let Some(Space::PoPu(space)) = self.get_space(id) {
            return Some(space);
        } else {
            None
        }
    }
    fn get_po_ge(&self, id: LocId) -> Option<&PoGeSpace> {
        if let Some(Space::PoGe(space)) = self.get_space(id) {
            Some(space)
        } else {
            None
        }
    }
    fn get_me_pu(&self, id: LocId) -> Option<&MemoSpace> {
        if let Some(Space::Memo(space)) = self.get_space(id) {
            Some(space)
        } else {
            None
        }
    }
    fn get_space(&self, id: LocId) -> Option<&Space> {
        self.spaces.get(&id)
    }
    pub fn loc_is_mem(&self, id: LocId) -> bool {
        match self.spaces.get(&id) {
            Some(Space::Memo(_)) => true,
            _ => false,
        }
    }
}

/// A single-cell message channel. The port-thread associated with this
/// dropbox waits here until a message is ready to tell them what to do.
#[derive(Debug)]
pub(crate) struct MsgDropbox {
    s: crossbeam::Sender<usize>,
    r: crossbeam::Receiver<usize>,
}
impl MsgDropbox {
    // Value chosen only for visibility during debug
    const NOTHING_MSG: usize = !0; // 0xffff...

    fn new() -> Self {
        let (s, r) = crossbeam::channel::bounded(1);
        Self { s, r }
    }

    #[inline]
    fn recv_timeout(&self, timeout: Duration) -> Option<usize> {
        self.r.recv_timeout(timeout).ok()
    }
    #[inline]
    fn recv(&self) -> usize {
        let msg = self.r.recv().unwrap();
        // println!("MSG {:b} rcvd!", msg);
        msg
    }
    #[inline]
    fn send(&self, msg: usize) {
        // println!("MSG {:b} sent!", msg);
        self.s.try_send(msg).expect("Msgbox was full!")
    }
    fn send_nothing(&self) {
        self.send(Self::NOTHING_MSG)
    }
    fn recv_nothing(&self) {
        let got = self.recv();
        assert_eq!(got, Self::NOTHING_MSG);
    }
}

/// The entire state of a single protocol instance. Usually only accessed via Arc.
pub struct ProtoAll {
    r: ProtoR,
    w: Mutex<ProtoW>,
}

/// Part of protocol Meta-state. Remembers that a Putter / Getter with this
/// ID has not yet been constructed for this proto.
#[derive(Debug, Copy, Clone)]
pub struct PortInfo {
    pub role: PortRole,
    pub type_id: TypeId,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PortRole {
    Putter,
    Getter,
}

/// Result of attempting to claim a given port Id from the protocol.
/// Fails if another putter/getter exists that has already claimed it.
pub enum ClaimResult<T: 'static> {
    GotGetter(Getter<T>),
    GotPutter(Putter<T>),
    NotUnclaimed,
    TypeMismatch,
}
impl<T: 'static> ClaimResult<T> {
    pub fn claimed_nothing(&self) -> bool {
        use ClaimResult::*;
        match self {
            GotGetter(_) | GotPutter(_) => false,
            NotUnclaimed | TypeMismatch => true,
        }
    }
}
impl<T: 'static> TryInto<Putter<T>> for ClaimResult<T> {
    type Error = bool;
    fn try_into(self) -> Result<Putter<T>, Self::Error> {
        use ClaimResult::*;
        match self {
            GotPutter(p) => Ok(p),
            GotGetter(_) => Err(true),
            NotUnclaimed | TypeMismatch => Err(true),
        }
    }
}
impl<T: 'static> TryInto<Getter<T>> for ClaimResult<T> {
    type Error = bool;
    fn try_into(self) -> Result<Getter<T>, Self::Error> {
        use ClaimResult::*;
        match self {
            GotPutter(_) => Err(true),
            GotGetter(g) => Ok(g),
            NotUnclaimed | TypeMismatch => Err(true),
        }
    }
}

/// Structure corresponding with one protocol rule at runtime
#[derive(Debug)]
struct RunRule {
    guard_ready: BitSet,
    guard_full: BitSet,
    guard_pred: Formula,
    assign_vals: BitSet,
    assign_mask: BitSet,
    actions: Vec<RunAction>,
}
impl RunRule {
    fn fire(&self, mut f: Firer) {
        for a in self.actions.iter() {
            use RunAction::*;
            match a {
                PortPut { putter, mg, pg } => f.port_to_locs(*putter, mg, pg),
                MemPut { putter, mg, pg } => f.mem_to_locs(*putter, mg, pg),
            }
        }
    }
}

/// Structure corresponing to one data-movement action (with 1 putter and N getters)
#[derive(Debug)]
enum RunAction {
    PortPut {
        putter: LocId,
        mg: Vec<LocId>,
        pg: Vec<LocId>,
    },
    MemPut {
        putter: LocId,
        mg: Vec<LocId>,
        pg: Vec<LocId>,
    },
}

pub(crate) struct PortCommon {
    p: Arc<ProtoAll>,
    id: LocId,
}

/// User-facing port-object with the role of "Getter" of type T.
pub struct Getter<T: 'static> {
    c: PortCommon,
    phantom: PhantomData<T>,
}
unsafe impl<T: 'static> Send for Getter<T> {}
unsafe impl<T: 'static> Sync for Getter<T> {}
impl<T: 'static> Getter<T> {
    const BAD_ID: &'static str = "My ID isn't associated with a valid getter!";

    /// combination of `get_signal` and `get_timeout`
    pub fn get_signal_timeout(&mut self, timeout: Duration) -> bool {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        po_ge.set_want_data(false);
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        po_ge
            .await_msg_timeout(&self.c.p, timeout, self.c.id)
            .is_some()
    }
    /// like `get`, but doesn't acquire any data. Useful for participation
    /// in synchrony when the data isn't useful.
    pub fn get_signal(&mut self) {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        po_ge.set_want_data(false);
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        po_ge.dropbox.recv_nothing()
    }
    /// like `get` but attempts to return with `None` if the provided duration
    /// elapses and there is not yet a protocol action which would supply
    /// this getter with data. Note, the call _may take longer than the duration_
    /// if the protocol initiates a data movement and other peers delay completion
    /// of the firing.
    pub fn get_timeout(&mut self, timeout: Duration) -> Option<T> {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        po_ge.set_want_data(true);
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let mut datum: MaybeUninit<T> = MaybeUninit::uninit();
        let out_ptr = unsafe { transmute(datum.as_mut_ptr()) };
        unsafe {
            let msg = po_ge.await_msg_timeout(&self.c.p, timeout, self.c.id)?;
            po_ge.participate_with_msg(&self.c.p, msg, out_ptr);
            Some(datum.assume_init())
        }
    }
    /// participates in a synchronous firing, acquiring data from some
    /// putter-peer in accordance with the protocol's definition
    pub fn get(&mut self) -> T {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        po_ge.set_want_data(true);
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let mut datum: MaybeUninit<T> = MaybeUninit::uninit();
        let out_ptr = unsafe { transmute(datum.as_mut_ptr()) };
        unsafe {
            po_ge.participate_with_msg(&self.c.p, po_ge.dropbox.recv(), out_ptr);
            datum.assume_init()
        }
    }
}
impl<T: 'static> Drop for Getter<T> {
    fn drop(&mut self) {
        self.c.p.w.lock().unclaimed_ports.insert(
            self.c.id,
            PortInfo {
                type_id: TypeId::of::<T>(),
                role: PortRole::Getter,
            },
        );
    }
}

/// Error code reporting the result of `Putter::put_timeout_lossy`.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PutTimeoutResult<T> {
    Timeout(T),
    Observed(T),
    Moved,
}

/// User-facing port-object with the role of "Putter" of type T.
pub struct Putter<T: 'static> {
    c: PortCommon,
    phantom: PhantomData<T>,
}
unsafe impl<T: 'static> Send for Putter<T> {}
unsafe impl<T: 'static> Sync for Putter<T> {}
impl<T: 'static> Putter<T> {
    const BAD_MSG: &'static str = "putter got a bad `num_movers_msg`";
    const BAD_ID: &'static str = "protocol doesn't recognize my role as putter!";

    /// Combination of `put_timeout` and `put_lossy`.
    pub fn put_timeout_lossy(&mut self, datum: T, timeout: Duration) -> PutTimeoutResult<()> {
        use PutTimeoutResult::*;
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        unsafe { po_pu.p.set_ptr(transmute(&datum)) };
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let num_movers_msg = match po_pu.await_msg_timeout(&self.c.p, timeout, self.c.id) {
            Some(msg) => msg,
            None => {
                drop(datum);
                return Timeout(());
            }
        };
        match num_movers_msg {
            0 => {
                drop(datum);
                Observed(())
            }
            1 => {
                std::mem::forget(datum);
                Moved
            }
            _ => panic!(Self::BAD_MSG),
        }
    }
    /// Like `put`, but attempts to return early (with `Some` variant) if no
    /// protocol action occurs that accesses the put-datum. Note, the call
    /// _may  take longer than the duration_ if the protocol initiates a data
    /// movement and other peers delay completion of the firing.
    pub fn put_timeout(&mut self, datum: T, timeout: Duration) -> PutTimeoutResult<T> {
        use PutTimeoutResult::*;
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        unsafe { po_pu.p.set_ptr(transmute(&datum)) };
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let num_movers_msg = match po_pu.dropbox.recv_timeout(timeout) {
            Some(msg) => msg,
            None => {
                if self.c.p.w.lock().active.ready.set_to(self.c.id, false) {
                    return Timeout(datum);
                } else {
                    po_pu.dropbox.recv()
                }
            }
        };
        match num_movers_msg {
            0 => Observed(datum),
            1 => {
                std::mem::forget(datum);
                Moved
            }
            _ => panic!(Self::BAD_MSG),
        }
    }
    /// Provide a data element for some getters to take according to the protocol
    /// definition. The datum is returned (as the `Some` variant) if the put-datum
    /// was observed by getters in a synchronous protocol rule, but not consumed
    /// by any getter.   
    pub fn put(&mut self, datum: T) -> Option<T> {
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        unsafe { po_pu.p.set_ptr(transmute(&datum)) };
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let num_movers_msg = po_pu.dropbox.recv();
        match num_movers_msg {
            0 => Some(datum),
            1 => {
                std::mem::forget(datum);
                None
            }
            _ => panic!(Self::BAD_MSG),
        }
    }
    /// This function mirrors the API of that of `put`, returning `Some` if the
    /// value was not consumed, but instead drops the datum in place.
    pub fn put_lossy(&mut self, datum: T) -> Option<()> {
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        unsafe { po_pu.p.set_ptr(transmute(&datum)) };
        self.c.p.w.lock().enter(&self.c.p.r, self.c.id);
        let num_movers_msg = po_pu.dropbox.recv();
        match num_movers_msg {
            0 => {
                drop(datum);
                Some(())
            }
            1 => {
                std::mem::forget(datum);
                None
            }
            _ => panic!(Self::BAD_MSG),
        }
    }
}
impl<T: 'static> Drop for Putter<T> {
    fn drop(&mut self) {
        self.c.p.w.lock().unclaimed_ports.insert(
            self.c.id,
            PortInfo {
                type_id: TypeId::of::<T>(),
                role: PortRole::Putter,
            },
        );
    }
}

/// Convenience structure. Contains behavior for actually executing a data-movement action.
pub struct Firer<'a> {
    r: &'a ProtoR,
    w: &'a mut ProtoActive,
}
impl<'a> Firer<'a> {
    fn release_sig_getters_count_getters(&self, getters: &[LocId]) -> usize {
        let mut count = 0;
        for &g in getters {
            let po_ge = self.r.get_po_ge(g).expect("bad id");
            if po_ge.get_want_data() {
                count += 1;
            } else {
                po_ge.dropbox.send_nothing()
            }
        }
        count
    }

    fn instruct_data_getters<F>(
        r: &ProtoR,
        po_ge: &[LocId],
        data_getters_count: usize,
        putter_id: LocId,
        space: &PutterSpace,
        cleanup: F,
    ) where
        F: FnOnce(),
    {
        // 3. instruct port-getters. delegate clearing putters to them (unless 0 getters)
        match data_getters_count {
            0 => cleanup(),
            1 => {
                // solo mover
                space.cloner_countdown.store(1, Ordering::SeqCst);
                let mut i = po_ge
                    .iter()
                    .filter(|&&g| r.get_po_ge(g).unwrap().get_want_data());
                let mover = *i.next().unwrap();
                assert_eq!(None, i.next());
                let msg = DataGetCase::OnlyMovers.include_in_msg(putter_id);
                r.send_to_getter(mover, msg);
            }
            n => {
                // no need to check if data is copy. GETTERS can determine that
                // themselves and act accordingly
                space.cloner_countdown.store(n, Ordering::SeqCst);
                for (is_first, &g) in po_ge
                    .iter()
                    .filter(|&&g| r.get_po_ge(g).unwrap().get_want_data())
                    .with_first()
                {
                    let msg = if is_first {
                        DataGetCase::BothYouMove
                    } else {
                        DataGetCase::BothYouClone
                    }
                    .include_in_msg(putter_id);
                    r.send_to_getter(g, msg);
                }
            }
        }
    }

    pub fn mem_to_locs(&mut self, me_pu: LocId, me_ge: &[LocId], po_ge: &[LocId]) {
        println!("mem_to_locs");
        let memo_space = self.r.get_me_pu(me_pu).expect("fewh");
        let tid = &memo_space.p.type_info.type_id;

        // 1. get data ptr of putter memcell.
        let src = memo_space.p.get_ptr();
        assert!(!src.is_null());
        println!("src is {:p}", src);

        // 1. duplicate ref to getter memcells
        let mut move_into_self = false;
        for g in me_ge.iter().cloned() {
            if g == me_pu {
                move_into_self = true;
            } else {
                let me_ge_space = self.r.get_me_pu(g).expect("gggg");
                assert_eq!(*tid, me_ge_space.p.type_info.type_id);
                me_ge_space.p.overwrite_null_ptr(src);
                self.w.ready.set_to(g, true); // PUTTER is ready
            }
        }

        // 2. release signal getters
        let data_getters_count = self.release_sig_getters_count_getters(po_ge);

        // 3. update refcounts
        let Firer { r, w } = self;
        let src_refs = w.mem_refs.get_mut(&src).expect("mem_to_locs BAD REFS?");
        assert!(*src_refs >= 1);
        *src_refs += me_ge.len();

        // 4. perform port moves
        Self::instruct_data_getters(r, po_ge, data_getters_count, me_pu, &memo_space.p, || {
            // cleanup function. invoked when there are 0 data-getters
            println!("PROTO MEM CLEANUP");
            if !move_into_self {
                memo_space.make_empty(w, true, me_pu);
            }
        });
    }

    pub fn port_to_locs(&mut self, po_pu: LocId, me_ge: &[LocId], po_ge: &[LocId]) {
        println!("port_to_locs");
        let po_pu_space = self.r.get_po_pu(po_pu).expect("ECH");

        // 1. port getters have move-priority
        let data_getters_count = self.release_sig_getters_count_getters(po_ge);
        // println!("::port_to_mem_and_ports| port_mover_id={:?}", port_mover_id);

        // 2. populate memory cells if necessary
        let mut me_ge_iter = me_ge.iter().cloned();
        if let Some(first_me_ge) = me_ge_iter.next() {
            let first_me_ge_space = self.r.get_me_pu(first_me_ge).expect("wfew");
            let type_info = &first_me_ge_space.p.type_info;
            let tid = type_info.type_id;

            // 3. move data into memcell
            let src = po_pu_space.p.get_ptr();
            let dest = unsafe {
                match data_getters_count {
                    0 => self.w.storage.move_in(src, type_info),
                    _ => self.w.storage.clone_in(src, type_info),
                }
            };
            let mut refcounts = 1;
            first_me_ge_space.p.overwrite_null_ptr(dest);
            self.w.ready.set_to(first_me_ge, true); // mem is ready for GET

            // 4. copy pointers to other memory cells (if any)
            for g in me_ge_iter {
                let me_ge_space = self.r.get_me_pu(g).expect("gggg");
                assert_eq!(tid, me_ge_space.p.type_info.type_id);

                me_ge_space.p.overwrite_null_ptr(dest);
                self.w.ready.set_to(g, true); // mem is ready for GET
                refcounts += 1;
            }
            let was = self.w.mem_refs.insert(dest, refcounts);
            assert!(was.is_none());
        }
        let Firer { r, .. } = self;
        Self::instruct_data_getters(r, po_ge, data_getters_count, po_pu, &po_pu_space.p, || {
            // cleanup function. invoked when there are 0 data-getters
            let mem_movers = if me_ge.is_empty() { 0 } else { 1 };
            po_pu_space.dropbox.send(mem_movers);
        });
    }
}

/// Enumeration that encodes one of four flags.
/// Not user-facing
/// Used by getters to determine how they need to collaborate (ie: must we wait for cloners? etc.)
/// Sent to getters encoded in the MsgDropbox message (using top two bits).
#[derive(Debug, Copy, Clone)]
pub(crate) enum DataGetCase {
    BothYouClone,
    BothYouMove,
    OnlyCloners,
    OnlyMovers,
}
impl DataGetCase {
    fn i_move(self) -> bool {
        use DataGetCase::*;
        match self {
            BothYouClone | OnlyCloners => false,
            BothYouMove | OnlyMovers => true,
        }
    }
    fn last_countdown(self) -> usize {
        use DataGetCase::*;
        match self {
            OnlyCloners | OnlyMovers => 1,
            BothYouClone | BothYouMove => 2,
        }
    }
    fn someone_moves(self) -> bool {
        use DataGetCase::*;
        match self {
            OnlyCloners => false,
            BothYouClone | OnlyMovers | BothYouMove => true,
        }
    }
    fn mover_must_wait(self) -> bool {
        use DataGetCase::*;
        match self {
            OnlyMovers => false,
            // OnlyCloners undefined anyway
            BothYouClone | BothYouMove | OnlyCloners => true,
        }
    }
    fn parse_msg(msg: usize) -> (Self, LocId) {
        // println!("... GOT {:b}", msg);
        use DataGetCase::*;
        let mask = 0b11 << 62;
        let case = match (msg & mask) >> 62 {
            0b00 => BothYouClone,
            0b01 => BothYouMove,
            0b10 => OnlyCloners,
            0b11 => OnlyMovers,
            _ => unreachable!(),
        };
        (case, msg & !mask)
    }
    fn include_in_msg(self, msg: usize) -> usize {
        use DataGetCase::*;
        // assert_eq!(msg & (0b11 << 62), 0);
        let x = match self {
            BothYouClone => 0b00,
            BothYouMove => 0b01,
            OnlyCloners => 0b10,
            OnlyMovers => 0b11,
        };
        msg | (x << 62)
    }
}
