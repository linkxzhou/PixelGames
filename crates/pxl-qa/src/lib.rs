//! Programmatic checks. `default` covers a single sprite document.
//! `tank` covers the 4-direction demo: bbox, tread motion, hull stability, tiles.
//! `character` covers a paper-doll: facings, mirror, walk/idle diffs, stable feet.

mod animation;
mod character;
mod common;
mod model;
mod tank;
mod tile;
mod validate;

pub use model::{AnimSeq, Check, Report, ValidateInput};
pub use validate::validate;

#[cfg(test)]
mod tests;
