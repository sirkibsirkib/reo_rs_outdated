use crate::reo::{self, ClosedErrorable, Component, Getter, Memory, PortClosed, Putter};
use bit_set::BitSet;
use hashbrown::HashSet;
use indexmap::IndexSet;
use mio::{Events, Poll, PollOpt, Ready, Token};
use std::ops::Deref;
use std::ops::Range;

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
struct ProdConsProto {
    p00g: Getter<u32>,
    p01g: Getter<u32>,
    p02p: Putter<u32>,
    m00: Memory<u32>,
}
impl ProdConsProto {

    #[rustfmt::skip]
    pub fn new(p00g: Getter<u32>, p01g: Getter<u32>, p02p: Putter<u32>) -> Self {
        let m00 = Default::default();
        Self { p00g, p01g, p02p, m00 }
    }
}
impl ProdConsProto {
    fn shutdown_if_mem(&mut self, raw_token: usize) -> Option<[usize; 2]> {
        Some(match raw_token {
            3 | 4 => { self.m00.shutdown(); [3,4] }
            _ => return None,
        })
    }
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        // use bits_prod_cons_proto::*;

        // Create poller and register ports
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(32);
        let a = Ready::all();
        let edge = PollOpt::edge();
        // bind port-ends and memory-ends with identifiable tokens
        poll.register(self.p00g.reg(), Token(0), a, edge).unwrap();
        poll.register(self.p01g.reg(), Token(1), a, edge).unwrap();
        poll.register(self.p02p.reg(), Token(2), a, edge).unwrap();
        poll.register(self.m00.reg_p().deref(), Token(3), a, edge).unwrap();
        poll.register(self.m00.reg_g().deref(), Token(4), a, edge).unwrap();

        // define the guards
        let mut guards = vec![];
        guard_cmd!(
            guards,
            bitset! {0,2,3},
            |_me: &mut Self| true,
            |me: &mut Self| {
                me.p02p.put(me.p00g.get()?).closed_err()?;
                me.m00.put(me.p01g.get()?).closed_err()?;
                Ok(())
            }
        );
        guard_cmd!(
            guards,
            bitset! {1,4},
            |_me: &mut Self| true,
            |me: &mut Self| {
                me.p02p.put(me.m00.get()?).closed_err()?;
                Ok(())
            }
        );
        let guard_idx_range: Range<usize> = 0..guards.len();
        let mut active_guards: HashSet<_> = guard_idx_range.collect();
        for (i, g) in guards.iter().enumerate() {
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
                if active_guards.contains(&guard_idx)
                    && ready.is_superset(&ready_set!(g))
                    && data_constraint!(g)(self)
                {
                    // remove fired ports from ready set
                    ready.difference_with(&ready_set!(g));
                    // apply the ACTION associated with this guard
                    let result = action_cmd!(g)(self);
                    if result.is_err() {
                        // failed! some port / memory closed!
                        // make a note to make this guard inactive
                        make_inactive.insert(guard_idx);
                    };
                }
            }
            while let Some(g_idx) = make_inactive.pop() {
                // make this guard inactive
                active_guards.remove(&g_idx);
                for tok in ready_set!(guards[g_idx]).iter() {
                    // traverse firing set
                    if let Some([mem_tok_p, mem_tok_g]) = self.shutdown_if_mem(tok) {
                        let idx_should_become_inactive = guards
                            .iter()
                            .enumerate()
                            .filter(|(i, g)| {
                                active_guards.contains(i)
                                    && (ready_set!(g).contains(mem_tok_p)
                                        || ready_set!(g).contains(mem_tok_g))
                            })
                            .map(|(i, _)| i);
                        for i in idx_should_become_inactive {
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





///////////////////////////////////

use crate::protocols::*;
struct Protoboi {
    g0: Getter<u32>,
    g1: Getter<u32>,
}
impl Peeker for Protoboi {
    fn try_peek_two<T: 'static>(&mut self, ids: [GetterId;2]) -> Result<[Option<&T>;2],PortClosed> {
        use std::any::Any;

        let x = match self.g0.try_peek()? {
            Some(x) => {
                let x: &(dyn Any) = x;
                if let Some(x) = x.downcast_ref::<T>() {
                    Some(x)
                } else {
                    panic!("X mismatch")
                }
            },
            None => None,
        };
        let y = match self.g1.try_peek()? {
            Some(y) => {
                let y: &(dyn Any) = y;
                if let Some(y) = y.downcast_ref::<T>() {
                    Some(y)
                } else {
                    panic!("y mismatch")
                }
            },
            None => None,
        };
        Ok([x,y])
    }
}



#[test] 
fn traity() {

}