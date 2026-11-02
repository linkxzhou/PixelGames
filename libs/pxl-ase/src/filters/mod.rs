//! Headless filters (merged `pxl-ase-filters`). Prefer outline/replace.

use pxl_ase_doc::{ColorMode, ImageBuffer, Sprite};
use pxl_core::{paint_outline, Brush, IndexFrame, OutlineMode, Rgba};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

/// Replace exact RGBA (or indexed index) across all image buffers.
pub fn replace_color_rgba(sprite: &mut Sprite, from: Rgba, to: Rgba) -> Result<usize> {
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if chunk[0] == from.r
                        && chunk[1] == from.g
                        && chunk[2] == from.b
                        && chunk[3] == from.a
                    {
                        chunk[0] = to.r;
                        chunk[1] = to.g;
                        chunk[2] = to.b;
                        chunk[3] = to.a;
                        n += 1;
                    }
                }
                buf.pixels = std::sync::Arc::new(px);
            }
            ImageBuffer::Indexed(_) | ImageBuffer::Grayscale(_) => {}
        }
    }
    Ok(n)
}

pub fn replace_index(sprite: &mut Sprite, from: u8, to: u8) -> Result<usize> {
    if sprite.color_mode != ColorMode::Indexed {
        return Err(AseErr("replace_index needs indexed mode".into()));
    }
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Indexed(buf) = img {
            let mut px = buf.pixels.as_ref().clone();
            for p in &mut px {
                if *p == from {
                    *p = to;
                    n += 1;
                }
            }
            buf.pixels = std::sync::Arc::new(px);
        }
    }
    Ok(n)
}

/// Outline on flattened indexed preview of frame 0; writes back as single flattened layer.
pub fn outline_flatten(
    sprite: &mut Sprite,
    color_index: u8,
    mode: OutlineMode,
    brush: Brush,
    width: i32,
) -> Result<()> {
    // Convert path: flatten → quantize to current/new palette → outline → replace layers.
    let rgba = flatten_frame_rgba(sprite, 0)?;
    if sprite.color_mode != ColorMode::Indexed || sprite.palette.is_empty() {
        crate::color::convert_sprite_color_mode(sprite, ColorMode::Indexed)?;
    }
    let pal = crate::sprite_palette(sprite);
    let indices = pxl_art::quantize::map_to_palette(
        sprite.width as usize,
        sprite.height as usize,
        &rgba,
        &pal,
        0,
    );
    let mut frame = IndexFrame::canvas(sprite.width, sprite.height).map_err(|e| {
        AseErr(e.message.clone())
    })?;
    frame.pixels = indices;
    paint_outline(&mut frame, color_index, mode, brush, width).map_err(|e| AseErr(e.message))?;
    sprite.layers.clear();
    sprite.images.clear();
    let layer = sprite.add_image_layer("Outlined");
    let img = pxl_ase_doc::IndexedImage::from_indices(sprite.width, sprite.height, frame.pixels);
    let id = sprite.insert_image(ImageBuffer::Indexed(img));
    sprite.set_cel(layer, 0, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    sprite.ensure_frames(1);
    sprite.frame_durations_ms.truncate(1);
    sprite.color_mode = ColorMode::Indexed;
    Ok(())
}
