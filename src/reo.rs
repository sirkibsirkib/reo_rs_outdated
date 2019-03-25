use crossbeam::{Receiver, Sender};
use mio::{event::Evented, Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use std::io;

pub trait Component {
    fn run(&mut self);
}

pub fn new_port<T>() -> (Putter<T>, Getter<T>) {
    let (data_s, data_r) = crossbeam::channel::bounded(0);
    let (sig_s, sig_r) = crossbeam::channel::bounded(0);
    let (g_reg, g_red) = mio::Registration::new2();
    let (p_reg, p_red) = mio::Registration::new2();
    let p = Putter {
        data: data_s,
        done_sig: sig_r,
        putter_reg: p_reg,
        getter_set_ready: g_red,
    };
    let g = Getter {
        cache: None,
        data: data_r,
        done_sig: sig_s,
        getter_reg: g_reg,
        putter_set_ready: p_red,
    };
    (p, g)
}

macro_rules! discard {
    () => {
        |_| {}
    };
}

//////////////////////////////////////////////////

pub struct Putter<T> {
    data: Sender<T>,
    done_sig: Receiver<()>,
    putter_reg: Registration,
    getter_set_ready: SetReadiness,
}

pub struct Getter<T> {
    cache: Option<T>,
    data: Receiver<T>,
    done_sig: Sender<()>,
    getter_reg: Registration,
    putter_set_ready: SetReadiness,
}

//////////////////////////////
impl<T> Putter<T> {
    pub fn put(&mut self, datum: T) -> Result<(), ()> {
        self.set_peer_readiness(true).unwrap();
        self.data.send(datum).map_err(discard!())?;
        self.done_sig.recv().map_err(discard!())?;
        self.set_peer_readiness(false).unwrap();
        Ok(())
    }
    fn set_peer_readiness(&self, is_ready: bool) -> Result<(), io::Error> {
        let r = if is_ready {
            Ready::readable()
        } else {
            Ready::empty()
        };
        self.getter_set_ready.set_readiness(r)
    }
}

impl<T> Getter<T> {
    pub fn peek(&mut self) -> Result<&T, ()> {
        if self.cache.is_none() {
            self.cache.replace(self.acquire_from_putter()?);
        }
        Ok(self.cache.as_ref().unwrap())
    }
    fn acquire_from_putter(&self) -> Result<T, ()> {
        self.set_peer_readiness(true).unwrap();
        Ok(self.data.recv().map_err(discard!())?)
    }
    pub fn get(&mut self) -> Result<T, ()> {
        let datum = self
            .cache
            .take()
            .or_else(|| self.acquire_from_putter().ok())
            .ok_or({})?;
        self.done_sig.send(()).map_err(discard!())?;
        self.set_peer_readiness(false).unwrap();
        Ok(datum)
    }
    fn set_peer_readiness(&self, is_ready: bool) -> Result<(), io::Error> {
        let r = if is_ready {
            Ready::writable()
        } else {
            Ready::empty()
        };
        self.putter_set_ready.set_readiness(r)
    }
}

////////////////////////////////////////////////////

impl<T> Drop for Putter<T> {
    fn drop(&mut self) {
        let _ = self.set_peer_readiness(true);
    }
}
impl<T> Drop for Getter<T> {
    fn drop(&mut self) {
        let _ = self.set_peer_readiness(true);
    }
}

impl<T> Evented for Putter<T> {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        ready: Ready,
        po: PollOpt,
    ) -> Result<(), io::Error> {
        self.putter_reg.register(poll, token, ready, po)
    }
    fn deregister(&self, poll: &Poll) -> Result<(), io::Error> {
        #[allow(deprecated)]
        self.putter_reg.deregister(poll)
    }
    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        ready: Ready,
        po: PollOpt,
    ) -> Result<(), io::Error> {
        self.putter_reg.reregister(poll, token, ready, po)
    }
}

impl<T> Evented for Getter<T> {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        ready: Ready,
        po: PollOpt,
    ) -> Result<(), io::Error> {
        self.getter_reg.register(poll, token, ready, po)
    }
    fn deregister(&self, poll: &Poll) -> Result<(), io::Error> {
        #[allow(deprecated)]
        self.getter_reg.deregister(poll)
    }
    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        ready: Ready,
        po: PollOpt,
    ) -> Result<(), io::Error> {
        self.getter_reg.reregister(poll, token, ready, po)
    }
}
