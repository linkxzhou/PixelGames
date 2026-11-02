//! Pxl CLI adapter for the agent loop.
mod adapter;
mod helpers;
mod repair;

pub use adapter::{PxlAdapter, PxlBuildOutput, PxlResult_};
pub use repair::{render_failed_checks, repair_class, RepairClass};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
