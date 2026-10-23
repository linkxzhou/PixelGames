//! Battle City slice.
//!
//! The simulation steps at a fixed 60 Hz. [`replay::run_replay`] drives that
//! clock from a script and writes PNGs. The `window` feature draws the same
//! indexed buffer through macroquad with nearest-neighbour filtering.

pub mod draw;
pub mod level;
pub mod palette;
pub mod pngutil;
pub mod replay;
pub mod sim;

pub use replay::{cli, run_replay, HeadlessRequest, PlayReport, Replay};
