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
	BToAAccepted,
	BToARefused,
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
pub struct Getter<T> {
	shared: Arc<Shared<T>>,
	barrier: crossbeam::Sender<PutterwardSignal>, 
	putter_state: PutterState,
	my_reg: Registration,
	peer_ready: SetReadiness,
}

impl<T> Getter<T> {
	pub fn peek(&mut self) -> Result<&T,()> {
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
	pub fn get(&mut self) -> Result<T,()> {
		if self.putter_state == PutterState::StateA  {
			/////// Barrier 1 
			if self.barrier.send(PutterwardSignal::AToB).is_err() {
				return Err(())
			}
		}
		let datum = unsafe { mem::replace(&mut **self.shared.data.get(), mem::uninitialized()) };
		/////// Barrier 2 
		let _ = self.barrier.send(PutterwardSignal::BToAAccepted);
		self.putter_state = PutterState::StateA;
		Ok(datum)
	}
	pub fn reg(&self) -> &Registration {
		&self.my_reg
	}
}

pub struct Putter<T> {
	shared: Arc<Shared<T>>,
	barrier: crossbeam::Receiver<PutterwardSignal>,
	my_reg: Registration,
	peer_ready: SetReadiness,
}

pub enum TryPutErr<T> {
	PeerDropped(T),
	Timeout(T),
}

impl<T> Putter<T> {
	pub fn put(&mut self, mut datum: T) -> Result<(),T> {
		let r: *mut T = &mut datum;
		unsafe { *self.shared.data.get() = r };
		self.peer_ready.set_readiness(Ready::writable()); // CERTAIN GET
		/////// Barrier 1 
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer won't block!
		match self.barrier.recv() { // SIGNAL 1
			Ok(PutterwardSignal::AToB) => {},
			Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
			Err(crossbeam::RecvError) => return Err(datum),
		}
		/////// Barrier 2 
		let res = self.barrier.recv();
		self.peer_ready.set_readiness(Ready::empty());
		match res {
			Ok(PutterwardSignal::BToAAccepted) => {
				self.peer_ready.set_readiness(Ready::empty()).unwrap(); // say: peer will block!
				mem::forget(datum);
				Ok(())
			},
			Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
			Err(crossbeam::RecvError) => return Err(datum),
		}		
	}
	pub fn try_put(&mut self, mut datum: T, wait_duration: Option<Duration>) -> Result<(),TryPutErr<T>> {
		let r: *mut T = &mut datum;
		unsafe { *self.shared.data.get() = r }; // set contents to datum on my stack
		self.peer_ready.set_readiness(Ready::readable()); // tentative put
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
		self.peer_ready.set_readiness(Ready::empty()); // tentative put
		match res {
			Ok(PutterwardSignal::BToAAccepted) => {
				mem::forget(datum);
				Ok(())
			},
			Ok(PutterwardSignal::BToARefused) => Err(TryPutErr::Timeout(datum)),
			Ok(wrong_signal) => panic!("Putter got wrong signal! {:?}", wrong_signal),
			Err(crossbeam::RecvError) => Err(TryPutErr::PeerDropped(datum)),
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
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer dead!
	}
}
impl<T> Drop for Putter<T> {
	fn drop(&mut self) {
		self.peer_ready.set_readiness(Ready::readable()).unwrap(); // say: peer dead!
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
	});
	let (s,r) = crossbeam::channel::bounded(0);
	let p = Putter {
		shared: a_shared.clone(),
		barrier: r,
		my_reg: p_reg,
		peer_ready: g_red,
	};
	let g = Getter {
		shared: a_shared,
		barrier: s,
		putter_state: PutterState::StateA,
		my_reg: g_reg,
		peer_ready: p_red,
	};
	(p,g)
}


pub trait Catcallable {
	// non-blocking no-value peek
	// panics if this gotten value has been peeked before
	// returns Err(()) if the port is dead
	// returns Ok(true) if the catcall succeeds. the putter locked, peek / get won't block.
	// returns Ok(false) if catcall fails. putter isn't ready. peek / get may block.
	fn catcall(&mut self) -> Result<bool,()>;

	// only execute AFTER successful catcall where no peek has been performed
	// returns true if the putter was successfully released OR the port is dead
	fn try_release_putter(&mut self) -> bool;
}
impl<T> Catcallable for Getter<T> {
	fn catcall(&mut self) -> Result<bool,()> {
		use crossbeam::channel::TrySendError;
		match self.putter_state {
			PutterState::StateA => match self.barrier.try_send(PutterwardSignal::AToB) {
				Ok(()) => {
					self.putter_state = PutterState::StateA;
					Ok(true)
				},
				Err(TrySendError::Full(_)) => Ok(false),
				Err(TrySendError::Disconnected(_)) => Err(()),
			},
			PutterState::StateBPeeked => panic!("Catcall on peeked value!"),
			PutterState::StateBUnpeeked => return Ok(true),
		}
	}
	fn try_release_putter(&mut self) -> bool {
		match self.putter_state { 
			PutterState::StateBUnpeeked => {},
			wrong_state => panic!("tried to release putter in state {:?}", wrong_state),
		}
		use crossbeam::channel::TrySendError;
		match self.barrier.try_send(PutterwardSignal::BToARefused) {
			Ok(()) => true,
			Err(TrySendError::Full(_)) => panic!("try release would block! you promised you catcalled!"),
			Err(TrySendError::Disconnected(_)) => true, // port is dead. no problem
		}
	}
}

pub fn catcall_all_or_release<'a, I>(it: I) -> bool
where I: IntoIterator<Item=&'a mut (dyn Catcallable)> + Clone {
	let it2 = it.clone().into_iter();
	for (i, c) in it.into_iter().enumerate() {
		if !c.catcall().expect("catcall failed!") {
			// catcall failed. must unroll.
			for c2 in it2.take(i) {
				assert!(c2.try_release_putter());
			}
			return false;
		}
	}
	true
}

