//! Shared runtime for the PixelGames arcade remakes.
//!
//! The crate is split in two halves:
//!
//! * `package`, `palette`, `rng`, `input`, `screen`, `state` and `replay` are
//!   macroquad free. Simulation code and tests use them under `cargo test`.
//! * `gfx`, `hud`, `shot` and `audio` only exist behind the `window` feature and
//!   are the only modules that touch the GPU.
//!
//! Every pixel still comes from the pxl `400-package`; this crate only decodes
//! the sheets, slices the frames and draws them.

pub mod input;
pub mod package;
pub mod palette;
pub mod path;
pub mod replay;
pub mod rng;
pub mod screen;
pub mod state;

#[cfg(feature = "window")]
pub mod audio;
#[cfg(feature = "window")]
pub mod gfx;
#[cfg(feature = "window")]
pub mod hud;
#[cfg(feature = "window")]
pub mod run;
#[cfg(feature = "window")]
pub mod shot;

pub use input::{Dir4, InputState};
pub use package::{Asset, Frame, Package, Sheet};
pub use screen::Viewport;
pub use state::ScreenState;
