use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::Condvar;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

struct Listener {
    sender: Sender<PortEvent>,
    token: usize,
}

struct Protected<T> {
    datum: Option<T>,
    put_listeners: Vec<Listener>,
    get_listeners: Vec<Listener>,
}

struct Shared<T> {
    protected: Mutex<Protected<T>>,
    full: Condvar,
    empty: Condvar,
}

pub struct Putter<T> {
    shared: Arc<Shared<T>>,
}
impl<T> Putter<T> {
    pub fn put(&self, datum: T) -> Result<(), T> {
        if Arc::strong_count(&self.shared) == 1 {
            return Err(datum);
        }
        let mut p = self.shared.protected.lock();
        if p.datum.is_some() {
            self.shared.empty.wait(&mut p);
        }
        if p.datum.is_some() {
            return Err(datum);
        }
        self.shared.full.notify_one();
        p.get_listeners
            .retain(|Listener { sender, token }| sender.send(PortEvent::GetReady(*token)).is_ok());
        let prev = p.datum.replace(datum);
        assert!(prev.is_none());
        Ok(())
    }
    pub fn register_with(&mut self, sel: &Selector, token: Token) {
        let mut p = self.shared.protected.lock();
        let sender = sel.sender.clone();
        p.put_listeners.push(Listener { sender, token });
    }
}

impl<T> Drop for Putter<T> {
    fn drop(&mut self) {
        let p = self.shared.protected.lock();
        for Listener { sender, token } in p.put_listeners.iter() {
            let _ = sender.send(PortEvent::Dropped(*token));
        }
    }
}

////////////
pub struct Getter<T> {
    shared: Arc<Shared<T>>,
}
impl<T> Getter<T> {
    pub fn get(&self) -> Result<T, ()> {
        if Arc::strong_count(&self.shared) == 1 {
            return Err(());
        }
        let mut p = self.shared.protected.lock();
        if p.datum.is_none() {
            self.shared.full.wait(&mut p);
        }
        match p.datum.take() {
            Some(x) => {
                p.put_listeners.retain(|Listener { sender, token }| {
                    sender.send(PortEvent::GetReady(*token)).is_ok()
                });
                Ok(x)
            }
            None => Err(()),
        }
    }
    pub fn register_with(&mut self, sel: &Selector, token: Token) {
        let mut p = self.shared.protected.lock();
        let sender = sel.sender.clone();
        p.get_listeners.push(Listener { sender, token });
    }
}
impl<T> Drop for Getter<T> {
    fn drop(&mut self) {
        let p = self.shared.protected.lock();
        for Listener { sender, token } in p.get_listeners.iter() {
            let _ = sender.send(PortEvent::Dropped(*token));
        }
    }
}

pub fn new_port<T>() -> (Putter<T>, Getter<T>) {
    let protected = Protected {
        datum: None,
        put_listeners: vec![],
        get_listeners: vec![],
    };
    let shared = Arc::new(Shared {
        empty: Default::default(),
        full: Default::default(),
        protected: Mutex::new(protected),
    });
    (
        Putter {
            shared: shared.clone(),
        },
        Getter { shared },
    )
}

type Token = usize;

#[derive(Debug, Copy, Clone)]
pub enum PortEvent {
    GetReady(Token),
    PutReady(Token),
    Dropped(Token),
}

pub struct Selector {
    sender: Sender<PortEvent>,
    receiver: Receiver<PortEvent>,
}
impl Default for Selector {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}
impl Selector {
    pub fn wait(&self) -> PortEvent {
        self.receiver.recv().expect("shouldn't happen")
    }
    pub fn wait_timeout(&self, timeout: Duration) -> Option<PortEvent> {
        self.receiver.recv_timeout(timeout).ok()
    }
}
