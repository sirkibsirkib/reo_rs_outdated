use std::ops::Range;
use bit_set::BitSet;
use hashbrown::HashSet;
use mio::{Events, Poll, PollOpt, Ready, Token};
use std::ops::Deref;
use crate::reo::{self, Putter, Getter, Component, Memory, ClosedErrorable, PortClosed};
use indexmap::IndexSet;

struct Producer {
    p_out: Putter<u32>,
    offset: u32,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..3 {
            self.p_out.put(i + self.offset).unwrap();
        }
    }
}

struct Consumer {
    p_in: Getter<u32>,
}
impl Component for Consumer {
    fn run(&mut self) {
        while let Ok(x) = self.p_in.get() {
            println!("{:?}", x);
        }
    }
}

mod bits_prod_cons_proto {    
    pub const P00G_BIT: usize = 0;
    pub const P01G_BIT: usize = 1;
    pub const P02P_BIT: usize = 2;

    pub const M00P_BIT: usize = 3;
    pub const M00G_BIT: usize = 4;
}
struct ProdConsProto {
    p00g: Getter<u32>,
    p01g: Getter<u32>,
    p02p: Putter<u32>,
    m00: Memory<u32>,
}
impl ProdConsProto {
    pub fn new(p00g: Getter<u32>, p01g: Getter<u32>, p02p: Putter<u32>) -> Self {
        let m00 = Default::default();
        Self {
            p00g, p01g, p02p, m00,
        }
    }
}
impl ProdConsProto {
    fn shutdown_if_mem(&mut self, raw_token: usize) -> Option<[usize;2]> {
        use bits_prod_cons_proto as x;
        Some(match raw_token {
            x::M00P_BIT | x::M00G_BIT => {
                self.m00.shutdown();
                [x::M00P_BIT, x::M00G_BIT]
            },
            _ => return None,
        })
    }
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        use bits_prod_cons_proto::*;

        // Create poller and register ports
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(32);
        let a = Ready::all();
        let edge = PollOpt::edge();
        // bind port-ends and memory-ends with identifiable tokens
        poll.register(self.p00g.reg(), Token(P00G_BIT), a, edge).unwrap();
        poll.register(self.p01g.reg(), Token(P01G_BIT), a, edge).unwrap();
        poll.register(self.p02p.reg(), Token(P02P_BIT), a, edge).unwrap();
        poll.register(self.m00.reg_p().deref(), Token(M00P_BIT), a, edge).unwrap();
        poll.register(self.m00.reg_g().deref(), Token(M00G_BIT), a, edge).unwrap();

        // define the guards
        let mut guards = vec![];
        guard_cmd!(guards,
            bitset! {P00G_BIT, P01G_BIT, P02P_BIT, M00P_BIT},
            |_me: &mut Self| true,
            |me: &mut Self| {
                me.p02p.put(me.p00g.get()?).closed_err()?;
                me.m00.put(me.p01g.get()?).closed_err()?;
                Ok(())
            }
        );
        guard_cmd!(guards,
            bitset! {P02P_BIT, M00G_BIT},
            |_me: &mut Self| true,
            |me: &mut Self| {
                me.p02p.put(me.m00.get()?).closed_err()?;
                Ok(())
            }
        );
        let guard_idx_range: Range<usize> = 0..guards.len();
        let mut active_guards: HashSet<_> = guard_idx_range.collect();
        for (i,g) in guards.iter().enumerate() {
            println!("{:?}: {:?}", i, &g.0);
        }

        let mut ready = BitSet::new();
        let mut make_inactive = IndexSet::new();
        while !active_guards.is_empty() {
            poll.poll(&mut events, None).unwrap();
            for event in events.iter() {
                // raise the 'ready' flag for this token.
                ready.insert(event.token().0);
            }
            for (guard_idx, g) in guards.iter().enumerate() {
                // if (1) guard active (2) firing set ready (3) data const met
                if active_guards.contains(&guard_idx)
                && ready.is_superset(&g.0)
                && (g.1)(self)
                {
                    // remove fired ports from ready set
                    ready.difference_with(&g.0);
                    // apply the ACTION associated with this guard
                    let result = (g.2)(self); 
                    if result.is_err() { // failed! some port / memory closed!
                        // make a note to make this guard inactive
                        make_inactive.insert(guard_idx);                        
                    };
                }
            }
            while let Some(g_idx) = make_inactive.pop() {
                // make this guard inactive
                active_guards.remove(&g_idx);
                for tok in guards[g_idx].0.iter() {
                    // traverse firing set 
                    if let Some([mem_tok_p,mem_tok_g]) = self.shutdown_if_mem(tok) {
                        for (i,_) in guards.iter().enumerate()
                        .filter(|(i,g)| {
                            active_guards.contains(i)
                            && (g.0.contains(mem_tok_p) || g.0.contains(mem_tok_g))
                        }) {
                            make_inactive.insert(i);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn alternator() {
    // create ports
    let (p00p, p00g) = reo::new_port();
    let (p01p, p01g) = reo::new_port();
    let (p02p, p02g) = reo::new_port();

    // spin up threads
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
