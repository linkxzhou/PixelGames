//! ASCII sprite block format.
mod charset;
mod spec;

pub use charset::*;
pub use spec::*;

#[cfg(test)]
#[path = "rle_tests.rs"]
mod rle_tests;
