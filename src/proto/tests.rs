use crate as reo_rs;
use crossbeam;
use reo_rs::{
    proto::{
        definition::{ActionDef, Formula, LocKind, ProtoDef, RuleDef, TypelessProtoDef},
        reflection::TypeInfo,
        traits::{HasUnclaimedPorts, MemFillPromise, MemFillPromiseFulfilled, Parsable, Proto},
        Getter, Putter,
    },
    LocId,
};

struct AlternatorProto<T0: Parsable> {
    phantom: std::marker::PhantomData<(T0,)>,
}
impl<T0: Parsable> Proto for AlternatorProto<T0> {
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
                    3 => LocKind::MemInitialized,
                },
            };
        }
        &DEF
    }
    fn fill_memory(loc_id: LocId, p: MemFillPromise) -> Option<MemFillPromiseFulfilled> {
        Some(match loc_id {
            3 => p.fill_memory(T0::try_parse("2368")?).ok()?,
            _ => return None,
        })
    }
    fn loc_type(loc_id: LocId) -> Option<TypeInfo> {
        Some(match loc_id {
            0..=3 => TypeInfo::new::<T0>(),
            _ => return None,
        })
    }
}
#[test]
fn instantiate_alternator() {
    let p = AlternatorProto::<u32>::instantiate();
    // return;

    use std::convert::TryInto;
    let mut p0: Putter<u32> = p.claim(0).try_into().unwrap();
    let mut p1: Putter<u32> = p.claim(1).try_into().unwrap();
    let mut p2: Getter<u32> = p.claim(2).try_into().unwrap();

    const N: u32 = 3;
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..N {
                p0.put(i);
            }
        });
        s.spawn(move |_| {
            for i in 0..N {
                p1.put(i + 100);
            }
        });
        s.spawn(move |_| {
            for _ in 0..(2 * N) {
                let v = p2.get();
                println!("v={:?}", v);
            }
        });
    })
    .expect("Crashed!");
}
