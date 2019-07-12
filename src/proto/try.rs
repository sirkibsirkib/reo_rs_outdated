

struct PutterRendesvous {
	ptr: AtomicPtr,
	countdown: AtomicUsize,
	mover: AtomicBool,
}

struct Proto {
	putters: Vec<PutterRendesvous>,
	ready: Arc<Mutex<BitSet>>,
	m0: u32,
}

impl Proto {
	fn become_ready_coordinate(&self, id: usize) {

	}
}