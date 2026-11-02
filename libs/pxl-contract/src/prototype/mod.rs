//! Prototype / scene-kit / catalog-lock contracts.
mod digest;
mod types;
mod validate;

pub use digest::{sha256_digest, valid_digest, valid_id};
pub use types::*;

#[cfg(test)]
#[path = "kit_tests.rs"]
mod kit_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
