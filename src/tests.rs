// use crate::reo::*;
// use crate::port::PortEvent;
// use hashbrown::HashMap;
// use crate::ports2::PortEventClass as Ev;
// use crate::ports2::*;
use crate::ports1::*;
use crate::protocols::*;

use bit_set::BitSet;
// use crossbeam::channel::{select, Select};
use crossbeam::scope;
use std::time::Duration;

// #[test]
// fn port_test() {
//     // use crate::ports2::*;
//     let (mut p, mut g) = new_port();
//     println!("spawning threads...");
//     scope(|s| {
//         s.spawn(move |_| {
//             p.put(5).unwrap();
//             p.put(2).unwrap();
//         });
//         s.spawn(|_| {
//             loop {
//                 println!("{:?}", g.get());
//             }
//         });
//     })
//     .unwrap();
// }

struct Producer {
    p00p: Putter<u32>,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..10 {
            std::thread::sleep(Duration::from_millis(500));
            println!("producer put...");
            self.p00p.put(i).unwrap();
            println!("... producer put done");
        }
    }
}

struct Consumer {
    p01g: Getter<u32>,
}
impl Component for Consumer {
    fn run(&mut self) {
        loop {
            println!("consumer cons...");
            if let Ok(x) = self.p01g.get() {
                println!("...cons got {:?}", x);
            } else {
                println!("cons got err. quitting");
                break;
            }
        }
    }
}

struct ProdConsProto {
    p00g: Getter<u32>,
    p01p: Putter<u32>,
}
impl ProdConsProto {
    const P00G_BIT: usize = 0;
    const P01P_BIT: usize = 1;
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        // std::thread::sleep(Duration::from_millis(2000));
        // create the selector to block on

        use mio::*;
        let poll = Poll::new().unwrap();
        poll.register(&self.p00g, Token(Self::P00G_BIT), Ready::readable(),
              PollOpt::edge()).unwrap();
        poll.register(&self.p01p, Token(Self::P01P_BIT), Ready::writable(),
              PollOpt::edge()).unwrap();

        // define the guards
        let mut guards = vec![];
        guard_cmd!(
            guards,
            bitset! {Self::P00G_BIT, Self::P01P_BIT},
            |_me: &mut Self| true,
            |me: &mut Self| me.p01p.put(me.p00g.get()?).map_err(|_| ())
        );

        let mut running = true;
        let mut ready = BitSet::new();
        let mut events = Events::with_capacity(1024);
        while running {
            poll.poll(&mut events, None).unwrap();
            for event in events.iter() {
                println!("event {:?}", &event);
                ready.insert(event.token().0);
                println!("TOKEN {}", event.token().0);
            }
            for g in guards.iter() {
                println!("ready: {:?} this guard needs {:?}", &ready, &g.0);
                if ready.is_superset(&g.0) {
                    println!("firing ready!");
                    if (g.1)(self) {
                        // check data const
                        println!("data pass!");
                        if (g.2)(self).is_err() {
                            running = false;
                        };
                        ready.difference_with(&g.0);
                    } else {
                        println!("data fail!");
                    }
                }
            }
        }
    }
}

#[test]
fn sync() {
    let (p00p, p00g) = new_port();
    let (p01p, p01g) = new_port();
    scope(|s| {
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
    .unwrap()
}
