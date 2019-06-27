use std::sync::Arc;
use rand::{Rng, thread_rng};
use crate as reo_rs;
use crossbeam;
use self::reo_rs::{
    proto::{
        definition::{ActionDef, Formula, LocKind, ProtoDef, RuleDef, TypelessProtoDef},
        reflection::TypeInfo,
        traits::{HasUnclaimedPorts, MemFillPromise, MemFillPromiseFulfilled, Parsable, Proto},
        Getter, Putter,
    },
    LocId,
};

struct AlternatorProto<T0: 'static> {
    phantom: std::marker::PhantomData<(T0,)>,
}
impl<T0: 'static> Proto for AlternatorProto<T0> {
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref DEF: TypelessProtoDef = TypelessProtoDef {
                structure: ProtoDef{
                    rules: vec![
                        rule![Formula::True; 0=>2; 1=>3],
                        rule![Formula::True; 3=>2],
                    ]
                },
                loc_kinds: map! {
                    0 => LocKind::PortPutter,
                    1 => LocKind::PortPutter,
                    2 => LocKind::PortGetter,
                    3 => LocKind::MemUninitialized,
                },
            };
        }
        &DEF
    }
    fn fill_memory(_loc_id: LocId, _p: MemFillPromise) -> Option<MemFillPromiseFulfilled> {
        None
    }
    fn loc_type(loc_id: LocId) -> Option<TypeInfo> {
        Some(match loc_id {
            0..=3 => TypeInfo::new::<T0>(),
            _ => return None,
        })
    }
}
#[test]
fn proto_alt_u32_build() {
    let _ = AlternatorProto::<u32>::instantiate();
}

#[test]
fn proto_alt_u32_claim() {
    let p = AlternatorProto::<u32>::instantiate();
    use std::convert::TryInto;
    let _: Putter<u32> = p.claim(0).try_into().unwrap();
    let _: Putter<u32> = p.claim(1).try_into().unwrap();
    let _: Getter<u32> = p.claim(2).try_into().unwrap();
    for i in 0..10 {
        assert!(p.claim::<Putter<u32>>(i).claimed_nothing());
        assert!(p.claim::<Getter<u32>>(i).claimed_nothing());
    }
}
#[test]
fn proto_alt_u32_basic() {
    let p = AlternatorProto::<u32>::instantiate();

    use std::convert::TryInto;
    let mut p0: Putter<u32> = p.claim(0).try_into().unwrap();
    let mut p1: Putter<u32> = p.claim(1).try_into().unwrap();
    let mut p2: Getter<u32> = p.claim(2).try_into().unwrap();

    const N: u32 = 3;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                assert!(p0.put(i).is_none());
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                assert!(p1.put(i + 100).is_none());
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                assert_eq!(p2.get(), i);
                assert_eq!(p2.get(), i + 100);
            }
        });
    })
    .expect("Crashed!");
}
#[test]
fn proto_alt_u32_signals() {
    let p = AlternatorProto::<u32>::instantiate();

    use std::convert::TryInto;
    let mut p0: Putter<u32> = p.claim(0).try_into().unwrap();
    let mut p1: Putter<u32> = p.claim(1).try_into().unwrap();
    let mut p2: Getter<u32> = p.claim(2).try_into().unwrap();

    const N: u32 = 10;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                // value returned
                assert!(p0.put(i).is_some());
            }
            println!("P0 done");
        });
        s.spawn(move |_| {
            for i in 0..N {
                // value dropped in circuit
                assert!(p1.put(i).is_none());
            }
            println!("P1 done");
        });
        s.spawn(move |_| {
            for _ in 0..N {
                assert_eq!(p2.get_signal(), ());
                assert_eq!(p2.get_signal(), ());
            }
            println!("P2 done");
        });
    })
    .expect("Crashed!");
}


#[test]
fn proto_alt_counting() {
    use parking_lot::Mutex;
    let ctr = Arc::new(Mutex::new(0));

    #[derive(Debug, Clone)]
    struct DropCounter(Arc<Mutex<u32>>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            *self.0.lock() += 1;
        }
    }

    let dc = DropCounter(ctr.clone());

    let p = AlternatorProto::<DropCounter>::instantiate();

    use std::convert::TryInto;
    let mut p0: Putter<DropCounter> = p.claim(0).try_into().unwrap();
    let mut p1: Putter<DropCounter> = p.claim(1).try_into().unwrap();
    let mut p2: Getter<DropCounter> = p.claim(2).try_into().unwrap();

    const N: u32 = 30;
    crossbeam::scope(|s| {
        s.spawn(|_| {
            for _i in 0..N {
                p0.put(dc.clone());
            }
        });
        s.spawn(|_| {
            for _i in 0..N {
                p1.put(dc.clone());
            }
        });
        s.spawn(|_| {
            let mut rng = thread_rng();
            for _ in 0..N {
                for _ in 0..2 {
                    match rng.gen() {
                        true => {
                            println!("GETTING SIG");
                            p2.get_signal();
                        },
                        false => {
                            println!("GETTING VAL");
                            p2.get();
                        },
                    }
                }
            }
        });
    })
    .expect("Crashed!");
    assert_eq!(*dc.0.lock(), N*2);
}

struct SyncProto<T0: Parsable> {
    phantom: std::marker::PhantomData<(T0,)>,
}
impl<T0: Parsable> Proto for SyncProto<T0> {
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref DEF: TypelessProtoDef = TypelessProtoDef {
                structure: ProtoDef{
                    rules: vec![
                        rule![Formula::True; 0=>1],
                    ]
                },
                loc_kinds: map! {
                    0 => LocKind::PortPutter,
                    1 => LocKind::PortGetter,
                },
            };
        }
        &DEF
    }
    fn fill_memory(_loc_id: LocId, _p: MemFillPromise) -> Option<MemFillPromiseFulfilled> {
        None
    }
    fn loc_type(loc_id: LocId) -> Option<TypeInfo> {
        Some(match loc_id {
            0..=1 => TypeInfo::new::<T0>(),
            _ => return None,
        })
    }
}

// #[test]
// fn proto_sync_u32_basic() {
//     let p = AlternatorProto::<u32>::instantiate();
//     use std::convert::TryInto;
//     let mut p0: Putter<u32> = p.claim(0).try_into().unwrap();
//     let mut p1: Getter<u32> = p.claim(1).try_into().unwrap();

//     const N: u32 = 3;
//     crossbeam::scope(|s| {
//         s.spawn(move |_| {
//             for i in 0..N {
//                 assert!(p0.put(i).is_none());
//             }
//         });
//         s.spawn(move |_| {
//             for i in 0..N {
//                 assert_eq!(p1.get(), i);
//             }
//         });
//     })
//     .expect("Crashed!");
// }

// test a normal moving sync with Arc<u32>
// test a normal moving sync with u32
// test replicator with u32
// test replicator with String
// test counter with u32
// test counter with String
