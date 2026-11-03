//! Deterministic, subject-independent QA for indexed pixels.
//! Integrity failures block validation. Style heuristics produce localized
//! warnings by default; explicit configuration or strict mode controls gating.

mod animation;
mod common;
mod composition;
mod config;
mod model;
mod pixels;
mod resolution;
mod tile;
mod validate;

pub use composition::compose_grid;
pub use config::QaConfig;
pub use model::{AnimSeq, Check, PixelLocation, Report, Severity, ValidateInput};
pub use validate::{validate, validate_with_config};

#[cfg(test)]
mod tests;
