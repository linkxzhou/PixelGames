//! Battle City runtime.
//!
//! The simulation ([`sim`]) is deterministic, fixed-step and free of any
//! windowing or clock dependency, so it can be unit tested headless. The
//! renderer ([`draw`]) only reads a [`sim::Sim`] and the pxl asset package.

pub mod assets;
pub mod enemy;
pub mod level;
pub mod palette;
pub mod powerup;
pub mod save;
pub mod sim;

#[cfg(feature = "window")]
pub mod draw;

pub use assets::{Package, Sheet};
pub use enemy::EnemyKind;
pub use level::Level;
pub use palette::Palette;
pub use powerup::PowerKind;
pub use save::Save;
pub use sim::{Dir, Input, Outcome, Sim};
