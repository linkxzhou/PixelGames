//! Integer vector contour reconstruction.
mod geom;
mod trace;

pub use trace::reconstruct;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
