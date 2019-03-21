// general Reo primitives for the runtime
mod reo;

// useful for the compiler to generate protocol components
#[macro_use]
mod protocols;

mod port;

// testing module
#[cfg(test)]
mod tests;
