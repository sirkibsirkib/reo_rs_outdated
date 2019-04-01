
// useful functions used by the template-generated protocol structures 
#[macro_use]
pub mod protocols;

// the primitives for reo such as Ports.
mod reo;
pub use reo::*;


mod port_backend;

// unit tests
#[cfg(test)]
mod tests;
