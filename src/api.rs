//// EXAMPLE //////
// 1. imports
use crate::proto::*;
use crate::tokens::*;
use std::marker::PhantomData;

// 2. state definition
pub struct State {
    phantom: PhantomData<()>,
}
unsafe impl Token for State {}
impl SomeState for State {
    fn new_predicate() -> StatePred {
        StatePred::new(vec![])
        // StatePred::new(vec![A::as_var(), B::as_var(), C::as_var()])
    }
}
pub trait SomeState {
    fn new_predicate() -> StatePred;
}

// rules {0}, not rules {}
pub enum Rules1 {
    R1(Coupon<E0, State>),
}
impl<T: 'static +  TryClone> Transition<SyncProto<T>> for Rules1 {
    fn from_rule_id(rule_id: RuleId) -> Self {
        match rule_id {
            0 => Rules1::R1(unsafe { Coupon::fresh() }),
            wrong => panic!("panic in Rules1 with {}", wrong),
        }
    }
}
impl<T: 'static +  TryClone> Advance<SyncProto<T>> for State {
    type Opts = Rules1;
}

// 3. type aliases
type P<T> = SyncProto<T>;
type Interface<T> = (Putter<T, P<T>>,);
type SafeInterface<T> = (Safe<E0, Putter<T, P<T>>>,);

// 4. component constructor function
fn new_atomic<F, S: SomeState, T: TryClone + 'static>(
    interface: Interface<T>,
    f: F,
) -> Result<(), GroupMakeError>
where
    F: FnOnce(PortGroup<P<T>>, S, SafeInterface<T>),
{
    let i = interface;
    let port_slice: &[&Port<P<T>>] = &[&i.0,];
    let state_predicate = S::new_predicate();
    let port_group = PortGroup::new(state_predicate, port_slice)?;
    let safe_interface = (Safe::new(i.0),);
    let start_token = if std::mem::size_of::<S>() != 0 {
        panic!("BAD")
    } else {
        unsafe { std::mem::uninitialized() }
    };
    Ok(f(port_group, start_token, safe_interface))
}


// what the user would implement
type Pr = SyncProto<u32>;
fn atomic_fn(g: PortGroup<Pr>, mut start: State, (p0,): SafeInterface<u32>) {
    let g = &g;
    loop {
        start = start
        .advance(g, |o| match o {
            Rules1::R1(c) => p0.put(c, 32),
        });
    }
}

#[test]
pub fn api_test() {
    let (p, g) = SyncProto::<u32>::new();
    crossbeam::scope(|s| {
        s.spawn(move |_| {
            new_atomic((p,), atomic_fn).expect("ye");
        });
        s.spawn(move |_| {
            for _ in 0..10 {
                println!("{:?}", g.get());
            }
        });
    })
    .expect("Fail");
}



//////////////////// example 2 ///////////////// 


// // concrete proto. implements Proto trait
// pub(crate) struct AltProto<T> {
//     data_type: PhantomData<(T,)>,
// }
// impl<T: 'static + TryClone> Proto for AltProto<T> {
//     fn state_predicate(&self, _predicate: &StatePred) -> bool {
//         true
//     }
//     type Interface = (Putter<T, Self>, Getter<T, Self>);
//     fn interface_ids() -> &'static [PortId] {
//         &[0, 1]
//     }
//     fn build_guards() -> Vec<Guard<Self>> {
//         vec![Guard {
//             min_ready: bitset! {0,1},
//             constraint: |_cr| true,
//             action: data_move_action![1 => 0],
//         }]
//     }
//     fn new() -> <Self as Proto>::Interface {
//         let proto = Self {
//             data_type: Default::default(),
//         };
//         let (proto_common, mut r_out) = ProtoCommon::new(proto);
//         let proto_common = Arc::new(proto_common);
//         let mut commons = <Self as Proto>::interface_ids()
//             .iter()
//             .map(|id| PortCommon {
//                 id: *id,
//                 r_out: r_out.remove(id).unwrap(),
//                 proto_common: proto_common.clone(),
//             });
//         finalize_ports!(commons, Putter, Getter)
//     }
// }