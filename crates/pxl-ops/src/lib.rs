//! Frame transforms and animation macros.
//!
//! Flip, rotate90, and shift follow Piskel `TransformUtils` semantics
//! (Apache-2.0, reimplemented). Outline brushes follow Pixelorama's MIT
//! outline modes via [`pxl_core::paint_outline`].

mod apply;
pub mod macros;
mod parts;

pub use apply::{apply_op, apply_ops, ApplyCx};
pub use macros::{
    expand_macro, macro_catalog, macro_info, parse_macro_call, MacroArgInfo, MacroInfo, MACRO_NAMES,
};
pub use parts::{compose_rig, PartPose};
