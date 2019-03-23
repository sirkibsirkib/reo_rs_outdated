// use crate::reo::*;
// use crate::port::PortEvent;
use hashbrown::HashMap;
use crate::ports2::*;
use crate::protocols::*;

use bit_set::BitSet;
// use crossbeam::channel::{select, Select};
use crossbeam::scope;
use std::time::Duration;

#[test]
fn port_test() {
    use crate::ports2::*;
    let (p, mut g) = new_port();
    println!("spawning threads...");
    scope(|s| {
        s.spawn(move |_| {
            println!("put start...");
            p.put(5).unwrap();
            println!("put 5");
            p.put(2).unwrap();
            println!("put 2");
            println!("put ok");
        });
        s.spawn(|_| {
            let sel = Selector::default();
            g.register_with(&sel, 0);
            loop {
                println!("waiting...");
                let ev = sel.wait_timeout(Duration::from_millis(2000));
                println!("ev {:?}", ev);
                match ev {
                    Some(PortEvent::GetReady(0)) => println!("GOT {:?}", g.get()),
                    None => break,
                    _ => {}
                }
            }
        });
    }).unwrap();
}

struct Producer {
    p00p: Putter<u32>,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..10 {
            // std::thread::sleep(Duration::from_millis(500));
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
        std::thread::sleep(Duration::from_millis(2000));
        // create the selector to block on
        let sel = Selector::default();
        self.p00g.register_with(&sel, Self::P00G_BIT);
        self.p01p.register_with(&sel, Self::P01P_BIT);

        // define the guards
        let mut guards = vec![];
        guard_cmd!(
            guards,
            bitset! {Self::P00G_BIT, Self::P01P_BIT},
            || true,
            || self.p01p.put(self.p00g.get()?).map_err(|_| ())
        );

        let mut running = true;
        let mut ready = BitSet::new();
        while running {
            println!("~~~ proto wait...");
            let ev = {
                let ev = sel.wait_timeout(Duration::from_millis(3000));
                if ev.is_none() {
                    println!("proto giving up");
                    return;
                }
                ev.unwrap()
            };
            match ev {
                PortEvent::GetReady(token) => { ready.insert(token); },
                PortEvent::PutReady(token) => { ready.insert(token); },
                PortEvent::Dropped(_) => unimplemented!(),
            }
            println!("... proto wait done");
            println!("token was {}", ev.token());
            for g in guards.iter() {
                println!("ready: {:?} this guard needs {:?}", &ready, &g.0);
                if ready.is_superset(&g.0) {
                    println!("firing ready!");
                    if (g.1)() {
                        // check data const
                        println!("data pass!");
                        if (g.2)().is_err() {
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
		s.builder().name("Producer".into()).spawn(
			move |_| Producer{p00p}.run()).unwrap();
		s.builder().name("ProdConsProto".into()).spawn(
			move |_| ProdConsProto{p00g, p01p}.run()).unwrap();
		s.builder().name("Consumer".into()).spawn(
			move |_| Consumer{p01g}.run()).unwrap();
	}).unwrap()
}
