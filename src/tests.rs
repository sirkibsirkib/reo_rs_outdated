use crate::reo::*;
use hashbrown::HashMap;
// use crate::protocols::*;

use bit_set::BitSet;
use crossbeam::channel::{select, Select};
use crossbeam::scope;
use std::time::Duration;

struct Producer {
    p00p: PortPutter<u32>,
}
impl Component for Producer {
    fn run(&mut self) {
        for i in 0..10 {
            // std::thread::sleep(Duration::from_millis(500));
            self.p00p.put(i).unwrap();
        }
    }
}

struct Consumer {
    p01g: PortGetter<u32>,
}
impl Component for Consumer {
    fn run(&mut self) {
        while let Ok(x) = self.p01g.get() {
            println!("cons {:?}", x);
        }
    }
}

struct ProdConsProto {
    p00g: PortGetter<u32>,
    p01p: PortPutter<u32>,
}
impl ProdConsProto {
    const P00G_BIT: usize = 0;
    const P01P_BIT: usize = 1;
}
impl Component for ProdConsProto {
    fn run(&mut self) {
        let mut running = true;
        let mut guards = vec![];
        guard_cmd!(
            guards,
            bitset! {Self::P00G_BIT, Self::P01P_BIT},
            || true,
            || self.p01p.put(self.p00g.get()?)
        );

        let mut ready = BitSet::new();
        while running {
            let mut sel = Select::new();
            if !ready.contains(Self::P00G_BIT) {
                sel.recv(self.p00g.inner());
            }
            if !ready.contains(Self::P01P_BIT) {
                sel.send(self.p01p.inner());
            }

            println!("blocking.. ");
            let sel_flagged = sel.ready();

            ready.insert(sel_flagged); // assume identity function!
            println!("ready (flagged: {:?}) all: {:?}", sel_flagged, &ready);
            for g in guards.iter() {
                println!("readY: {:?} this guard needs {:?}", &ready, &g.0);
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

#[derive(Debug)]
struct MyType(u32);
impl Drop for MyType {
    fn drop(&mut self) {
        println!("MYDROP {:?}", self.0);
    }
}

#[test]
fn port() {
    use std::time::Duration;
    let (mut a, mut b) = crate::port::new_port();
    scope(|s| {
        s.spawn(|_| {
            std::thread::sleep(Duration::from_millis(2000));
            a.put(Box::new(MyType(0)));
                std::thread::sleep(Duration::from_millis(250));
            a.put(Box::new(MyType(1)));
                std::thread::sleep(Duration::from_millis(250));
            a.put(Box::new(MyType(2)));
            println!("T1 (putter) exit");
        });
        s.spawn(|_| {
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(100));
                println!("peek1 {:?}", b.peek());
                std::thread::sleep(Duration::from_millis(100));
                println!("peek2 {:?}", b.peek());
                std::thread::sleep(Duration::from_millis(100));
                println!("get {:?}", b.get());
            }
            println!("T2 (getter) exit");
        });
    })
    .unwrap();
    println!("main waiting...");
    std::thread::sleep(Duration::from_millis(2000));
    println!("MAIN DONE");
}

// #[test]
// fn sync() {
// 	let (p00p, p00g) = new_port();
// 	let (p01p, p01g) = new_port();
// 	scope(|s| {
// 		s.builder().name("Producer".into()).spawn(
// 			|_| Producer{p00p}.run()).unwrap();
// 		s.builder().name("ProdConsProto".into()).spawn(
// 			|_| ProdConsProto{p00g, p01p}.run()).unwrap();
// 		s.builder().name("Consumer".into()).spawn(
// 			|_| Consumer{p01g}.run()).unwrap();
// 	}).unwrap()
// }
