use std::cell::UnsafeCell;
use std::time::Duration;
use std::{mem, sync::Arc};
use mio::{Ready, Registration, SetReadiness};

/* IDEA:
the Shared object is only accessed by 0/1 putters and 0/1 getters.
The UnsafeCell is mutably referenced by two threads. how do we make this safe?
It all hinges on what the PUTTER is doing. We can partition the putter's time
in terms of where in the function put() it is, and call these states A and B.

~~~~~~~BARRIER1~~~~~~BARRIER2~~~~~~~ // wraps around
A A A A A | B B B B B B | A A A A A A ...

The getter keeps track of the putter's state with "putter_state".
In state B, the getter may access the unsafecell.
In state A, the putter may access the unsafecell.

When in state B: the putter is inside the get() function with 
the unsafecell pointing to the datum in their stackframe.
In B, the getter makes a shallow copy of the datum (unsafe!)
After Barrier2, The putter _forgets_ their original on the stack (unsafe!)
these two operations work together to be SAFE, as when control flow resumes,
there is precisely 1 copy of the datum.

This approach was chosen to facilitate cheap and safe getter PEEK operations (in state B).
The datum never moves from the putter's stackframe, so the getter crashing has no 
effect on the original.

BARRIER-wait operations essentially return an errorcode with the result of the 
operation. If the getter crashes in state B, the GETTER is dropped. As part of
the drop operation, the PUTTER is released with an error code indicating getter-crash.
In such a case, the Putter does NOT forget the original datum, and returns it as Err(T).


Another consequence of this design is that the putter is UNABLE TO TIMEOUT while
in state B. Entering state B represents the putter committing to the put-value.
Thus, un undesirable situation arises if the getter performes PEEK() but never GET().
*/

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PutterwardSignal {
	AToB,
	BToAGot,
	BToAThaw,
}

pub struct Shared<T> {
	data: UnsafeCell<*mut T>,
	// flag: UnsafeCell<bool>, // P->G "I allow refusal" // G->P "I refused"
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum  PutterState {
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
	fn try_peek(&mut self) -> Result<&T,bool> {
		if self.putter_state == PutterState::StateA {
			/////// Barrier 1 
			use crossbeam::TrySendError;
			match self.barrier.try_send(PutterwardSignal::AToB) {
				Ok(()) => {},
				Err(TrySendError::Disconnected(_)) => return Err(false),
				Err(TrySendError::Full(_)) => return Err(true),
			}
		}
		self.putter_state = PutterState::StateBPeeked;
		let datum: &T = unsafe {
			let r: *mut T = *self.shared.data.get();
			&*r
		};
		Ok(datum)
	}
	fn peek(&mut self) -> Result<&T,()> {
		if self.putter_state == PutterState::StateA {
			/////// Barrier 1 
			if self.barrier.send(PutterwardSignal::AToB).is_err() {
				return Err(())
			}
		}
		self.putter_state = PutterState::StateBPeeked;
		let datum: &T = unsafe {
			let r: *mut T = *self.shared.data.get();
			&*r
		};
		Ok(datum)
	}
	fn get(&mut self) -> Result<T,()> {
		self.peer_ready.set_readiness(Ready::writable()).unwrap(); // CERTAIN PUT
		if self.putter_state == PutterState::StateA  {
			/////// Barrier 1 
			if self.barrier.send(PutterwardSignal::AToB).is_err() {
				return Err(())
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
	fn put(&mut self, mut datum: T) -> Result<(),T> {
		let r: *mut T = &mut datum;
		unsafe { *self.shared.data.get() = r };
		self.peer_ready.set_readiness(Ready::writable()).unwrap(); // CERTAIN GET
		/////// Barrier 1 
		match self.barrier.recv() { // SIGNAL 1
			Ok(PutterwardSignal::AToB) => {},
			Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
			Err(crossbeam::RecvError) => return Err(datum),
		}
		/////// Barrier 2 
		let res = self.barrier.recv();
		self.peer_ready.set_readiness(Ready::empty()).unwrap();
		match res {
			Ok(PutterwardSignal::BToAGot) => {
				self.peer_ready.set_readiness(Ready::empty()).unwrap(); // say: peer will block!
				mem::forget(datum);
				Ok(())
			},
			Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
			Err(crossbeam::RecvError) => return Err(datum),
		}		
	}
	fn try_put(&mut self, mut datum: T, mut wait_duration: Option<Duration>) -> Result<(),TryPutErr<T>> {
		let start = std::time::Instant::now();
		let r: *mut T = &mut datum;
		unsafe { *self.shared.data.get() = r }; // set contents to datum on my stack
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // tentative put
		loop {
			/////// Barrier 1 
			if let Some(dur) = wait_duration {
				use crossbeam::RecvTimeoutError;
				match self.barrier.recv_timeout(dur) {
					Ok(PutterwardSignal::AToB) => {},
					Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
					Err(RecvTimeoutError::Timeout) => return Err(TryPutErr::Timeout(datum)),
					Err(RecvTimeoutError::Disconnected) => return Err(TryPutErr::PeerDropped(datum)),
				}
			} else {
				match self.barrier.recv() { 
					Ok(PutterwardSignal::AToB) => {},
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
					return Ok(())
				},
				Ok(PutterwardSignal::BToAThaw) => {
					println!("THAWED");
					if let Some(dur) = wait_duration {
						if let Some(to_wait) = dur.checked_sub(start.elapsed()) {
							wait_duration = Some(to_wait)
						} else {
							return Err(TryPutErr::Timeout(datum))
						}
					}
				},
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
	let (s,r) = crossbeam::channel::bounded(0);
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
	(p,g)
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
				},
				Err(TrySendError::Full(_)) => FreezeOutcome::PeerNotWaiting,
				Err(TrySendError::Disconnected(_)) => FreezeOutcome::PeerDropped,
			},
			PutterState::StateBPeeked => FreezeOutcome::PutterCommitted,
			PutterState::StateBUnpeeked => FreezeOutcome::Frozen,
		}
	}
	fn thaw(&mut self) {
		match self.putter_state { 
			PutterState::StateBUnpeeked => {},
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
	fn try_peek(&mut self) -> Result<&T,bool> {
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
	fn try_put(&mut self, datum: T, _wait_duration: Option<Duration>) -> Result<(),TryPutErr<T>> {
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
	fn put(&mut self, datum: T) -> Result<(),T>;
	fn try_put(&mut self, datum: T, wait_duration: Option<Duration>) -> Result<(),TryPutErr<T>>;
}

pub trait Getter<T> {
	fn try_peek(&mut self) -> Result<&T,bool>;
	fn peek(&mut self) -> Result<&T,()>;
	fn get(&mut self) -> Result<T,()>;
}