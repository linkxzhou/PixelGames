use pxl_core::{IndexFrame, Palette, PxlResult};

use crate::png::encode_rgba_png;
use crate::raster::composite;

/// Preview composite. Transparent pixels become a flat dark backdrop so the
/// PNG is readable, and `--grid` draws a 1px lattice on the scaled cells.
pub fn render_preview_png(
    frame: &IndexFrame,
    palette: &Palette,
    scale: u32,
    grid: bool,
) -> PxlResult<Vec<u8>> {
    let (w, h, rgba) = composite(frame, palette, scale.max(1), grid);
    encode_rgba_png(w, h, &rgba)
}
