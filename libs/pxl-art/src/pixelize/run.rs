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


use super::grid::{crop_to_grid, detect_grid, downscale, expand_outlines, Downscale, Grid, Rgba8};

#[derive(Clone, Debug)]
pub struct PixelizeOptions {
    /// Target size. `None` recovers the native grid with `detect_grid`.
    pub size: Option<(u16, u16)>,
    pub method: Downscale,
    /// Outline expansion radius in source pixels; `None` = a quarter cell.
    pub outline_radius: Option<usize>,
    /// Palette to map onto; `None` builds `colors` with median cut.
    pub palette: Option<Palette>,
    pub colors: usize,
    /// 0..=100 Bayer strength when mapping.
    pub dither: u8,
}

impl Default for PixelizeOptions {
    fn default() -> Self {
        Self {
            size: None,
            method: Downscale::KCentroid,
            outline_radius: None,
            palette: None,
            colors: 16,
            dither: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pixelized {
    pub frame: IndexFrame,
    pub palette: Palette,
    pub grid: Grid,
    pub outline_radius: usize,
}

pub fn pixelize(img: &Rgba8, opts: &PixelizeOptions) -> PxlResult<Pixelized> {
    if img.w == 0 || img.h == 0 || img.data.len() != img.w * img.h * 4 {
        return Err(PxlError::schema(
            "pixelize: the image buffer does not match its size",
        ));
    }
    let (src, grid, w, h) = match opts.size {
        Some((w, h)) => {
            let cell = (img.w / w.max(1) as usize)
                .min(img.h / h.max(1) as usize)
                .max(1);
            (
                img.clone(),
                Grid {
                    cell,
                    phase_x: 0,
                    phase_y: 0,
                    score: 0,
                },
                w as usize,
                h as usize,
            )
        }
        None => {
            let grid = detect_grid(img, 64);
            let (cropped, cols, rows) = crop_to_grid(img, grid);
            (cropped, grid, cols, rows)
        }
    };
    if w == 0 || h == 0 || w > pxl_core::MAX_DIM as usize || h > pxl_core::MAX_DIM as usize {
        return Err(PxlError::semantic(alloc::format!(
            "pixelize.bad_size: the result would be {w}x{h}"
        ))
        .with_hint("pass a target size of at most 256x256"));
    }
    let radius = opts.outline_radius.unwrap_or(grid.cell / 4);
    let expanded = if grid.cell >= 3 {
        expand_outlines(&src, radius, 48)
    } else {
        src
    };
    let small = downscale(&expanded, w, h, opts.method);
    let palette = match &opts.palette {
        Some(p) => p.clone(),
        None => {
            let colors = median_cut(&small.data, opts.colors.clamp(1, 61), 6);
            let mut p = Palette {
                id: "pixelize".into(),
                colors,
                ramps: Default::default(),
                cycles: Default::default(),
                special: pxl_core::Special::default(),
            };
            let darkest = (1..p.colors.len())
                .min_by_key(|i| Oklab::from_rgba(p.colors[*i]).l)
                .unwrap_or(0) as u8;
            p.special.outline = darkest;
            p.special.flash = darkest;
            p
        }
    };
    let pixels = map_to_palette(w, h, &small.data, &palette, opts.dither);
    let mut frame = IndexFrame::new(w as u16, h as u16)?;
    frame.pixels = pixels;
    Ok(Pixelized {
        frame,
        palette,
        grid,
        outline_radius: radius,
    })
}
