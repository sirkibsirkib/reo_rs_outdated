use crossbeam::channel::{bounded, Receiver, RecvError, Select, SendError, Sender, TryIter};
use parking_lot::Mutex;

#[derive(Debug, Copy, Clone)]
pub struct PortClosedError;
use std::convert::From;
impl<T> From<SendError<T>> for PortClosedError {
    fn from(_e: SendError<T>) -> PortClosedError {
        Self
    }
}
impl From<RecvError> for PortClosedError {
    fn from(_e: RecvError) -> PortClosedError {
        Self
    }
}

pub trait Component {
    fn run(&mut self);
}

pub fn new_port<T>() -> (PortPutter<T>, PortGetter<T>) {
    let (data_s, data_r) = bounded(1);
    let (done_s, done_r) = bounded(0);
    let p = PortPutter {
        data: data_s,
        done: done_r,
    };
    let g = PortGetter {
        data: data_r,
        done: done_s,
        storage: Mutex::new(None),
    };
    (p, g)
}

pub struct PortPutter<T> {
    data: Sender<T>,
    done: Receiver<()>,
}
impl<T> PortPutter<T> {
    pub fn put(&self, datum: T) -> Result<(), PortClosedError> {
        self.done.recv();
        Ok(self.data.send(datum)?)
    }
    pub(crate) fn inner(&self) -> &Sender<T> {
        &self.data
    }
}

pub struct PortGetter<T> {
    storage: Mutex<Option<T>>,
    data: Receiver<T>,
    done: Sender<()>,
}
impl<T> PortGetter<T> {
    pub fn get(&self) -> Result<T, PortClosedError> {
        let mut storage = self.storage.lock();
        if storage.is_none() {
            *storage = Some(self.data.recv()?);
        }
        self.done.send(())?;
        Ok(storage.take().unwrap())
    }
    pub fn peek_apply<F, R>(&self, func: F) -> Result<R, PortClosedError>
    where
        F: Fn(&T) -> R,
    {
        let mut storage = self.storage.lock();
        if storage.is_none() {
            *storage = Some(self.data.recv()?);
        }
        Ok(func(storage.as_ref().unwrap()))
    }
    pub(crate) fn inner(&self) -> &Receiver<T> {
        &self.data
    }
}
