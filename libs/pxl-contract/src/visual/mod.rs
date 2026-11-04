//! Visual intent contract types and validation.
mod types;
mod validate;

pub use types::*;

#[cfg(test)]
#[path = "tests.rs"]
pub(crate) mod tests;
