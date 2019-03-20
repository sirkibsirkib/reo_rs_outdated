use crossbeam::channel::{Sender, Receiver, bounded, Select, SendError, RecvError};

#[derive(Debug, Copy, Clone)]
pub struct PortClosedError;
use std::convert::From;
impl<T> From<SendError<T>> for PortClosedError {
	fn from(_e: SendError<T>) -> PortClosedError { Self }
}
impl From<RecvError> for PortClosedError {
	fn from(_e: RecvError) -> PortClosedError { Self }
}

pub trait Component {
	fn run(&mut self);
}

pub fn new_port<T>() -> (PortPutter<T>, PortGetter<T>) {
	let (s,r) = bounded(1);
	(PortPutter(s), PortGetter(r))
}

pub struct PortPutter<T>(Sender<T>);
impl<T> PortPutter<T> {
	pub fn put(&self, datum: T) -> Result<(),PortClosedError> {
		Ok(self.0.send(datum)?)
	}
	pub(crate) fn inner(&self) -> &Sender<T> {
		&self.0
	}
}

pub struct PortGetter<T>(Receiver<T>);
impl<T> PortGetter<T> {
	pub fn get(&self) -> Result<T,PortClosedError>  {
		Ok(self.0.recv()?)
	}
	pub(crate) fn inner(&self) -> &Receiver<T> {
		&self.0
	}
}