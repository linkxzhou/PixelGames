//! Deterministic pixel-art reconstruction on index frames.
//!
//! The enhance chain is analyse → upscale → clean → outline → shade → measure;
//! `finish` runs the same passes on a native frame, `pixelize` turns any
//! raster into an indexed frame, `rotsprite` rotates without new colours. Every
//! stage is integer-only and returns an [`IndexFrame`], so an output pixel is
//! always one of the palette's colours: nothing here interpolates or blurs.
//!
//! `no_std` + `alloc` so the wasm build can reuse the same code path.

#![no_std]

extern crate alloc;

pub mod analyze;
pub mod cleanup;
pub mod contour;
pub mod finish;
pub mod metrics;
pub mod mmpx;
pub mod outline;
pub mod palette;
pub mod pipeline;
pub mod pixelize;
pub mod quantize;
pub mod rotsprite;
pub mod shade;
pub mod upscale;

pub use analyze::{Material, SpriteGraph};
pub use finish::{finish, FinishOptions, FinishReport, Finished};
pub use metrics::Metrics;
pub use pipeline::{
    enhance, enhance_to, CleanOptions, EnhanceOptions, EnhanceReport, Enhanced, Light, OutlineMode,
    ProtectReport, ShadeMode, Upscale,
};
pub use pixelize::{detect_grid, pixelize, Downscale, Grid, PixelizeOptions, Pixelized, Rgba8};
pub use rotsprite::rotsprite;
