//! Deterministic pixel-art primitives.
//!
//! This crate does not touch the filesystem, the system clock, threads, or
//! platform RNGs. It is `no_std` + `alloc` so a future WASM build can reuse it.

#![no_std]

extern crate alloc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod color;
pub mod error;
pub mod frame;
pub mod outline;
pub mod palette;
pub mod prng;
pub mod text;

pub use color::{parse_hex, Rgba};
pub use error::{ErrorCode, PxlError, PxlResult};
pub use frame::{FrameStats, IndexFrame, Rect};
pub use outline::{paint_outline, Brush, OutlineMode};
pub use palette::{colors_from_hex, fraction_to_bp, fraction_to_q8, Cycle, Palette, Special};
pub use prng::{op_seed, sha256_hex, Pcg32, SplitMix64};
pub use text::{decode_index, decode_matrix, encode_index, encode_matrix};

pub const PXL_VERSION: &str = "0.1.0";
pub const GEN_VERSION: u32 = 1;
/// Hard cap from the pipeline spec. M1 sprites are 32×32 or smaller.
pub const MAX_DIM: u16 = 64;
pub const MAX_ANIM_FRAMES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Mirror {
    None,
    X,
    Y,
    Xy,
}

impl Mirror {
    pub fn parse(s: &str) -> PxlResult<Self> {
        match s {
            "none" | "" | "off" => Ok(Mirror::None),
            "x" | "h" | "horizontal" => Ok(Mirror::X),
            "y" | "v" | "vertical" => Ok(Mirror::Y),
            "xy" | "both" => Ok(Mirror::Xy),
            other => Err(PxlError::schema(alloc::format!("unknown mirror '{other}'"))
                .with_hint("mirror is none, x, y, or xy")),
        }
    }
}
