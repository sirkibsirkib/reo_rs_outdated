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

pub struct Shared<T> {
	data: UnsafeCell<*mut T>,
	flag: UnsafeCell<bool>, // P->G "I allow refusal" // G->P "I refused"
}

enum  PutterState {
	StartBarrier,
	MidBarrier,
}
impl PutterState {
	fn is_start(&self) -> bool {
		match self {
			PutterState::StartBarrier => true,
			PutterState::MidBarrier => false,
		}
	}
	fn reset(&mut self) {
		*self = PutterState::StartBarrier;
	}
}
pub struct Getter<T> {
	shared: Arc<Shared<T>>,
	barrier: crossbeam::Receiver<()>, 
	putter_state: PutterState,
	my_reg: Registration,
	peer_ready: SetReadiness,
}

impl<T> Getter<T> {
	pub fn try_peek(&mut self, wait_duration: Option<Duration>) -> Result<&T,bool> {
		if self.putter_state.is_start() {
			match self.barrier_wait_timeout(wait_duration) { // BARRIER 1
				WaitResult::Rendezvous => {},
				WaitResult::PeerDropped => return Err(false),
				WaitResult::Timeout => return Err(true),
			}
		}
		self.putter_state = PutterState::MidBarrier;
		let datum: &T = unsafe {
			let r: *mut T = *self.shared.data.get();
			&*r
		};
		Ok(datum)
	}
	pub fn peek(&mut self) -> Result<&T,()> {
		if self.putter_state.is_start() && !self.barrier_wait() { // BARRIER 1
			return Err(())
		}
		self.putter_state = PutterState::MidBarrier;
		let datum: &T = unsafe {
			let r: *mut T = *self.shared.data.get();
			&*r
		};
		Ok(datum)
	}
	pub fn try_get(&mut self, wait_duration: Option<Duration>) -> Result<T,bool> {
		if self.putter_state.is_start() {
			match self.barrier_wait_timeout(wait_duration) { // BARRIER 1
				WaitResult::Rendezvous => {},
				WaitResult::PeerDropped => return Err(false),
				WaitResult::Timeout => return Err(true),
			}
		}
		let datum: T = unsafe {
			let r: *mut T = *self.shared.data.get();
			std::mem::replace(&mut *r, std::mem::uninitialized())
		};
		self.barrier_wait();
		self.putter_state.reset();
		Ok(datum)
	}
	pub fn try_refuse(&mut self) -> Result<(),T> {
		println!("GET START");
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer won't block!
		/////// Barrier 1 
		if self.putter_state.is_start() {
			if !self.barrier_wait() {
				return Ok(())
			}
		}
		let ret = if unsafe { *self.shared.flag.get() } {
			// allowed to refuse. leave flag
			let datum = unsafe { mem::replace(&mut **self.shared.data.get(), mem::uninitialized()) };
			Err(datum)
		} else {
			// not allowed to refuse. set flag
			Ok(())
		};
		/////// Barrier 2 
		self.barrier_wait();
		self.peer_ready.set_readiness(Ready::empty()).unwrap(); // say: peer will block!
		self.putter_state.reset();
		ret
	}
	pub fn get(&mut self) -> Result<T,()> {
		println!("GET START");
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer won't block!
		/////// Barrier 1 
		if self.putter_state.is_start() {
			if !self.barrier_wait() {
				return Err(())
			}
		}
		unsafe { *self.shared.flag.get() = false }; // I DID NOT REFUSE
		let datum = unsafe { mem::replace(&mut **self.shared.data.get(), mem::uninitialized()) };
		/////// Barrier 2 
		self.barrier_wait();
		self.peer_ready.set_readiness(Ready::empty()).unwrap(); // say: peer will block!
		self.putter_state.reset();
		Ok(datum)
	}
	fn barrier_wait(&mut self) -> bool {
		self.barrier.recv().is_ok()
	}
	fn barrier_wait_timeout(&mut self, wait_duration: Option<Duration>) -> WaitResult {
		if let Some(t) = wait_duration {
			use crossbeam::channel::RecvTimeoutError;
			match self.barrier.recv_timeout(t) {
				Ok(()) => WaitResult::Rendezvous,
				Err(RecvTimeoutError::Timeout) => WaitResult::Timeout,
				Err(RecvTimeoutError::Disconnected) => WaitResult::PeerDropped,
			}
		} else {
			use crossbeam::channel::TryRecvError;
			match self.barrier.try_recv() {
				Ok(()) => WaitResult::Rendezvous,
				Err(TryRecvError::Empty) => WaitResult::Timeout,
				Err(TryRecvError::Disconnected) => WaitResult::PeerDropped,
			}
		}
	}
	pub fn reg(&self) -> &Registration {
		&self.my_reg
	}
}

pub struct Putter<T> {
	shared: Arc<Shared<T>>,
	barrier: crossbeam::Sender<()>,
	my_reg: Registration,
	peer_ready: SetReadiness,
}

pub enum TryPutErr {
	PeerDropped,
	Refused,
	Timeout,
}

impl<T> Putter<T> {
	pub fn put(&mut self, mut datum: T) -> Result<(),T> {
		// println!("PUT START");
		let r: *mut T = &mut datum;
		// println!("PUT R ptr {:p}", r);
		unsafe { *self.shared.data.get() = r }; // set contents to datum on my stack
		unsafe { *self.shared.flag.get() = false }; // I DONT ALLOW REFUSAL
		/////// Barrier 1 

		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer won't block!
		if !self.barrier_wait() { // SIGNAL 1
			return Err(datum)
		}
		/////// Barrier 2 
		self.barrier_wait(); // SIGNAL 2

		self.peer_ready.set_readiness(Ready::empty()).unwrap(); // say: peer will block!
		mem::forget(datum);
		Ok(())
	}
	pub fn try_put(&mut self, mut datum: T, wait_duration: Option<Duration>) -> Result<(),(TryPutErr,T)> {
		let r: *mut T = &mut datum;
		unsafe { *self.shared.data.get() = r }; // set contents to datum on my stack
		unsafe { *self.shared.flag.get() = true }; // I DO ALLOW REFUSAL
		match self.barrier_wait_timeout(wait_duration) { // SIGNAL 1
			WaitResult::Rendezvous => {},
			WaitResult::PeerDropped => return Err((TryPutErr::PeerDropped, datum)),
			WaitResult::Timeout => return Err((TryPutErr::Timeout, datum)),
		}
		self.barrier_wait(); // SIGNAL 2
		if unsafe { *self.shared.flag.get() } {
			// the value was refused!
			Err((TryPutErr::Refused, datum))
		} else {
			// the value was gotten! (not refused)
			mem::forget(datum);
			Ok(())
		}
	}
	fn barrier_wait(&mut self) -> bool {
		self.barrier.send(()).is_ok()
	}
	fn barrier_wait_timeout(&mut self, wait_duration: Option<Duration>) -> WaitResult {
		if let Some(t) = wait_duration {
			use crossbeam::channel::SendTimeoutError;
			match self.barrier.send_timeout((),t) {
				Ok(()) => WaitResult::Rendezvous,
				Err(SendTimeoutError::Timeout(())) => WaitResult::Timeout,
				Err(SendTimeoutError::Disconnected(())) => WaitResult::PeerDropped,
			}
		} else {
			use crossbeam::channel::TrySendError;
			match self.barrier.try_send(()) {
				Ok(()) => WaitResult::Rendezvous,
				Err(TrySendError::Full(())) => WaitResult::Timeout,
				Err(TrySendError::Disconnected(())) => WaitResult::PeerDropped,
			}
		}
	}
	pub fn reg(&self) -> &Registration {
		&self.my_reg
	}
}


unsafe impl<T> Sync for Putter<T> {}
unsafe impl<T> Sync for Getter<T> {}
unsafe impl<T> Send for Putter<T> {}
unsafe impl<T> Send for Getter<T> {}
impl<T> Drop for Getter<T> {
	fn drop(&mut self) {
		self.peer_ready.set_readiness(Ready::writable()).unwrap(); // say: peer dead!
	}
}
impl<T> Drop for Putter<T> {
	fn drop(&mut self) {
		self.peer_ready.set_readiness(Ready::writable()).unwrap(); // say: peer dead!
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum WaitResult {
	Rendezvous,
	PeerDropped,
	Timeout,
}
impl WaitResult {
	pub fn was_rendezvous(self) -> bool {
		match self {
			WaitResult::Rendezvous => true,
			_ => false,
		}
	}
}

pub fn new_port<T>() -> (Putter<T>, Getter<T>) {
    let (g_reg, g_red) = mio::Registration::new2();
    let (p_reg, p_red) = mio::Registration::new2();
	let a_shared = Arc::new(Shared {
		data: UnsafeCell::new(std::ptr::null_mut()),
		flag: UnsafeCell::new(false),
	});
	let (s,r) = crossbeam::channel::bounded(0);
	let p = Putter {
		shared: a_shared.clone(),
		barrier: s,
		my_reg: p_reg,
		peer_ready: g_red,
	};
	let g = Getter {
		shared: a_shared,
		barrier: r,
		putter_state: PutterState::StartBarrier,
		my_reg: g_reg,
		peer_ready: p_red,
	};
	(p,g)
}
