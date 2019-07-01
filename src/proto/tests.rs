use self::reo_rs::{
    proto::{
        definition::{ActionDef, BehaviourDef, Formula, LocKind, RuleDef, TypelessProtoDef},
        reflection::TypeInfo,
        traits::{HasUnclaimedPorts, MemFillPromise, MemFillPromiseFulfilled, Proto},
        Getter, Putter,
    },
    LocId,
};
use crate as reo_rs;
use crossbeam;
use parking_lot::Mutex;
use rand::{thread_rng, Rng};
use std::{sync::Arc, thread, time::Duration};

fn dur(x: u64) -> Duration {
    Duration::from_millis(x)
}

#[derive(Debug, Clone)]
struct DropCounter(Arc<Mutex<u32>>);
impl Drop for DropCounter {
    fn drop(&mut self) {
        *self.0.lock() += 1;
    }
}
struct AlternatorProto<T0: 'static> {
    phantom: std::marker::PhantomData<(T0,)>,
}
impl<T0: 'static> Proto for AlternatorProto<T0> {
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref DEF: TypelessProtoDef = TypelessProtoDef {
                behaviour: BehaviourDef {
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
    type Interface = (Putter<T0>, Putter<T0>, Getter<T0>);
    fn instantiate_and_claim() -> Self::Interface {
        let p = Self::instantiate();
        putters_getters![p => 0,1,2]
    }
}
#[test]
fn proto_alt_u32_build() {
    let _ = AlternatorProto::<u32>::instantiate();
}
#[test]
fn proto_alt_u32_build_repeatedly() {
    let p = AlternatorProto::<u32>::instantiate();
    use std::convert::TryInto;

    let a: Putter<u32> = p.claim(0).try_into().unwrap();
    drop(a);
    let a: Putter<u32> = p.claim(0).try_into().unwrap();
    drop(a);
    let a: Putter<u32> = p.claim(0).try_into().unwrap();
    drop(a);
}
#[test]
fn proto_alt_u32_instantiate_and_claim() {
    let (_, _, _) = AlternatorProto::<u32>::instantiate_and_claim();
}

#[test]
fn proto_alt_u32_claim() {
    let p = AlternatorProto::<u32>::instantiate();
    use std::convert::TryInto;
    for i in 0..3 {
        assert!(p.claim::<Putter<u16>>(i).claimed_nothing());
        assert!(p.claim::<Getter<u16>>(i).claimed_nothing());
    }
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
    let dc = DropCounter(Arc::new(Mutex::new(0)));
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
                        }
                        false => {
                            println!("GETTING VAL");
                            p2.get();
                        }
                    }
                }
            }
        });
    })
    .expect("Crashed!");
    assert_eq!(*dc.0.lock(), N * 2);
}

////////////////////////////////////////////////////////////////////////

struct SyncProto<T0: 'static> {
    phantom: std::marker::PhantomData<(T0,)>,
}
impl<T0: 'static> Proto for SyncProto<T0> {
    fn typeless_proto_def() -> &'static TypelessProtoDef {
        lazy_static::lazy_static! {
            static ref DEF: TypelessProtoDef = TypelessProtoDef {
                behaviour: BehaviourDef {
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
    type Interface = (Putter<T0>, Getter<T0>);
    fn instantiate_and_claim() -> Self::Interface {
        let p = Self::instantiate();
        putters_getters![p => 0,1]
    }
}

#[test]
fn proto_sync_f64_create() {
    let _p = SyncProto::<f64>::instantiate();
}
#[test]
fn proto_sync_f64_instantiate_and_claim() {
    let (_, _) = SyncProto::<f64>::instantiate_and_claim();
}
#[test]
fn proto_sync_f64_basic() {
    let (mut p0, mut p1) = SyncProto::<f64>::instantiate_and_claim();
    const N: u32 = 10;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                assert!(p0.put(i as f64).is_none());
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                assert_eq!(p1.get(), i as f64);
            }
        });
    })
    .expect("Crashed!");
}

#[test]
fn proto_sync_u8_put_timeout() {
    let (mut p0, mut p1) = SyncProto::<u8>::instantiate_and_claim();
    const N: u8 = 5;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                assert!(!p0.put_timeout(i, dur(100)).moved()); // times out
                assert!(p0.put_timeout(i, dur(100)).moved()); // succeeds
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                thread::sleep(dur(150));
                assert_eq!(p1.get(), i);
            }
        });
    })
    .expect("Crashed!");
}

#[test]
fn proto_sync_string_in_place() {
    let (mut p0, mut p1) = SyncProto::<String>::instantiate_and_claim();
    const N: u32 = 10;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                unsafe {
                    let mut src = std::mem::MaybeUninit::new(format!("STRING #{}.", i));
                    let sent = p0.put_in_place(src.as_mut_ptr());
                    assert!(sent);
                }
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                let mut dest = std::mem::MaybeUninit::uninit();
                unsafe {
                    p1.get_in_place(dest.as_mut_ptr());
                    let value = dest.assume_init();
                    assert_eq!(&value, &format!("STRING #{}.", i));
                    println!("{:?}", value);
                }
            }
        });
    })
    .expect("Crashed!");
}
#[test]
fn proto_sync_counter_in_place() {
    let dc = DropCounter(Arc::new(Mutex::new(0)));
    let (mut p0, mut p1) = SyncProto::<DropCounter>::instantiate_and_claim();
    const N: u32 = 10;
    crossbeam::scope(|s| {
        s.spawn(|_| {
            for _i in 0..N {
                unsafe {
                    let mut src = std::mem::MaybeUninit::new(dc.clone());
                    let sent = p0.put_in_place(src.as_mut_ptr());
                    assert!(sent);
                }
            }
        });
        s.spawn(|_| {
            for _i in 0..N {
                let mut dest = std::mem::MaybeUninit::uninit();
                unsafe {
                    p1.get_in_place(dest.as_mut_ptr());
                    let value = dest.assume_init();
                    println!("got value {:?}. now dropping!", &value);
                }
            }
        });
    })
    .expect("Crashed!");
    assert_eq!(*dc.0.lock(), N);
}

#[test]
fn proto_sync_f64_chain() {
    use std::convert::TryInto;

    let p = SyncProto::<f64>::instantiate();
    let mut p0: Putter<f64> = p.claim(0).try_into().unwrap();
    let mut p1: Getter<f64> = p.claim(1).try_into().unwrap();

    let p = SyncProto::<f64>::instantiate();
    let mut p2: Putter<f64> = p.claim(0).try_into().unwrap();
    let mut p3: Getter<f64> = p.claim(1).try_into().unwrap();

    const N: u32 = 10;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                assert!(p0.put(i as f64).is_none());
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                let val = p1.get();
                assert_eq!(val, i as f64);
                assert!(p2.put(val).is_none());
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                let val = p3.get();
                assert_eq!(val, i as f64);
            }
        });
    })
    .expect("Crashed!");
}
