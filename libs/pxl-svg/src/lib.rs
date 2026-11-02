//! SVG subset to palette-indexed pixel frames.
//!
//! The rasteriser never blends: geometry is filled on an `N x N` sample grid
//! with palette indices, and one output pixel is the most common index of its
//! samples (see [`raster`]). That keeps edges crisp and the palette closed,
//! which is what an SVG written by a model needs before it looks like pixel
//! art.
//!
//! Supported subset: `svg`, `g`, `defs`, `symbol`, `use`, `rect`, `circle`,
//! `ellipse`, `line`, `polyline`, `polygon`, `path`, `linearGradient`,
//! `radialGradient`, `clipPath`; `transform`, `fill`, `stroke`,
//! `stroke-width`, `stroke-linejoin`, `stroke-linecap`, `stroke-miterlimit`,
//! `fill-rule`, `display`, `visibility`, inline `style`.
//!
//! Model-facing hints, all optional:
//!
//! | attribute | effect |
//! |---|---|
//! | `data-frame="idle_0"` | the group is one frame; no frames means `base` |
//! | `data-role="outline"` | the outline is redrawn by the finishing pass |
//! | `data-role="highlight\|shadow\|eye\|detail"` | the detail gets at least one pixel |
//! | `data-material="metal\|cloth\|..."` | per-pixel material for cleanup/shade |
//! | `data-ramp="skin" data-step="-1"` | take the colour from a palette ramp |
//! | `data-min="2"` | keep at least N pixels whatever the size |
//! | `data-sizes="64,128,256"` | draw only at these tiers |

extern crate alloc;

use alloc::format;
use alloc::string::String;

mod geom;
mod num;
mod palette_map;
mod raster;
mod scene;
mod xml;

pub use geom::{parse_transform, Affine, Cap, Join, Poly};
pub use palette_map::{ColorMode, ColorReport};
pub use scene::{SvgOptions, SvgOut, SvgReport};

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};

/// Rasterise `text` at `opts.w x opts.h`.
pub fn rasterize(text: &str, opts: &SvgOptions, palette: &Palette) -> PxlResult<SvgOut> {
    if opts.w == 0 || opts.h == 0 || opts.w > pxl_core::MAX_DIM || opts.h > pxl_core::MAX_DIM {
        return Err(PxlError::schema(alloc_msg(opts)));
    }
    let root = xml::parse(text)?;
    if root.name != "svg" && !root.name.ends_with(":svg") {
        return Err(PxlError::schema(format!(
            "svg.root: the root element is <{}>, not <svg>",
            root.name
        ))
        .with_hint("pass a file whose top-level element is <svg>"));
    }
    scene::build(&root, opts, palette)
}

fn alloc_msg(opts: &SvgOptions) -> String {
    format!(
        "svg.bad_size: target {}x{} is outside 1..={}",
        opts.w,
        opts.h,
        pxl_core::MAX_DIM
    )
}

/// One frame, no finishing: the smallest useful call.
pub fn rasterize_one(text: &str, size: u16, palette: &Palette) -> PxlResult<(IndexFrame, Palette)> {
    let opts = SvgOptions {
        w: size,
        h: size,
        ..Default::default()
    };
    let out = rasterize(text, &opts, palette)?;
    let frame = out
        .frames
        .into_iter()
        .next()
        .map(|(_, f)| f.frame)
        .ok_or_else(|| PxlError::semantic("svg.no_frame: the document drew nothing"))?;
    Ok((frame, out.palette))
}
