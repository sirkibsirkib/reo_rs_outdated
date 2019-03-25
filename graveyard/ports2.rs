use self::PortEventClass as Ev;
use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::Condvar;
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use std::sync::Arc;
use std::time::Duration;

pub trait Component {
    fn run(&mut self);
}

/////////////////////////////////////

struct Listener {
    sender: Sender<PortEvent>,
    token: usize,
}

struct Protected<T> {
    datum: Option<T>,
    put_listener: Option<Listener>,
    get_listener: Option<Listener>,
}

struct Shared<T> {
    protected: Mutex<Protected<T>>,
    putter_wait_to_write: Condvar,
}

pub struct Putter<T> {
    shared: Arc<Shared<T>>,
    rendezvous: Sender<()>,
}
impl<T> Putter<T> {
    pub fn put(&self, datum: T) -> Result<(), T> {
        {
            let mut p = self.shared.protected.lock();
            while p.datum.is_some() {
                println!("putter waiting");
                self.shared.putter_wait_to_write.wait(&mut p);
            }
            let prev = p.datum.replace(datum);
            assert!(prev.is_none());
            if let Some(Listener { ref sender, token }) = p.get_listener {
                let _ = sender.send(PortEvent {
                    token,
                    class: Ev::GetReady,
                });
            };
        }
        println!("putter rendezvous...");
        let _ = self.rendezvous.send(());
        println!("...putter rendezvous done");
        Ok(())
    }
    pub fn register_with(&mut self, sel: &Selector, token: Token) {
        let mut p = self.shared.protected.lock();
        let sender = sel.sender.clone();
        if p.datum.is_none() {
            let _ = sender.send(PortEvent {
                token,
                class: Ev::PutReady,
            });
        }
        let was = p.put_listener.replace(Listener { sender, token });
        assert!(was.is_none());
    }
    // TODO deregister
}

impl<T> Drop for Putter<T> {
    fn drop(&mut self) {
        println!("putter drop");
        let p = self.shared.protected.lock();
        if let Some(Listener { ref sender, token }) = p.put_listener {
            let _ = sender.send(PortEvent {
                token,
                class: Ev::Dropped,
            });
        }
    }
}

////////////
pub struct Getter<T> {
    shared: Arc<Shared<T>>,
    rendezvous: Receiver<()>,
}
impl<T> Getter<T> {
    pub fn get(&self) -> Result<T, ()> {
        println!("getter rendezvous...");
        self.rendezvous.recv().map_err(|_| ())?;
        let mut p = self.shared.protected.lock();
        println!("...getter rendezvous done");
        match p.datum.take() {
            Some(x) => {
                println!("notifying putters");
                if let Some(Listener { ref sender, token }) = p.put_listener {
                    let _ = sender.send(PortEvent {
                        token,
                        class: Ev::PutReady,
                    });
                };
                self.shared.putter_wait_to_write.notify_all();
                Ok(x)
            }
            None => Err(()),
        }
    }
    pub fn register_with(&mut self, sel: &Selector, token: Token) -> Option<Token> {
        let mut p = self.shared.protected.lock();
        let sender = sel.sender.clone();
        let maybe_tok: Option<Token> = Self::deregister_with_lock(&mut p);
        if p.datum.is_some() {
            println!("GETTER NOT WITH REG");
            let _ = sender.send(PortEvent {
                token,
                class: Ev::GetReady,
            });
        }
        let _ = p.get_listener.replace(Listener { sender, token });
        maybe_tok
    }
    pub fn deregister(&mut self) -> Option<Token> {
        let mut p = self.shared.protected.lock();
        Self::deregister_with_lock(&mut p)
    }
    fn deregister_with_lock(p: &mut MutexGuard<Protected<T>>) -> Option<Token> {
        if let Some(Listener { sender, token }) = p.get_listener.take() {
            let _ = sender.send(PortEvent {
                token,
                class: Ev::Deregistered,
            });
            Some(token)
        } else {
            None
        }
    }
}
impl<T> Drop for Getter<T> {
    fn drop(&mut self) {
        println!("getter drop");
        let p = self.shared.protected.lock();
        if let Some(Listener { ref sender, token }) = p.get_listener {
            let _ = sender.send(PortEvent {
                token,
                class: Ev::Dropped,
            });
        }
        self.shared.putter_wait_to_write.notify_all();
    }
}
////////////////////

pub fn new_port<T>() -> (Putter<T>, Getter<T>) {
    let protected = Protected {
        datum: None,
        put_listener: None,
        get_listener: None,
    };
    let (s, r) = crossbeam::channel::bounded(0);
    let shared = Arc::new(Shared {
        putter_wait_to_write: Default::default(),
        protected: Mutex::new(protected),
    });
    (
        Putter {
            shared: shared.clone(),
            rendezvous: s,
        },
        Getter {
            shared,
            rendezvous: r,
        },
    )
}

type Token = usize;

#[derive(Debug, Copy, Clone)]
pub enum PortEventClass {
    GetReady,
    PutReady,
    Deregistered,
    Dropped,
}
#[derive(Debug, Copy, Clone)]
pub struct PortEvent {
    pub class: PortEventClass,
    pub token: Token,
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
