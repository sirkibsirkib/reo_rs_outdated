
use crate as reo_rs;

use bit_set::BitSet;
use mio::{Poll, PollOpt, Ready, Token};
use std::time::Duration;
use reo_rs::{Memory, PortGetter, PortPutter, Putter, Getter, Component, Freezer};
use reo_rs::protocols::{GuardCmd, ProtoComponent, DiscardableError};

struct Producer {
    p_out: PortPutter<u32>,
    offset: u32,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..10 {
            println!("putter with offset {:?} got result {:?}", self.offset, self.p_out.try_put(i + self.offset, Some(Duration::from_millis(3))));
        }
    }
}

struct Consumer {
    p_in: PortGetter<u32>,
}
impl Component for Consumer {
    fn run(&mut self) {
        while let Ok(x) = self.p_in.get() {
            println!("{:?}", x);
        }
    }
}
struct ProdConsProto {
    p00g: PortGetter<u32>,
    p01g: PortGetter<u32>,
    p02p: PortPutter<u32>,
    m00: Memory<u32>,
}
impl ProdConsProto {

    #[rustfmt::skip]
    pub fn new(p00g: PortGetter<u32>, p01g: PortGetter<u32>, p02p: PortPutter<u32>) -> Self {
        let m00 = Default::default();
        Self { p00g, p01g, p02p, m00 }
    }
}

def_consts![0 => P00G, P01G, P02P, M00G, M00P];
impl ProtoComponent for ProdConsProto {
    fn lookup_getter(&mut self, tok: usize) -> Option<&mut (dyn Freezer)> {
        Some(match tok {
            P00G => &mut self.p00g,
            P01G => &mut self.p01g,
            _ => return None,
        })
    }
    fn get_local_peer_token(&self, token: usize) -> Option<usize> {
        Some(match token {
            M00P => M00G,
            M00G => M00P,
            _ => return None,
        })
    }
    fn token_shutdown(&mut self, token: usize) {
        match token {
            M00P | M00G => self.m00.shutdown(),
            _ => {},
        }
    }
    fn register_all(&mut self, poll: &Poll) {
        let a = Ready::all();
        let edge = PollOpt::edge();
        poll.register(self.p00g.reg(), Token(P00G), a, edge).unwrap();
        poll.register(self.p01g.reg(), Token(P01G), a, edge).unwrap();
        poll.register(self.p02p.reg(), Token(P02P), a, edge).unwrap();
        poll.register(self.m00.reg_p().as_ref(), Token(M00P), a, edge).unwrap();
        poll.register(self.m00.reg_g().as_ref(), Token(M00G), a, edge).unwrap();
    }
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        let mut gcmds = vec![];
        guard_cmd!(gcmds,
            bitset! {P00G,P01G,P02P,M00P},
            |_me: &mut Self| {
                true
            },
            |me: &mut Self| {
                me.p02p.put(me.p00g.get()?).unit_err()?;
                me.m00.put(me.p01g.get()?).unit_err()?;
                Ok(())
            }
        );
        guard_cmd!(gcmds,
            bitset! {P02P,M00G},
            |_me: &mut Self| {
                true
            },
            |me: &mut Self| {
                me.p02p.put(me.m00.get()?).unit_err()?;
                Ok(())
            }
        );
        for g in gcmds.iter() {
            println!("{:?}", g.get_ready_set());
        }
        self.run_to_termination(&gcmds);
    }
}

#[test]
fn alternator() {
    // create ports
    use reo_rs::new_port;
    let (p00p, p00g) = new_port();
    let (p01p, p01g) = new_port();
    let (p02p, p02g) = new_port();

    // spin up threads
    #[rustfmt::skip]
    crossbeam::scope(|s| {
        s.builder()
            .name("Producer_1".into())
            .spawn(move |_| Producer { p_out: p00p, offset: 0 }.run())
            .unwrap();
        s.builder()
            .name("Producer_2".into())
            .spawn(move |_| Producer { p_out: p01p, offset: 100 }.run())
            .unwrap();
        s.builder()
            .name("Proto".into())
            .spawn(move |_| ProdConsProto::new(p00g, p01g, p02p).run())
            .unwrap();
        s.builder()
            .name("Consumer".into())
            .spawn(move |_| Consumer { p_in: p02g }.run())
            .unwrap();
    })
    .expect("A worker thread panicked!");
}



#[test]
fn threadless_test() {
    use crate::threadless2::*;
    impl CloneFrom<[u32;32]> for [u32;8] {
        fn clone_from(t: &[u32;32]) -> Self {
            let mut ret = [0;8];
            for i in 0..8 {
                ret[i] = t[i];
            }
            ret
        }
    }

    fn prod(mut p: Putter<[u32;32]>) {
        for i in 0..20 {
            p.put([i;32]).unwrap();
        }
    }
    fn cons(mut g: Getter<[u32;32]>) {
        type Signal = ();
        for i in 0..20 {
            match i%3 {
                0 => println!("{:?}", g.get().unwrap()),
                1 => println!("{:?}", g.get_weaker::<[u32;8]>().unwrap()),
                2 => println!("{:?}", g.get_weaker::<Signal>().unwrap()),
                _ => unreachable!(),
            }
        }
    }

    let (p, g) = new_proto();

    crossbeam::scope(|s| {
        s.spawn(|_| prod(p));
        s.spawn(|_| cons(g)); 
    }).unwrap();
}
