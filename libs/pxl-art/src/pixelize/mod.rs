//! Raster to pixel art: recover the pixel grid of a "fake" pixel-art image,
//! or pixelate any picture to a target size, then index it.
//!
//! Stages (all integer, all deterministic):
//! 1. `detect_grid` — find the block size and phase of an upscaled or
//!    AI-generated image from the periodicity of its edge profile.
//! 2. `expand_outlines` — contrast-aware outline expansion (after PixelOE):
//!    thin dark strokes are thickened before downscaling so they survive it.
//! 3. `downscale` — one output pixel per cell: `nearest`, `mode` (most common
//!    colour), `kcentroid` (mean of the larger of two OKLab clusters, which
//!    rejects anti-aliasing fringe) or `contrast` (the extreme that differs
//!    most from the median, which keeps 1-px detail).
//! 4. palette — map to a given palette (optionally Bayer-dithered) or build
//!    one with median cut + k-means.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Oklab, Palette, PxlError, PxlResult, Rgba};

use crate::quantize::{map_to_palette, median_cut};


mod grid;
mod run;

pub use grid::{detect_grid, downscale, expand_outlines, crop_to_grid, Downscale, Rgba8, Grid};
pub use run::{pixelize, PixelizeOptions, Pixelized};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
