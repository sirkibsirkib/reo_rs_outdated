we need to avoid the scenario that is similar to a lost wakeup:

Rules:
1. m -> a
2. b -> m

Trace:
1. a: [enters, marks a as ready. no rules can fire. a finishes enter.
		blocks on recv()]

2. b: [enters, marks b as ready. fires rule 1. b finishes enter.
		recv() succeeds and m is filled and marked as ready b returns]

deadlock. nobody is left to check rule 1.

this problem is more serious as the deadlock may be several layers deep.
imagine rules:
1. m1 -> a
2. m2 -> m1
3. b -> m2
here the same occurs, but b seems to be responsible for <enter>.

## CURRENT SOLUTION:
when a port-getter empties a memory cell, they must again <enter> to catch any
changes.

## LATER SOLUTION:
passing of responsibility ie. instead of exiting <enter> just because
your rule isnt satisfied _right now_, you enter a new state where you become receptive
to notifications to check again. these notifications are sent whenever a memory cell is
emtied by a _getter_. (when the proto manipulates memcells its not an issue)



```rust

fn enter()

///////////

enum CaretakerState  {
	Needed,
	Waiting,
	Unneeded,
}
struct Responsibility {
	cond: Cond,
	awaiting_caretaker,
}
impl Responsibility {
	// we need a caretaker eventually
	fn defer(&mut self) {
		use CaretakerState::*;
		if cond.notify_one() {
			// someone woke up to take care of it
		} else {
			self.awaiting_caretaker = true;
		}
	}
	fn offer(&mut self) {
		if self.awaiting_caretaker {
			self.awaiting_caretaker = false;
			cond.await();
		}
	}
}
```