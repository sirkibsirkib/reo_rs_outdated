use crate::proto::memory::Storage;
use itertools::izip;
use smallvec::SmallVec;

pub mod definition;
mod memory;
use definition::{Formula, LocKind, ProtoBuildErr, ProtoBuilder, Term, TypelessProtoDef};

pub mod reflection;
use reflection::TypeInfo;

pub mod traits;
use traits::{
    DataSource, HasMsgDropBox, HasUnclaimedPorts, MaybeClone, MaybeCopy, MaybePartialEq, Proto,
};

#[cfg(test)]
mod tests;

pub mod groups;

use crate::{
    bitset::BitSet,
    tokens::{decimal::Decimal, Grouped},
    LocId, ProtoHandle,
};
use hashbrown::HashMap;
use parking_lot::{Mutex, MutexGuard};
use std::{
    alloc::{self, Layout},
    any::TypeId,
    convert::TryInto,
    fmt::Debug,
    marker::PhantomData,
    mem::{transmute, MaybeUninit},
    str::FromStr,
    sync::{
        atomic::{AtomicPtr, AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use std_semaphore::Semaphore;

#[derive(Debug, Default)]
struct MoveFlags {
    move_flags: AtomicU8,
}
impl MoveFlags {
    const MOVE_FLAG_MOVED: u8 = 0b01;
    const MOVE_FLAG_DISABLED: u8 = 0b10;

    #[inline]
    fn type_is_copy_i_moved(&self) {
        self.move_flags
            .store(Self::MOVE_FLAG_MOVED, Ordering::SeqCst);
    }

    #[inline]
    fn did_someone_move(&self) -> bool {
        let x: u8 = self.move_flags.load(Ordering::SeqCst);
        x & Self::MOVE_FLAG_MOVED != 0 && x & Self::MOVE_FLAG_DISABLED == 0
    }

    #[inline]
    fn ask_for_move_permission(&self) -> bool {
        0 == self
            .move_flags
            .fetch_or(Self::MOVE_FLAG_MOVED, Ordering::SeqCst)
    }

    #[inline]
    fn reset(&self, move_enabled: bool) {
        let val = if move_enabled {
            Self::MOVE_FLAG_DISABLED
        } else {
            0
        };
        self.move_flags.store(val, Ordering::SeqCst);
    }
}

/// A coordination point that getters interact with to acquire a datum.
/// Common to memory and port putters.
#[derive(debug_stub_derive::DebugStub)]
pub(crate) struct PutterSpace {
    ptr: AtomicPtr<u8>,
    cloner_countdown: AtomicUsize,
    move_flags: MoveFlags,
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
            move_flags: MoveFlags::default(),
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

/// Special instance of a memory space.
/// differs from memory space in some ways:
/// 1. only acts as putter or getter in ONE rule
///   - normal memory cells are kept in consistent state by relying on
///     becoming UNREADY when its fired, and only again explicitly when someone finished it. See DataSource::finalize
///   - this memory cell is kept in consistent state by relying on the readiness of others:
///     either the coordinator itself OR 1+ getters involved in the ONE RULE must still be unready before it can be fired again
///   - invariant between firings of its one rule: this memory cell is EMPTY
/// 2. filled explicitly by the coordinator. emptied by coordinator or last getter as usual
#[derive(Debug)]
struct TempSpace(MemoSpace);
impl TempSpace {
    fn new(type_info: Arc<TypeInfo>) -> Self {
        Self(MemoSpace::new(std::ptr::null_mut(), type_info))
    }
}

/// Personal coordination space for this getter to receive messages and advertise
/// whether it called get() or get_signal().
#[derive(Debug)]
struct PoGeSpace {
    dropbox: MsgDropbox, // used only by this guy to recv messages
}
impl PoGeSpace {
    fn new() -> Self {
        Self {
            dropbox: MsgDropbox::new(),
        }
    }
    unsafe fn get_data(&self, a: &ProtoAll, putter_id: LocId, out_ptr: *mut u8) {
        // let (_case, putter_id) = DataGetCase::parse_msg(msg);
        match a.r.get_space(putter_id) {
            Some(Space::Memo(space)) => {
                space.acquire_data([out_ptr].iter().copied(), (a, putter_id))
            }
            Some(Space::PoPu(space)) => space.acquire_data([out_ptr].iter().copied(), ()),
            _ => panic!("Bad putter ID!!"),
        }
    }
    unsafe fn get_signal(&self, a: &ProtoAll, putter_id: LocId) {
        match a.r.get_space(putter_id) {
            Some(Space::Memo(space)) => space.acquire_data(std::iter::empty(), (a, putter_id)),
            Some(Space::PoPu(space)) => space.acquire_data(std::iter::empty(), ()),
            _ => panic!("Bad putter ID!!"),
        }
    }
}

// unsafe impl are safe. autoderive inhibited by HashMap<*mut u8, ..> but the
// pointers are only used as keys (not accessed) in this context.
unsafe impl Send for ProtoActive {}
unsafe impl Sync for ProtoActive {}

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

enum EvalTerm {
    True,
    False,
    Var(LocId),
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
    fn ready_set_coordinate(&mut self, r: &ProtoR, my_id: LocId) {
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
                if bits_ready {
                    // TODO
                    unsafe { self.build_temps(r, rule) };
                    let guard_pass = unsafe { r.eval_formula(&rule.guard_pred, self) };
                    if !guard_pass {
                        unsafe { self.unbuild_temps(r, rule) };
                        continue;
                    }

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
    #[inline]
    unsafe fn build_temps(&mut self, r: &ProtoR, rule: &RunRule) {
        for t in rule.temp_mems.iter() {
            let putter_space = r
                .get_temp(t.temp_mem_loc_id)
                .expect("NOT TEMP??")
                .my_space();
            let dest = self.active.storage.alloc(&putter_space.type_info);
            putter_space.overwrite_null_ptr(dest);
            use TempRuleFunc::*;
            match &t.func {
                Arity0 { func, .. } => func(dest),
                Arity1 { func, args } => func(dest, r.eval_term(&args[0], self)),
                Arity2 { func, args } => func(
                    dest,
                    r.eval_term(&args[0], self),
                    r.eval_term(&args[1], self),
                ),
                Arity3 { func, args } => func(
                    dest,
                    r.eval_term(&args[0], self),
                    r.eval_term(&args[1], self),
                    r.eval_term(&args[2], self),
                ),
            }
            let was = self.active.mem_refs.insert(dest, 1);
            assert!(was.is_none());
            println!(
                "COMPUTED TEMP VAL. INSERTED INTO TEMP SLOT {}",
                t.temp_mem_loc_id
            );
        }
    }
    #[inline]
    unsafe fn unbuild_temps(&mut self, r: &ProtoR, rule: &RunRule) {
        for t in rule.temp_mems.iter() {
            let dest = r.get_temp(t.temp_mem_loc_id).expect("NOT TEMP??");
            dest.0.make_empty(&mut self.active, true, t.temp_mem_loc_id);
            println!("DROPPING TEMP VAL IN SLOT {}", t.temp_mem_loc_id);
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
    Temp(TempSpace),
    Unused,
}

/// Part of the protocol NOT protected by the lock
pub struct ProtoR {
    rules: Vec<RunRule>,
    spaces: Vec<Space>,
}
impl ProtoR {
    unsafe fn eval_formula(&self, formula: &Formula, w: &ProtoW) -> bool {
        use definition::Formula::*;
        let f = |q: &Formula| self.eval_formula(q, w);
        match formula {
            True => true,
            None(x) => !x.iter().any(f),
            And(x) => !x.iter().all(f),
            Or(x) => x.iter().any(f),
            ValueEq(a, b) => {
                let aptr = self.eval_term(a, w);
                let bptr = self.eval_term(b, w);
                self.equal_put_data(*a, *b);
                // cannot check equality anymore
                i1.funcs.partial_eq.execute(p1, p2)
            },
            MemIsNull(a) => !w.memory_bits.test(*a),
            FuncDeclaration { .. } => panic!("TEMP. NOT ALLOWED AT RUNTIME"),
        }
    }
    unsafe fn eval_term(&self, term: &Term, w: &ProtoW) -> *mut u8 {
        match term {
            Term::Boolean(f) => {
                if self.eval_formula(f, w) {
                    std::mem::transmute((&true) as *const bool)
                } else {
                    std::mem::transmute((&false) as *const bool)
                }
            }
            Term::Value(loc_id) => self
                .get_space_putter(*loc_id)
                .expect("NOT PUTTER")
                .get_ptr(),
        }
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
    fn get_temp(&self, id: LocId) -> Option<&TempSpace> {
        if let Some(Space::Temp(space)) = self.get_space(id) {
            Some(space)
        } else {
            None
        }
    }
    fn get_space(&self, id: LocId) -> Option<&Space> {
        self.spaces.get(id)
    }
    fn get_space_putter(&self, id: LocId) -> Option<&PutterSpace> {
        use Space::*;
        Some(match self.get_space(id)? {
            PoPu(p) => p.my_space(),
            Memo(p) => p.my_space(),
            Temp(p) => p.my_space(),
            _ => return None,
        })
    }
    pub fn loc_is_mem(&self, id: LocId) -> bool {
        match self.spaces.get(id) {
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
        msg
    }
    #[inline]
    fn send(&self, msg: usize) {
        self.s.try_send(msg).expect("Msgbox was full!")
    }
    fn send_nothing(&self) {
        self.send(Self::NOTHING_MSG)
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

#[derive(Debug)]
enum TempRuleFunc {
    // first arg is destination
    Arity0 {
        func: fn(*mut u8),
    },
    Arity1 {
        func: fn(*mut u8, *mut u8),
        args: [Term; 1],
    },
    Arity2 {
        func: fn(*mut u8, *mut u8, *mut u8),
        args: [Term; 2],
    },
    Arity3 {
        func: fn(*mut u8, *mut u8, *mut u8, *mut u8),
        args: [Term; 3],
    },
}
#[derive(Debug)]
struct TempMemRunnable {
    temp_mem_loc_id: LocId,
    func: TempRuleFunc,
}

/// Structure corresponding with one protocol rule at runtime
/// for every t in temp_mems assumes:
/// 1. t.temp_mem_loc_id is not used by ANY OTHER RULE
/// 2. t.func is well-formed: its function populates the correct type, reading the correct types
#[derive(Debug)]
struct RunRule {
    guard_ready: BitSet,
    guard_full: BitSet,

    temp_mems: Vec<TempMemRunnable>,
    guard_pred: Formula,

    assign_vals: BitSet,
    assign_mask: BitSet,
    actions: Vec<RunAction>,
}
impl RunRule {
    #[inline]
    fn fire(&self, mut f: Firer) {
        for a in self.actions.iter() {
            f.perform_action(a.putter, &a.mg, &a.pg)
        }
    }
}

/// Structure corresponing to one data-perform_action action (with 1 putter and N getters)
#[derive(Debug)]
struct RunAction {
    putter: LocId,
    mg: SmallVec<[LocId; 4]>,
    pg: SmallVec<[LocId; 4]>,
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
impl<T: 'static> Getter<T> {
    const BAD_ID: &'static str = "My ID isn't associated with a valid getter!";

    pub fn proto_handle(&self) -> &Arc<ProtoAll> {
        &self.c.p
    }

    /// combination of `get_signal` and `get_timeout`
    pub fn get_signal_timeout(&mut self, timeout: Duration) -> bool {
        unsafe { self.get_signal_in_place_timeout(timeout) }
    }
    /// like `get`, but doesn't acquire any data. Useful for participation
    /// in synchrony when the data isn't useful.
    pub fn get_signal(&mut self) {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        unsafe { po_ge.get_signal(&self.c.p, po_ge.dropbox.recv()) }
    }
    /// like `get` but attempts to return with `None` if the provided duration
    /// elapses and there is not yet a protocol action which would supply
    /// this getter with data. Note, the call _may take longer than the duration_
    /// if the protocol initiates a data perform_action and other peers delay completion
    /// of the firing.
    pub fn get_timeout(&mut self, timeout: Duration) -> Option<T> {
        let mut datum: MaybeUninit<T> = MaybeUninit::uninit();
        unsafe {
            match self.get_in_place_timeout(datum.as_mut_ptr(), timeout) {
                true => Some(datum.assume_init()),
                false => None,
            }
        }
    }

    /// Safety: `dest` is uninitialized at first.
    /// on return: `dest` is initialized.
    pub unsafe fn get_in_place(&mut self, dest: *mut T) {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        // po_ge.set_want_data(true);
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        po_ge.get_data(&self.c.p, po_ge.dropbox.recv(), transmute(dest));
    }

    /// Safety: `dest` is uninitialized at first.
    /// on return: `dest` is initialized iff `true` was returned.
    pub unsafe fn get_in_place_timeout(&mut self, dest: *mut T, timeout: Duration) -> bool {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        // po_ge.set_want_data(true);
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        match po_ge.await_msg_timeout(&self.c.p, timeout, self.c.id) {
            Some(msg) => {
                po_ge.get_data(&self.c.p, msg, transmute(dest));
                true
            }
            None => false,
        }
    }

    pub unsafe fn get_signal_in_place_timeout(&mut self, timeout: Duration) -> bool {
        let po_ge = self.c.p.r.get_po_ge(self.c.id).expect(Self::BAD_ID);
        // po_ge.set_want_data(true);
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        match po_ge.await_msg_timeout(&self.c.p, timeout, self.c.id) {
            Some(msg) => {
                po_ge.get_signal(&self.c.p, msg);
                true
            }
            None => false,
        }
    }

    /// participates in a synchronous firing, acquiring data from some
    /// putter-peer in accordance with the protocol's definition
    pub fn get(&mut self) -> T {
        let mut datum: MaybeUninit<T> = MaybeUninit::uninit();
        unsafe {
            self.get_in_place(datum.as_mut_ptr());
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
impl<T> Into<Result<(), (T, bool)>> for PutTimeoutResult<T> {
    fn into(self) -> Result<(), (T, bool)> {
        use PutTimeoutResult::*;
        match self {
            Timeout(t) => Err((t, false)),
            Observed(t) => Err((t, true)),
            Moved => Ok(()),
        }
    }
}
impl<T> PutTimeoutResult<T> {
    pub fn moved(&self) -> bool {
        use PutTimeoutResult::*;
        match self {
            Timeout(_) | Observed(_) => false,
            Moved => true,
        }
    }
}

/// User-facing port-object with the role of "Putter" of type T.
pub struct Putter<T: 'static> {
    c: PortCommon,
    phantom: PhantomData<T>,
}
impl<T: 'static> Putter<T> {
    const BAD_MSG: &'static str = "putter got a bad `num_movers_msg`";
    const BAD_ID: &'static str = "protocol doesn't recognize my role as putter!";

    pub fn proto_handle(&self) -> &Arc<ProtoAll> {
        &self.c.p
    }

    /// Combination of `put_timeout` and `put_lossy`.
    pub fn put_timeout_lossy(&mut self, mut datum: T, timeout: Duration) -> PutTimeoutResult<()> {
        use PutTimeoutResult::*;
        unsafe {
            match self.put_in_place_timeout(&mut datum, timeout) {
                Moved => {
                    std::mem::forget(datum);
                    Moved
                }
                Timeout(()) => Timeout(()),
                Observed(()) => Observed(()),
            }
        }
    }
    /// Like `put`, but attempts to return early (with `Some` variant) if no
    /// protocol action occurs that accesses the put-datum. Note, the call
    /// _may  take longer than the duration_ if the protocol initiates a data
    /// perform_action and other peers delay completion of the firing.
    pub fn put_timeout(&mut self, mut datum: T, timeout: Duration) -> PutTimeoutResult<T> {
        use PutTimeoutResult::*;
        unsafe {
            match self.put_in_place_timeout(&mut datum, timeout) {
                Timeout(()) => Timeout(datum),
                Observed(()) => Observed(datum),
                Moved => Moved,
            }
        }
    }

    /// Safety: `src` is initialized at first.
    /// on return: `src` was moved IFF `true` was returned.
    pub unsafe fn put_in_place(&mut self, src: *mut T) -> bool {
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        po_pu.p.set_ptr(transmute(src));
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        let num_movers_msg = po_pu.dropbox.recv();
        match num_movers_msg {
            0 => false,
            1 => true,
            _ => panic!(Self::BAD_MSG),
        }
    }

    /// Safety: `src` is initialized at first.
    /// on return: `src` was moved IFF `Moved` was returned.
    pub unsafe fn put_in_place_timeout(
        &mut self,
        src: *mut T,
        timeout: Duration,
    ) -> PutTimeoutResult<()> {
        use PutTimeoutResult::*;
        let po_pu = self.c.p.r.get_po_pu(self.c.id).expect(Self::BAD_ID);
        po_pu.p.set_ptr(transmute(src));
        self.c
            .p
            .w
            .lock()
            .ready_set_coordinate(&self.c.p.r, self.c.id);
        let num_movers_msg = match po_pu.dropbox.recv_timeout(timeout) {
            Some(msg) => msg,
            None => {
                if self.c.p.w.lock().active.ready.set_to(self.c.id, false) {
                    return Timeout(());
                } else {
                    po_pu.dropbox.recv()
                }
            }
        };
        match num_movers_msg {
            0 => Observed(()),
            1 => Moved,
            _ => panic!(Self::BAD_MSG),
        }
    }

    /// Provide a data element for some getters to take according to the protocol
    /// definition. The datum is returned (as the `Some` variant) if the put-datum
    /// was observed by getters in a synchronous protocol rule, but not consumed
    /// by any getter.   
    pub fn put(&mut self, mut datum: T) -> Option<T> {
        unsafe {
            if self.put_in_place(&mut datum) {
                std::mem::forget(datum);
                None
            } else {
                Some(datum)
            }
        }
    }
    /// This function mirrors the API of that of `put`, returning `Some` if the
    /// value was not consumed, but instead drops the datum in place.
    pub fn put_lossy(&mut self, mut datum: T) -> Option<()> {
        unsafe {
            if self.put_in_place(&mut datum) {
                std::mem::forget(datum);
                None
            } else {
                drop(datum); // for readability
                Some(())
            }
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

/// Convenience structure. Contains behavior for actually executing a data-perform_action action.
pub struct Firer<'a> {
    r: &'a ProtoR,
    w: &'a mut ProtoActive,
}
impl<'a> Firer<'a> {
    pub fn perform_action(&mut self, putter: LocId, me_ge: &[LocId], po_ge: &[LocId]) {
        let space = self.r.get_space(putter);
        let (putter_space, mem_putter): (&PutterSpace, bool) = match space {
            Some(Space::PoPu(space)) => (&space.p, false),
            Some(Space::Memo(space)) | Some(Space::Temp(TempSpace(space))) => (&space.p, true),
            _ => panic!("Not a putter!"),
        };
        let src = putter_space.get_ptr();
        let tid = putter_space.type_info.type_id;

        let mut move_into_self = false;
        let disable_move = if mem_putter {
            // 1. duplicate ref to getter memcells
            for g in me_ge.iter().cloned() {
                if g == putter {
                    move_into_self = true;
                } else {
                    let me_ge_space = self.r.get_me_pu(g).expect("gggg");
                    assert_eq!(tid, me_ge_space.p.type_info.type_id);
                    me_ge_space.p.overwrite_null_ptr(src);
                    self.w.ready.set_to(g, true); // PUTTER is ready
                }
            }
            // 3. update refcounts
            let src_refs = self
                .w
                .mem_refs
                .get_mut(&src)
                .expect("mem_to_locs BAD REFS?");
            assert!(*src_refs >= 1);
            *src_refs += me_ge.len();
            *src_refs != 1
        } else {
            // 2. populate memory cells if necessary
            let mut me_ge_iter = me_ge.iter().cloned();
            if let Some(first_me_ge) = me_ge_iter.next() {
                let first_me_ge_space = self.r.get_me_pu(first_me_ge).expect("wfew");
                let type_info = &first_me_ge_space.p.type_info;
                let tid = type_info.type_id;

                // 3. move data into memcell
                let dest = unsafe {
                    match po_ge.len() {
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
            false
        };

        // 4. perform port moves
        if po_ge.len() == 0 {
            println!("PROTO MEM CLEANUP");
            match space.unwrap() {
                Space::PoPu(space) => {
                    let mem_movers = if me_ge.is_empty() { 0 } else { 1 };
                    space.dropbox.send(mem_movers);
                }
                Space::Memo(space) | Space::Temp(TempSpace(space)) => {
                    if !move_into_self {
                        space.make_empty(self.w, true, putter);
                    }
                }
                _ => unreachable!(),
            };
        } else {
            putter_space
                .cloner_countdown
                .store(po_ge.len(), Ordering::SeqCst);
            // move disabled only for memory cells with 2+ refcounts
            putter_space.move_flags.reset(!disable_move);
            for g in po_ge.iter().copied() {
                self.r.send_to_getter(g, putter);
            }
        }
    }
}
