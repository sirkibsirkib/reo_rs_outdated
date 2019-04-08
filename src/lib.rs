// useful functions used by the template-generated protocol structures
#[macro_use]
pub mod protocols2;

// the primitives for reo such as Ports.
// mod reo;
// pub use reo::*;

#[macro_use]
mod threadless2;
// pub use threadless2 as reo;

// unit tests
#[cfg(test)]
mod tests;
