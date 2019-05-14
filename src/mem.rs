

// // use hashbrown::{HashMap, HashSet};
// // use crate::bitset::BitSet;
// // use smallvec::SmallVec;
// // use std_semaphore::Semaphore;
// // use std::sync::atomic::{AtomicU8, Ordering};
// // use std::mem::ManuallyDrop;

// // type PortId = usize;

// // trait PortDatum {
// // 	fn clone_to(&self, other: *mut ());
// // }

// // struct DatumValue {
// // 	value: ManuallyDrop<Box<dyn PortDatum>>,
// // 	owned: AtomicU8,
// // }
// // impl DatumValue {
// // 	const DANGLING: u8 = 0; // 
// // 	const OWNED: u8 = 1; // the CONTENTS must be dropped
// // 	const BOX: u8 = 2; // the CONTENTS + BOX must be dropped
// // }

// // impl Drop for DatumValue {
// // 	fn drop(&mut self) {
// // 		if self.owned.swap(false, Ordering::SeqCst) {
// // 			unsafe {
// // 				ManuallyDrop::drop(&mut self.value)
// // 			}
// // 		}
// // 	}
// // }

// // struct DataSlot {
// // 	datum_value: DatumValue,
// // 	rw_sema: Semaphore,
// // }
// // struct MsgDropbox {
// // 	full_sema: Semaphore, 
// // 	msg: usize,
// // }

// // struct Action {
// // 	putter_id: PortId,
// // 	getter_ids: Vec<PortId>,
// // }
// // struct Rule {
// // 	guard_1: BitSet,
// // 	guard_2: fn(&Proto) -> bool,
// // 	actions: SmallVec<[Action;2]>,
// // }

// // struct ProtoCr {
// // 	ready: BitSet,
// // 	rules: Vec<Rule>,
// // }

// // /*
// // [mem_putter | port_putter | port_getter | mem_getter ]
// // ~~~~~~~~~~~~ = num_mem
// //  ^^^^^^^^^^^^^^^^^^^^^^^^^ data-spaces

// // */
// // struct Proto {
// // 	mem_ids: usize,
// // 	data_slots: Vec<DataSlot>,
// // 	msg_dropboxes: Vec<MsgDropbox>,
// // }


// // struct ProtoBuilder {
// // 	mem: HashMap<PortId, Option<Box<dyn PortDatum>>>,
// // 	rules: Vec<Vec<Action>>,
// // }
// // impl ProtoBuilder {
// // 	fn new_with_mem(mem: HashMap<PortId, Option<Box<dyn PortDatum>>>) -> Self {
// // 		Self {
// // 			mem,
// // 			rules: vec![],
// // 		}
// // 	}

// // 	fn with_rule(mut self, actions: Vec<Action>) -> Self {
// // 		self.rules.push(actions);
// // 		self
// // 	}

// // 	fn build(mut self) -> Result<Proto,ProtoBuildError> {
// // 		use ProtoBuildError::*;
// // 		let mem_ids = self.mem.len();
// // 		let mut renaming = HashMap::default();
// // 		let mut data_slots = Vec::with_capacity(mem_ids);
// // 		for (id, (outer_id, maybe_slot)) in self.mem.into_iter().enumerate() {
// // 			renaming.insert(outer_id, id);
// // 			data_slots.push(DataSlot {
// // 				rw_sema: Semaphore::new(0),
// // 				datum_value: match maybe_slot {
// // 					Some(m) => DatumValue {
// // 						owned: true.into(),
// // 						value: ManuallyDrop::new(m),
// // 					},
// // 					None => DatumValue {
// // 						owned: false.into(),
// // 						value: unsafe {
// // 							ManuallyDrop::new(std::mem::uninitialized())
// // 						}
// // 					},
// // 				}
// // 			})
// // 		}
// // 		// let mut renaming: HashMap<_,_> = self.mem.keys().cloned().enumerate().collect();
// // 		let mut port_putters: HashSet<PortId> = HashSet::default();
// // 		let mut port_getters: HashSet<PortId> = HashSet::default();
// // 		for a in self.rules.iter().flat_map(|r| r) {
// // 			if renaming.contains_key(&a.putter_id) {
// // 				// mem id
// // 			} else if port_getters.contains(&a.putter_id) {
// // 				return Err(PortIsBidirectional(a.putter_id))
// // 			} else {
// // 				port_putters.insert(a.putter_id);
// // 			}
// // 			for gid in a.getter_ids {
// // 				if renaming.contains_key(&a.putter_id) {
// // 					// mem id
// // 				} else if port_putters.contains(&a.putter_id) {
// // 					return Err(PortIsBidirectional(a.putter_id))
// // 				} else {
// // 					port_getters.insert(a.putter_id);
// // 				}
// // 			}
// // 		}
// // 		let num_port_putters = port_putters.len();
// // 		let num_port_getters = port_getters.len();
// // 		let mut next_id = mem_ids;
// // 		for id in port_putters {
// // 			data_slots.push()
// // 			renaming.insert(next_id, id);
// // 		}
// // 		for id in port_getters {
// // 			renaming.insert(next_id, id);
// // 		}
// // 		let data_slots = 
// // 		Proto {
// // 			mem_ids,
// // 			data_slots: Vec<DataSlot>,
// // 			msg_dropboxes: Vec<MsgDropbox>,
// // 		}
// // 	}
// // }

// // enum ProtoBuildError {
// // 	PortIsBidirectional(PortId)
// // }


// trait Proto {
// 	const NUM_MEMS: usize;
// 	const NUM_PORT_PUTS: usize;
// 	const NUM_PORT_GETS: usize;
// }

// struct AltProto {

// }
// impl Proto for AltProto {
// 	const NUM_MEMS: usize = 1;
// 	const NUM_PORT_PUTS: usize = 1;
// 	const NUM_PORT_GETS: usize = 2;
// }

#[test]
pub fn box_swap() {
	let mut x = Box::new("X");
	let mut y = Box::new("Y");
	std::mem::swap(&mut x, &mut y);
	println!("{:?}, {:?}", x, y);
}