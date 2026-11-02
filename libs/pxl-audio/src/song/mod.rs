//! BeepBox song validation.
mod consts;
mod validate;

pub use consts::{SongFacts, BEEPBOX_FORMAT, BEEPBOX_VERSION};
pub use validate::validate_song;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
