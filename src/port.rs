use std::time::Duration;
use hashbrown::HashSet;
use crossbeam::{Receiver, Sender};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::mem::{self, ManuallyDrop};
use parking_lot::RwLock;

// both port-halves SHARE this on the heap

struct Notifier {
    sender: Sender<PortEvent>,
    token: usize,
}
#[derive(Default)]
struct InnerUpdater {
    update_putter: Option<Notifier>,
    update_getter: Option<Notifier>,
}
struct Inner<T> {
    t: ManuallyDrop<T>,
    occupied: AtomicBool,
    refs: AtomicUsize,
    updater: RwLock<InnerUpdater>,
}

// each port-half has one of these on the stack
/* USE CAFEULLY: Assumptions:
- 0/1 putters share the Inner
- 0/1 getters share the Inner
- inner.refs == sum of #getters + #putters
- Box is dropped if dropped and refs==1. contents are then dropped if inner.occuped == 1
*/
struct Shared<T> {
    inner: ManuallyDrop<Box<Inner<T>>>,
}
impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        println!("DROPPING SHARED");
        if self.inner.refs.fetch_sub(1, Ordering::Relaxed) == 1 {
            if self.inner.occupied.load(Ordering::Relaxed) {
                println!("DROP BOX WITH T (T is SOME)");
                unsafe { ManuallyDrop::drop(&mut self.inner.t); }
            } else {
                println!("DROP BOX WITHOUT T (T is NONE)");
            }
            unsafe { ManuallyDrop::drop(&mut self.inner); }
        }
    }
}

pub fn new_port<T>() -> (PutPort<T>, GetPort<T>) {
    let inner_box = Box::new(Inner {
        t: unsafe { mem::uninitialized() },
        refs: AtomicUsize::from(2),
        occupied: AtomicBool::from(false),
        updater: RwLock::new(InnerUpdater::default()),
    });
    let inner = Box::into_raw(inner_box);
    let [inner1, inner2] = unsafe {
        [
            ManuallyDrop::new(Box::from_raw(inner)),
            ManuallyDrop::new(Box::from_raw(inner)),
        ]
    };
    let shared1 = Shared { inner: inner1 };
    let shared2 = Shared { inner: inner2 };
    (PutPort { shared: shared1 }, GetPort { shared: shared2, know_occupied: false })
}

///////////
use std::ops::DerefMut;
pub struct PutPort<T> {
    shared: Shared<T>,
}
impl<T> PutPort<T> {
    pub fn put(&mut self, datum: T) {
        let inner: &mut Inner<T> = &mut self.shared.inner;
        let Inner {
            updater, occupied, t, ..
        } = inner;
        let r_lock = updater.read();
        let was = occupied.swap(true, Ordering::Relaxed);
        let mut old = mem::replace(t, ManuallyDrop::new(datum));
        if let Some(ref notifier) = r_lock.update_getter {
            let e = PortEvent::Put(notifier.token);
            println!("putter notifying {:?}", &e);
            notifier.sender.send(e).expect("PUT OK?");
        }
        if was {
            println!("PUT WAS SOMETHING");
            unsafe { ManuallyDrop::drop(&mut old) };
        } else {
            println!("PUT WASNT SOMETHING");
        }
    }
}
impl<T> Drop for PutPort<T> {
    fn drop(&mut self) {
        println!("putterdrop");
        if let Some(ref notifier) = self.shared.inner.updater.read().update_getter {
            let e = PortEvent::Dropped(notifier.token);
            println!("putter notifying with {:?}", &e);
            let _ = notifier.sender.send(e);
        }
    }
}

pub struct GetPort<T> {
    shared: Shared<T>,
    know_occupied: bool,
}
impl<T> GetPort<T> {
    pub fn get(&mut self) -> Option<T> {
        self.know_occupied = false;
        let inner: &mut Inner<T> = &mut self.shared.inner;
        let Inner {
            updater, occupied, t, ..
        } = inner;
        let r_lock = updater.read();
        let was = occupied.swap(false, Ordering::Relaxed);
        if let Some(ref notifier) = r_lock.update_putter {
            let e = PortEvent::Get(notifier.token);
            println!("GET will try send e {:?}", &e);
            notifier.sender.send(e).unwrap();
        }
        if was {
            println!("GET WAS SOMETHING");
            let mut ret: ManuallyDrop<T> = ManuallyDrop::new(unsafe { mem::uninitialized() });
            mem::swap(t, &mut ret);
            Some(ManuallyDrop::into_inner(ret))
        } else {
            println!("GET WASNT SOMETHING");
            None
        }
    }
    pub fn peek(&mut self) -> Option<&T> {
        if !self.know_occupied {
            self.know_occupied = self.shared.inner.occupied.load(Ordering::Relaxed);
        } 
        if self.know_occupied {
            Some(&self.shared.inner.t)
        } else {
            None
        }
    }
    pub fn register_with(&mut self, sel: &mut Selector, token: usize) -> Result<(),TokenError> {
        // TODO
        let mut w_lock = self.shared.inner.updater.write();
        println!("registering");
        if let Some(ref mut prev_update_getter) = w_lock.update_getter {
            println!("prev putter");
            let e = PortEvent::DeregisteredGet(prev_update_getter.token);
            let r = prev_update_getter.sender.send(e);
            println!("{:?}",r );
        } else {
            println!("no prev putter");
        }
        w_lock.update_getter = Some(Notifier {
            sender: sel.sender_proto.clone(),
            token,
        });
        Ok(())
    }
}
impl<T> Drop for GetPort<T> {
    fn drop(&mut self) {
        println!("getterdrop");
        if let Some(ref notifier) = self.shared.inner.updater.read().update_putter {
            let e = PortEvent::Dropped(notifier.token);
            println!("getter notifying with {:?}", &e);
            let _ = notifier.sender.send(e);
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TokenError;

#[derive(Debug, Copy, Clone)]
pub enum PortEvent {
    // sent whenever Some(Notifier) is overwritten with new registration
    // OR explicitly deregistered
    DeregisteredPut(usize),
    DeregisteredGet(usize),
    Get(usize),
    Put(usize),
    Dropped(usize)
}

pub struct Selector {
    assigned_toks: HashSet<usize>,
    toks_waiting: Receiver<PortEvent>,
    sender_proto: Sender<PortEvent>,
}
impl Default for Selector {
    fn default() -> Self {
        let (s,r) = crossbeam::channel::bounded(10);
        Self {
            // TODO
            toks_waiting: r,
            sender_proto: s,
            assigned_toks: Default::default(),
        }
    }
}
impl Selector {
    pub fn wait(&mut self) -> PortEvent {
        self.toks_waiting.recv().expect("???")
    }
    pub fn wait_timeout(&mut self, dur: Duration) -> Option<PortEvent> {
        match self.toks_waiting.recv_timeout(dur) {
            Ok(x) => Some(x),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct NoneRegisteredError;

/*
TODO:
- find a way to BLOCK on the get and put...
- look into this atomic ordering business
- look into CachePadded
- signal registration and composition?
- blockstrategies?
*/
