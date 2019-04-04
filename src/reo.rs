use mio::{Ready, Registration, SetReadiness};
use std::{cell::UnsafeCell, mem, sync::Arc, time::Duration};

pub trait Component {
    fn run(&mut self);
}

/*

## Put / Get safety and state

the Shared object is only accessed by 0/1 putters and 0/1 getters.
The UnsafeCell is mutably referenced by two threads. how do we make this safe?
It all hinges on what the PUTTER is doing during a put() call:

<- more puts... [~~~~~~~BARRIER1~~~~~~BARRIER2!~~~~~~~] ...more puts-->
            ... A A A A A | B B B B B B | A A A A A A A ...

The system is designed that the PUTTER may modify the shared state during state A,
and the GETTER may read the shared state during state B.
The Getter keeps track of what the putter is doing by following along with
a local structure: "PutterState".

Before BARRIER1, the putter updates the unsafecell to point to the datum on their
local stackframe. Putter and Getter eventually pass Barrier1. In state B, putter does
not do anything meaningful, but immediately waits at barrier2.
Important! The putter cannot return while the Getter considers it to be in stateB!.

While in stateB, the getter is able to peek() at their leisure. Eventually,
the getter must call get(). This manifests as (unsafely!) copying the pointed
datum and then waiting at the barrier2. Now it's state A again.

Once waking up after barrier2, the putter forgets (unsafely!) the datum on their
stack before returning.

These two unsafe events are safe again in combination. During stateB, there may
exist two replicas of the datum, but it won't be used by the putter in this case,
simply forgotten once it wakes up.

time-->
Putter | D D D D
Getter |     D D D D D D

## Crashes
There cannot be a panic during the get operation itself, but in stateB, the
Getter _may_ be dropped (by panic or otherwise).
This is OK because a getter-panic will free the Putter,
who is able to distinguish a getter-crash from being correctly freed. In this case
the putter returns the datum to the put-caller, as the data never moved.

## Timing out
Putters may time out when waiting at Barrier1. If this occurs, they will exit
the put function. If the getter arrives later, they will get stuck at Barrier1
until the Putter puts again.

The getter, in this way, is able to _freeze_ the timeout-clock of the putter
by causing the putter to enter stateB. The user is thus responsible for any
Putter-delays caused by separating the first peek() from the get() or getter-drop.
The _freeze_ is also possible using a special peek that does not actually read
the data at all ("freeze").

If the data has not been inspected by the Getter, the getter is allowed to
call "thaw". This moves the state back to StateA (giving the putter another
chance to timeout.) Note that freezing and thawing has no meaningful effect
on a non-timeout put operation (simply moves between the two barriers for
no good reason).

## I/O Signals and avoiding blocking
Reo protocols must be ready to act on data operations on several ports at once.
It is not sufficient to block on them all sequentially with GET or PUT operations
until they are ready, as just one idle port would bring the system to a standstill.

This is achieved using the `mio` crate. Ultimately, multiple sources of events can
be associated with one concurrent queue using TOKENS. This library uses mio
for the purpose of tracking which get / put operations are READY (ie. will not block)
This is achieved by a putter signalling its peer getter with a READY as it enters.
(Similarly for its counterpart)

A protocol needs to have a way to know when a set of ports are ready to go and that they
won't ever become not-ready again. Operations that provide this guarantee for their ports
send ONE kind of signal to the port's peer. try_put is not such an operation.
Instead, it sends a distinguishable signal. Before committing to some operation,
the protocol will wait for all involved ports to become either CERTAINLY or
TENTATIVELY ready. In this case, it will traverse all ports that were marked as
TENTATIVE. It will call freeze() on them sequentially. If all succeed, then it
will proceed (all are now locked. it is as if all called PUT without timeout)
if just one fails, the protocol unfreezes them all and returns to checking events.
*/

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PutterwardSignal {
    AToB,
    BToAGot,
    BToAThaw,
}

pub struct Shared<T> {
    data: UnsafeCell<*mut T>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PutterState {
    StateA,
    StateBUnpeeked,
    StateBPeeked,
}
pub struct PortGetter<T> {
    shared: Arc<Shared<T>>,
    barrier: crossbeam::Sender<PutterwardSignal>,
    putter_state: PutterState,
    my_reg: Registration,
    peer_ready: SetReadiness,
}

impl<T> PortGetter<T> {
    pub fn reg(&self) -> &Registration {
        &self.my_reg
    }
}
impl<T> Getter<T> for PortGetter<T> {
    fn try_peek(&mut self, wait_duration: Option<Duration>) -> Result<&T, bool> {
        if self.putter_state == PutterState::StateA {
            /////// Barrier 1
            if let Some(dur) = wait_duration {
                use crossbeam::SendTimeoutError;
                match self.barrier.send_timeout(PutterwardSignal::AToB, dur) {
                    Ok(()) => {}
                    Err(SendTimeoutError::Disconnected(_)) => return Err(false),
                    Err(SendTimeoutError::Timeout(_)) => return Err(true),
                }
            } else {
                use crossbeam::TrySendError;
                match self.barrier.try_send(PutterwardSignal::AToB) {
                    Ok(()) => {}
                    Err(TrySendError::Disconnected(_)) => return Err(false),
                    Err(TrySendError::Full(_)) => return Err(true),
                }
            }
        }
        self.putter_state = PutterState::StateBPeeked;
        let datum: &T = unsafe {
            let r: *mut T = *self.shared.data.get();
            &*r
        };
        Ok(datum)
    }
    fn peek(&mut self) -> Result<&T, ()> {
        if self.putter_state == PutterState::StateA {
            /////// Barrier 1
            // TODO should peek send a signal?
            if self.barrier.send(PutterwardSignal::AToB).is_err() {
                return Err(());
            }
        }
        self.putter_state = PutterState::StateBPeeked;
        let datum: &T = unsafe {
            let r: *mut T = *self.shared.data.get();
            &*r
        };
        Ok(datum)
    }
    fn get(&mut self) -> Result<T, ()> {
        self.peer_ready.set_readiness(Ready::writable()).unwrap(); // CERTAIN PUT
        if self.putter_state == PutterState::StateA {
            /////// Barrier 1
            if self.barrier.send(PutterwardSignal::AToB).is_err() {
                return Err(());
            }
        }
        let datum = unsafe { mem::replace(&mut **self.shared.data.get(), mem::uninitialized()) };
        /////// Barrier 2
        let _ = self.barrier.send(PutterwardSignal::BToAGot);
        self.peer_ready.set_readiness(Ready::empty()).unwrap();
        self.putter_state = PutterState::StateA;
        Ok(datum)
    }
}

pub struct PortPutter<T> {
    shared: Arc<Shared<T>>,
    barrier: crossbeam::Receiver<PutterwardSignal>,
    my_reg: Registration,
    peer_ready: SetReadiness,
}

#[derive(Debug)]
pub enum TryPutErr<T> {
    PeerDropped(T),
    Timeout(T),
}

impl<T> PortPutter<T> {
    pub fn reg(&self) -> &Registration {
        &self.my_reg
    }
}
impl<T> Putter<T> for PortPutter<T> {
    fn put(&mut self, mut datum: T) -> Result<(), T> {
        let r: *mut T = &mut datum;
        unsafe { *self.shared.data.get() = r };
        self.peer_ready.set_readiness(Ready::writable()).unwrap(); // CERTAIN GET
        loop {
            /////// Barrier 1
            match self.barrier.recv() {
                // SIGNAL 1
                Ok(PutterwardSignal::AToB) => {}
                Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
                Err(crossbeam::RecvError) => return Err(datum),
            }
            /////// Barrier 2
            match self.barrier.recv() {
                Ok(PutterwardSignal::BToAGot) => {
                    self.peer_ready.set_readiness(Ready::empty()).unwrap();
                    mem::forget(datum);
                    return Ok(());
                }
                Ok(PutterwardSignal::BToAThaw) => {} // loop again
                Ok(wrong_signal) => {
                    self.peer_ready.set_readiness(Ready::empty()).unwrap();
                    panic!("Putter got wrong signal! {:?}", wrong_signal)
                }
                Err(crossbeam::RecvError) => {
                    self.peer_ready.set_readiness(Ready::empty()).unwrap();
                    return Err(datum);
                }
            }
        }
    }
    fn try_put(
        &mut self,
        mut datum: T,
        mut wait_duration: Option<Duration>,
    ) -> Result<(), TryPutErr<T>> {
        let start = std::time::Instant::now();
        let r: *mut T = &mut datum;
        unsafe { *self.shared.data.get() = r }; // set contents to datum on my stack
        loop {
            self.peer_ready.set_readiness(Ready::readable()).unwrap(); // tentative put
                                                                       /////// Barrier 1
            if let Some(dur) = wait_duration {
                use crossbeam::RecvTimeoutError;
                match self.barrier.recv_timeout(dur) {
                    Ok(PutterwardSignal::AToB) => {}
                    Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
                    Err(RecvTimeoutError::Timeout) => return Err(TryPutErr::Timeout(datum)),
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(TryPutErr::PeerDropped(datum));
                    }
                }
            } else {
                match self.barrier.recv() {
                    Ok(PutterwardSignal::AToB) => {}
                    Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
                    Err(crossbeam::RecvError) => return Err(TryPutErr::PeerDropped(datum)),
                }
            }
            /////// Barrier 2
            let res = self.barrier.recv();
            self.peer_ready.set_readiness(Ready::empty()).unwrap(); // tentative put
            match res {
                Ok(PutterwardSignal::BToAGot) => {
                    mem::forget(datum);
                    return Ok(());
                }
                Ok(PutterwardSignal::BToAThaw) => {
                    if let Some(dur) = wait_duration {
                        if let Some(to_wait) = dur.checked_sub(start.elapsed()) {
                            wait_duration = Some(to_wait)
                        } else {
                            return Err(TryPutErr::Timeout(datum));
                        }
                    }
                }
                Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
                Err(crossbeam::RecvError) => return Err(TryPutErr::PeerDropped(datum)),
            }
        }
    }
}

unsafe impl<T> Sync for PortPutter<T> {}
unsafe impl<T> Sync for PortGetter<T> {}
unsafe impl<T> Send for PortPutter<T> {}
unsafe impl<T> Send for PortGetter<T> {}
impl<T> Drop for PortGetter<T> {
    fn drop(&mut self) {
        self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer dead!
    }
}
impl<T> Drop for PortPutter<T> {
    fn drop(&mut self) {
        self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer dead!
    }
}

pub fn new_port<T>() -> (PortPutter<T>, PortGetter<T>) {
    let (g_reg, g_red) = mio::Registration::new2();
    let (p_reg, p_red) = mio::Registration::new2();
    let a_shared = Arc::new(Shared {
        data: UnsafeCell::new(std::ptr::null_mut()),
    });
    let (s, r) = crossbeam::channel::bounded(0);
    let p = PortPutter {
        shared: a_shared.clone(),
        barrier: r,
        my_reg: p_reg,
        peer_ready: g_red,
    };
    let g = PortGetter {
        shared: a_shared,
        barrier: s,
        putter_state: PutterState::StateA,
        my_reg: g_reg,
        peer_ready: p_red,
    };
    (p, g)
}

#[derive(Debug, Copy, Clone)]
pub enum FreezeOutcome {
    Frozen,
    PeerNotWaiting,
    PeerDropped,
    PutterCommitted,
}

pub trait Freezer {
    // attempts to freeze a waiting try_put on the putter's side
    // returns Ok(_) if the value
    fn freeze(&mut self) -> FreezeOutcome;

    // only execute AFTER successful freeze where no peek has been performed
    // blocks until the putter receives the signal
    fn thaw(&mut self);
}
impl<T> Freezer for PortGetter<T> {
    fn freeze(&mut self) -> FreezeOutcome {
        use crossbeam::channel::TrySendError;
        match self.putter_state {
            PutterState::StateA => match self.barrier.try_send(PutterwardSignal::AToB) {
                Ok(()) => {
                    self.putter_state = PutterState::StateBUnpeeked;
                    FreezeOutcome::Frozen
                }
                Err(TrySendError::Full(_)) => FreezeOutcome::PeerNotWaiting,
                Err(TrySendError::Disconnected(_)) => FreezeOutcome::PeerDropped,
            },
            PutterState::StateBPeeked => FreezeOutcome::PutterCommitted,
            PutterState::StateBUnpeeked => FreezeOutcome::Frozen,
        }
    }
    fn thaw(&mut self) {
        match self.putter_state {
            PutterState::StateBUnpeeked => {}
            wrong_state => panic!("tried to release putter in state {:?}", wrong_state),
        }
        let _ = self.barrier.send(PutterwardSignal::BToAThaw); // either way no problem
        self.putter_state = PutterState::StateA;
    }
}

struct EventedTup {
    reg: Registration,
    ready: SetReadiness,
}
impl Default for EventedTup {
    fn default() -> Self {
        let (reg, ready) = mio::Registration::new2();
        Self { reg, ready }
    }
}

#[derive(Default)]
pub struct Memory<T> {
    shutdown: bool,
    data: Option<T>,
    full: EventedTup,
    empty: EventedTup,
}
impl<T> Getter<T> for Memory<T> {
    fn get(&mut self) -> Result<T, ()> {
        match self.data.take() {
            Some(x) => {
                self.update_ready();
                Ok(x)
            }
            None => Err(()),
        }
    }
    fn try_peek(&mut self, _wait_duration: Option<Duration>) -> Result<&T, bool> {
        self.peek().map_err(|_| false)
    }
    fn peek(&mut self) -> Result<&T, ()> {
        if self.shutdown {
            return Err(());
        }
        match self.data.as_ref() {
            Some(x) => Ok(x),
            None => Err(()),
        }
    }
}
impl<T> Memory<T> {
    pub fn shutdown(&mut self) {
        if !self.shutdown {
            self.shutdown = true;
            let _ = self.empty.ready.set_readiness(Ready::writable());
            let _ = self.full.ready.set_readiness(Ready::writable());
        }
    }
    pub fn reg_g(&self) -> impl AsRef<Registration> + '_ {
        RegHandle {
            reg: &self.full.reg,
            when_dropped: move || self.update_ready(),
        }
    }
    pub fn reg_p(&self) -> impl AsRef<Registration> + '_ {
        RegHandle {
            reg: &self.empty.reg,
            when_dropped: move || self.update_ready(),
        }
    }
    pub fn update_ready(&self) {
        if self.data.is_none() {
            let _ = self.empty.ready.set_readiness(Ready::writable());
            let _ = self.full.ready.set_readiness(Ready::empty());
        } else {
            let _ = self.empty.ready.set_readiness(Ready::empty());
            let _ = self.full.ready.set_readiness(Ready::writable());
        }
    }
}
impl<T> Putter<T> for Memory<T> {
    fn put(&mut self, datum: T) -> Result<(), T> {
        if self.shutdown {
            return Err(datum);
        }
        match self.data.replace(datum) {
            None => {
                self.update_ready();
                Ok(())
            }
            Some(x) => Err(x),
        }
    }
    fn try_put(&mut self, datum: T, _wait_duration: Option<Duration>) -> Result<(), TryPutErr<T>> {
        self.put(datum).map_err(|t| TryPutErr::PeerDropped(t))
    }
}

struct RegHandle<'a, F>
where
    F: Fn(),
{
    reg: &'a Registration,
    when_dropped: F,
}
impl<'a, F> Drop for RegHandle<'a, F>
where
    F: Fn(),
{
    fn drop(&mut self) {
        (self.when_dropped)()
    }
}
impl<'a, F> AsRef<Registration> for RegHandle<'a, F>
where
    F: Fn(),
{
    fn as_ref(&self) -> &Registration {
        &self.reg
    }
}

pub trait Putter<T> {
    fn put(&mut self, datum: T) -> Result<(), T>;
    fn try_put(&mut self, datum: T, wait_duration: Option<Duration>) -> Result<(), TryPutErr<T>>;
}

pub trait Getter<T> {
    fn try_peek(&mut self, wait_duration: Option<Duration>) -> Result<&T, bool>;
    fn peek(&mut self) -> Result<&T, ()>;
    fn get(&mut self) -> Result<T, ()>;
}
