//! Crate root.

pub mod adapters;
pub mod agent;
pub mod config;
pub mod contracts;
pub mod digest;
pub mod error;
pub mod pipeline;
pub mod policy;
pub mod storage;
pub mod qa;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
