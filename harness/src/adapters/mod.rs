//! Adapters: controlled process execution and the typed pxl CLI wrapper.

pub mod process;
pub mod pxl;

pub use pxl::{repair_class, PxlAdapter, PxlBuildOutput, PxlResult_, RepairClass};
