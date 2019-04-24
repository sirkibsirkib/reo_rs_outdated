
use hashbrown::{HashSet,HashMap};


type PortId = u32;
type StateId = usize;


const A: usize = 0;
const B: usize = 1;
const C: usize = 2;

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
enum Val {
	T, // True
	F, // False
	G, // generic. never appears in STATE
	U, // explicity UNKNOWN. used only in assignment
}

type Pred = [Val;3];

#[derive(Debug)]
struct Gcmd {
	input: Pred,
	involved: PortId,
	output: Pred,
}
impl Gcmd {
	fn new(input_map: HashMap<usize, Val>, involved: PortId, output_map: HashMap<usize, Val>) -> Self {
		let mut input = [Val::G; 3];
		for (&k,&v) in input_map.iter() {
			input[k] = v;
		}
		let mut output = [Val::G; 3];
		for (&k,&v) in output_map.iter() {
			output[k] = v;
		}
		println!("{:?}, {:?}", &input, &output);
		Self {input, involved, output}
	}
	fn apply(&self, state: &Pred) -> Option<Pred> {
		let mut res = state.clone();
		for i in 0..3 {
			use Val::*;
			res[i] = match [state[i], self.output[i]] {
				[T, F] |
				[F, T] => return None, // unsatisfied
				[v, G] | // carry over value from state
				[_, v] => v, //overwrite value
			};
		}
		Some(res)
	}
}

#[derive(Debug)]
struct Branch {
	port: PortId,
	dest: Pred,
}

#[macro_export]
macro_rules! set {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(set!(@single $rest)),*]));

    ($($value:expr,)+) => { set!($($value),+) };
    ($($value:expr),*) => {
        {
            let _countcap = set!(@count $($value),*);
            let mut _the_set = HashSet::with_capacity(_countcap);
            $(
                let _ = _the_set.insert($value);
            )*
            _the_set
        }
    };
}

fn print_pred_name(p: &Pred) {
	for i in 0..3 {
		print!("{:?}", p[i]);
	}
}

#[test]
fn rba_build() {
	use Val::*;
	let mut states = set!{[F,F,F]};
	let mut states_todo = vec![states.iter().next().unwrap().clone()];
	let mut opts: HashMap<Pred, Vec<Branch>> = HashMap::default();

	//input: gcmds and starting state.
	//output: map state->Vec<Branch>
	let gcmd = vec![
		Gcmd::new(map!{A=>T, C=>T}, 1, map!{B=>U}),
		Gcmd::new(map!{A=>T, C=>F}, 1, map!{B=>T}),
		Gcmd::new(map!{B=>F}, 2, map!{A=>T}),
		Gcmd::new(map!{C=>T}, 2, map!{A=>T}),
		Gcmd::new(map!{B=>T, C=>F}, 2, map!{A=>T}),
	];
	while let Some(state) = states_todo.pop() {
		println!("\n~~~~~~~~~~ processing: {:?}", state);
		let mut o = vec![];
		for (gid, g) in gcmd.iter().enumerate() {
			if let Some(new_state) = g.apply(&state) {
				println!("state {:?} can apply rule {}", &state, gid);
				if !states.contains(&new_state) {
					states_todo.push(new_state);
					states.insert(new_state);
				}
				o.push(Branch{
					port: g.involved,
					dest: new_state,
				});
			} 
		}
		opts.insert(state, o);
	}
	// println!("{:#?}", opts);

	println!("DENSE...");
	// let mut i = 0;
	// let names: HashMap<_,usize> = states.iter().map(|s| {i += 1; (s, i-1)}).collect();
	for (src, opts) in opts.iter() {
		print_pred_name(src);
		print!(" {{");
		for b in opts.iter() {
			print!("={}=> ", b.port);
			print_pred_name(&b.dest);
			print!(", ");
		}
		print!("}}\n");
	}
}