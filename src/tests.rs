use bit_set::BitSet;
use hashbrown::HashSet;
use mio::{Events, Poll, PollOpt, Ready, Token};
use crate::reo::{self, Putter, Getter, Component};


struct Producer {
    p00p: Putter<u32>,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..1000 {
            self.p00p.put(i).unwrap();
        }
    }
}

struct Consumer {
    p01g: Getter<u32>,
}
impl Component for Consumer {
    fn run(&mut self) {
        let mut got = Vec::with_capacity(1000);
        loop {
            // println!("consumer cons...");
            if let Ok(x) = self.p01g.get() {
                got.push(x);
            } else {
                println!("cons got err. quitting");
                break;
            }
        }
        println!("got {:?}", &got);
    }
}

mod bits_prod_cons_proto {
    pub const P00G_BIT: usize = 0;
    pub const P01P_BIT: usize = 1;
}
struct ProdConsProto {
    p00g: Getter<u32>,
    p01p: Putter<u32>,
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        use bits_prod_cons_proto::*;

        // Create poller and register ports
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(32);
        let a = Ready::all();
        let edge = PollOpt::edge();
        poll.register(&self.p00g, Token(P00G_BIT), a, edge).unwrap();
        poll.register(&self.p01p, Token(P01P_BIT), a, edge).unwrap();

        // define the guards
        let mut guards = vec![];
        guard_cmd!(guards,
            bitset! {P00G_BIT, P01P_BIT},
            |_me: &mut Self| true,
            |me: &mut Self| me.p01p.put(me.p00g.get()?).map_err(discard!())
        );
        let mut active_guards: HashSet<_> = (0..guards.len()).collect();

        let mut ready = BitSet::new();
        while !active_guards.is_empty() {
            poll.poll(&mut events, None).unwrap();
            for event in events.iter() {
                ready.insert(event.token().0);
            }
            for (guard_idx, g) in guards.iter().enumerate() {
                if active_guards.contains(&guard_idx) // guard is active
                && ready.is_superset(&g.0) // firing constraint
                && (g.1)(self) // data constraint
                {
                    ready.difference_with(&g.0); // remove fired ports from ready set
                    if (g.2)(self).is_err() {
                        // apply change and make guard inactive if any port dies
                        active_guards.remove(&guard_idx);
                    };
                }
            }
        }
    }
}

#[test]
fn sync() {
    // create ports
    let (p00p, p00g) = reo::new_port();
    let (p01p, p01g) = reo::new_port();

    // spin up threads
    crossbeam::scope(|s| {
        s.builder()
            .name("Producer".into())
            .spawn(move |_| Producer { p00p }.run())
            .unwrap();
        s.builder()
            .name("ProdConsProto".into())
            .spawn(move |_| ProdConsProto { p00g, p01p }.run())
            .unwrap();
        s.builder()
            .name("Consumer".into())
            .spawn(move |_| Consumer { p01g }.run())
            .unwrap();
    })
    .expect("A worker thread panicked!");   
}
