use std::ptr::NonNull;
use crate::bitset::BitSet;
use std::any::TypeId;

#[derive(Debug, Copy, Clone)]
struct Ppid(usize);
#[derive(Debug, Copy, Clone)]
struct Pgid(usize);
#[derive(Debug, Copy, Clone)]
struct Mpid(usize);
#[derive(Debug, Copy, Clone)]
struct Mgid(usize);

pub trait BitIndexed: Copy {
	fn bit(self) -> usize;
}
impl BitIndexed for Ppid {
	fn bit(self) -> usize {self.0}
}
impl BitIndexed for Pgid {
	fn bit(self) -> usize {self.0}
}
impl BitIndexed for Mpid {
	fn bit(self) -> usize {self.0}
}
impl BitIndexed for Mgid {
	fn bit(self) -> usize {self.0}
}

enum Pid {
	Port(Ppid),
	Mem(Mpid),
}
enum Gid {
	Port(Pgid),
	Mem(Mgid),
}

struct UntypedPtr(*const ());



struct Action {
	tid: TypeId,
	src: Pid,
	dests: Vec<Gid>,
	clone_from_fn: Option<CloneFromFn>,
	drop_fn: Option<DropFn>,
}

enum Evaluable {
	True,
	False,
	Or(Vec<Evaluable>),
	And(Vec<Evaluable>),
	None(Vec<Evaluable>),
}

type CloneFromFn = fn(UntypedPtr, UntypedPtr);
type EqFn = fn(UntypedPtr, UntypedPtr) -> bool;
type DropFn = fn(UntypedPtr);


struct Guard {
	must_be_ready: BitSet,
	constraint: Evaluable,
	actions: Vec<Action>,
}

pub struct Proto {
	guards: [Guard; 1],
	memory: (),
}
impl Default for Proto {
	fn default() -> Self {
		Self {
			guards: [
				Guard {
					must_be_ready: bitset!{0,1},
					constraint: Evaluable::True,
					actions: vec![
						Action {
							tid: TypeId::of::<u32>(),
							src: Pid::Port(Ppid(0)),
							dests: vec![
								Gid::Port(Pgid(1)),
							],
							clone_from_fn: Some(<u32 as CloneFrom>::clone_from),
							drop_fn: Some(trivial_drop),
						}
					],
				},
			],
			memory: (),
		}
	}
}

trait CloneFrom: Clone {
	fn clone_from(src: UntypedPtr, dest: UntypedPtr) {

	}
} 
impl<T> CloneFrom for T where T: Clone {}


trait DropAt: Drop {
	fn drop_at(me: UntypedPtr) {

	}
}
impl<T> DropAt for T where T: Drop {}


fn trivial_drop(x: UntypedPtr) {}



type CloneFn<T> = fn(&T) -> T;
fn foo<T>(t: T, clone_fn: Option<CloneFn<T>>) -> T {
	(clone_fn.expect("O SHIT"))(&t)
}

