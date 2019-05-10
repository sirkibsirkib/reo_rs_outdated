use hashbrown::HashSet;
use crate::bitset::BitSet;
use hashbrown::HashMap;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std_semaphore::Semaphore;

use parking_lot::Mutex;

type SetVec<T> = Vec<T>; // sorted, dedup
type PortId = usize;
type RuleId = usize;
struct Action {
    putter: PortId,
    getters: SetVec<PortId>,
}
struct Rule {
    guard: BitSet,
    actions: Vec<Action>,
}

unsafe impl Send for Ptr {}
unsafe impl Sync for Ptr {}
struct Ptr(UnsafeCell<*const ()>);
impl Default for Ptr {
    fn default() -> Self {
        Self(UnsafeCell::new(std::ptr::null()))
    }
}
impl Ptr {
    fn transfer_moving<T: Sized>(&self, dst: &Self) {
        unsafe {
            let src: *const T = std::mem::transmute(*self.0.get());
            let dst: *mut T = std::mem::transmute(*dst.0.get());
            std::ptr::copy(src, dst, 1);
        }
    }
    fn write<T: Sized>(&self, datum: &T) {
        unsafe { *self.0.get() = std::mem::transmute(datum) };
    }
    fn read_moving<T: Sized>(&self) -> T {
        unsafe {
            let x: *const T = std::mem::transmute(*self.0.get());
            x.read()
        }
    }
    fn read_cloning<T: Sized + Clone>(&self) -> T {
        unsafe {
            let x: &T = std::mem::transmute(*self.0.get());
            x.clone()
        }
    }
}
struct PutterSpace {
    ptr: Ptr,
    getters_sema: Semaphore,
    owned: AtomicBool,
}

unsafe impl Send for MsgDropbox {}
unsafe impl Sync for MsgDropbox {}
struct MsgDropbox {
    sema: Semaphore,
    msg: UnsafeCell<usize>,
}
impl Default for MsgDropbox {
    fn default() -> Self {
        Self {
            sema: Semaphore::new(0),
            msg: 0.into(),
        }
    }
}
impl MsgDropbox {
    fn send(&self, msg: usize) {
        unsafe { *self.msg.get() = msg }
        self.sema.release(); // += 1
    }
    fn recv(&self) -> usize {
        self.sema.acquire(); // -= 1
        unsafe { *self.msg.get() }
    }
}

struct ProtoCr {
    ready: BitSet,
    rules: Vec<Rule>,
}
impl ProtoCr {
    fn enter(&mut self, proto: &Proto, goal: PortId) {
        self.ready.set(goal);
        loop {
            for (_rule_id, rule) in self.rules.iter().enumerate() {
                if self.ready.is_superset(&rule.guard) {
                    self.ready.difference_with(&rule.guard);
                    self.fire(proto, rule);
                    let goal_met = !self.ready.test(goal);
                    if goal_met {
                        return;
                    }
                }
            }
        }
    }

    fn fire(&self, proto: &Proto, rule: &Rule) {
        // mem getters only
        for action in rule.actions.iter() {
            let mut mg_iter = action.getters.iter().filter(|g| proto.is_mem.contains(&action.putter));
            // let num_tot_getters = action.getters.len();
            // let num_mem_getters = mg_iter.clone().count();
            // let num_prt_getters = num_tot_getters - num_mem_getters;

            let src_space = proto.spaces.get(&action.putter).expect("UII");
            match mg_iter.next() {
                Some(mg) => {
                    // memory mv
                    let mv = src_space.owned.swap(false, Ordering::SeqCst);
                    assert_eq!(mv, true);
                },
                None => (),
            }
            for mg in mg_iter {
                let dst_space = proto.spaces.get(&mg).expect("UUAAA");
                // TODO clone instead
                src_space.ptr.transfer_moving::<u32>(&dst_space.ptr);
            }
        }
    }
}
struct Proto {
    cr: Mutex<ProtoCr>,
    // TODO use vecs instead of hashmaps
    spaces: HashMap<PortId, PutterSpace>,
    is_mem: HashSet<PortId>,
    messaging: HashMap<PortId, MsgDropbox>,
}

struct MemoryDef {
    ptr: Ptr,
    initialized: bool,
}
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum PortRole {
    Putter,
    Getter,
}
struct ProtoBuilder {
    memory: HashMap<PortId, MemoryDef>,
    port_type: HashMap<PortId, PortRole>,
    rules: Vec<Rule>,
}
impl ProtoBuilder {
    fn with_memory(memory: HashMap<PortId, MemoryDef>) -> Self {
        Self {
            memory,
            rules: vec![],
            port_type: Default::default(),
        }
    }
    fn id_played_role(&mut self, id: PortId, role: PortRole) {
        if !self.memory.contains_key(&id) {
            let prev = self.port_type.entry(id).or_insert(role);
            if *prev != role {
                panic!("MISMATCH {:?} != {:?}", prev, role);
            }
        }
    }
    fn with_rule(mut self, mut actions: Vec<Action>) -> Self {
        let mut guard = BitSet::default();
        for a in actions.iter_mut() {
            self.id_played_role(a.putter, PortRole::Putter);
            guard.set(a.putter);
            a.getters.sort();
            a.getters.dedup();
            for &g in a.getters.iter() {
                self.id_played_role(g, PortRole::Getter);
                guard.set(g);
            }
        }
        self.rules.push(Rule { guard, actions });
        self
    }
    fn need_spaces_iter(&self) -> impl Iterator<Item = PortId> + '_ {
        self.port_type
            .iter()
            .filter_map(|(id, role)| match role {
                PortRole::Getter => None,
                PortRole::Putter => Some(*id),
            })
            .chain(self.memory.keys().cloned())
    }
    fn build(self) -> Proto {
        let proto_cr = ProtoCr {
            ready: BitSet::default(),
            rules: self.rules,
        };
        let messaging = self
            .memory
            .keys()
            .chain(self.port_type.keys())
            .map(|&id| {
                (
                    id,
                    MsgDropbox {
                        sema: Semaphore::new(0),
                        msg: 0.into(),
                    },
                )
            })
            .collect();
        let is_mem = self.memory.keys().cloned().collect();
        let spaces = self
            .port_type
            .iter()
            .filter_map(|(id, role)| match role {
                PortRole::Getter => None,
                PortRole::Putter => Some((
                    *id,
                    PutterSpace {
                        ptr: Ptr::default(),
                        getters_sema: Semaphore::new(0),
                        owned: false.into(),
                    },
                )),
            })
            .chain(self.memory.into_iter().map(|(id, def)| {
                (
                    id,
                    PutterSpace {
                        ptr: def.ptr,
                        getters_sema: Semaphore::new(0),
                        owned: def.initialized.into(),
                    },
                )
            }))
            .collect();
        Proto {
            is_mem,
            cr: Mutex::new(proto_cr),
            spaces,
            messaging,
        }
    }
}

#[derive(derive_new::new)]
struct Putter<T> {
    id: PortId,
    proto: Arc<Proto>,
    #[new(default)]
    data_type: PhantomData<T>,
}
impl<T> Putter<T> {
    fn put(&mut self, datum: T) -> Option<T> {
        let space = self.proto
            .spaces
            .get(&self.id)
            .expect("NO SPACE");
        let msg_dropbox = self.proto
            .messaging
            .get(&self.id)
            .expect("NO MSG");
        space.ptr.write(&datum);
        space.owned.store(true, Ordering::SeqCst);
        self.proto.cr.lock().enter(&self.proto, self.id);
        let num_getters = msg_dropbox.recv();
        for _ in 0..num_getters {
            space.getters_sema.acquire();
        }
        if space.owned.swap(false, Ordering::SeqCst) {
            Some(datum)
        } else {
            std::mem::forget(datum);
            None
        }
    }
}

#[derive(derive_new::new)]
struct Getter<T> {
    id: PortId,
    proto: Arc<Proto>,
    #[new(default)]
    data_type: PhantomData<T>,
}
impl<T> Getter<T> {
    fn get(&mut self) -> T {
        let msg_dropbox = self.proto
            .messaging
            .get(&self.id)
            .expect("NO MSG");
        self.proto.cr.lock().enter(&self.proto, self.id);
        let putter = msg_dropbox.recv();
        let space = self.proto
            .spaces
            .get(&putter)
            .expect("NO SPACE");
        let mv = space.owned.swap(false, Ordering::SeqCst);
        let datum = if mv {
            space.ptr.read_moving()
        } else {
            panic!("Cannot clone!")
            // space.ptr.read_cloning()
        };
        space.getters_sema.release();
        datum
    }
}

#[test]
pub fn tabaccee() {
    let proto = ProtoBuilder::with_memory(map! {})
        .with_rule(vec![Action {
            putter: 0,
            getters: vec![1],
        }])
        .build();
    let proto = Arc::new(proto);

    let mut p0 = Putter::<u32>::new(0, proto.clone());
    let mut p1 = Getter::<u32>::new(1, proto.clone());
    println!("OK");

    crossbeam::scope(|s| {
        s.spawn(move |_| {
            for i in 0..10 {
                p0.put(i);
            }
        });
        s.spawn(move |_| {
            for _ in 0..10 {
                let x = p1.get();
                println!("{:?}", x);
            }
        });
    })
    .expect("EY");
}
