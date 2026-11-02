//! Headless filters (merged `pxl-ase-filters`).

use std::sync::Arc;

use pxl_ase_doc::{ColorMode, ImageBuffer, Sprite};
use pxl_core::{paint_outline, Brush, IndexFrame, OutlineMode, Rgba};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

/// Replace exact RGBA across all RGBA image buffers.
pub fn replace_color_rgba(sprite: &mut Sprite, from: Rgba, to: Rgba) -> Result<usize> {
    replace_color_rgba_tol(sprite, from, to, 0)
}

/// Replace RGBA within Chebyshev distance `tolerance` on each channel.
pub fn replace_color_rgba_tol(
    sprite: &mut Sprite,
    from: Rgba,
    to: Rgba,
    tolerance: u8,
) -> Result<usize> {
    let tol = tolerance as i16;
    let near = |a: u8, b: u8| (a as i16 - b as i16).abs() <= tol;
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if near(chunk[0], from.r)
                        && near(chunk[1], from.g)
                        && near(chunk[2], from.b)
                        && near(chunk[3], from.a)
                    {
                        chunk[0] = to.r;
                        chunk[1] = to.g;
                        chunk[2] = to.b;
                        chunk[3] = to.a;
                        n += 1;
                    }
                }
                buf.pixels = Arc::new(px);
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
            buf.pixels = Arc::new(px);
        }
    }
    Ok(n)
}

/// Invert RGB (keep alpha) on RGBA buffers; invert indices around mid for indexed.
pub fn invert(sprite: &mut Sprite) -> Result<usize> {
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    chunk[0] = 255 - chunk[0];
                    chunk[1] = 255 - chunk[1];
                    chunk[2] = 255 - chunk[2];
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Grayscale(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(2) {
                    chunk[0] = 255 - chunk[0];
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(buf) => {
                let max = sprite.palette.len().saturating_sub(1).max(1) as u8;
                let mut px = buf.pixels.as_ref().clone();
                for p in &mut px {
                    if *p != sprite.transparent_index {
                        *p = max.saturating_sub(*p);
                        n += 1;
                    }
                }
                buf.pixels = Arc::new(px);
            }
        }
    }
    Ok(n)
}

/// Brightness/contrast on RGBA (and grayscale) buffers.
/// `brightness` in -255..=255, `contrast` in -100..=100 (percent-ish).
pub fn brightness_contrast(sprite: &mut Sprite, brightness: i32, contrast: i32) -> Result<usize> {
    let b = brightness.clamp(-255, 255);
    let c = contrast.clamp(-100, 100);
    // contrast factor: map -100..100 → 0..2 around 1.0
    let factor = (100 + c) as f32 / 100.0;
    let adj = |v: u8| -> u8 {
        let mut x = v as f32;
        x = (x - 128.0) * factor + 128.0 + b as f32;
        x.round().clamp(0.0, 255.0) as u8
    };
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if chunk[3] == 0 {
                        continue;
                    }
                    chunk[0] = adj(chunk[0]);
                    chunk[1] = adj(chunk[1]);
                    chunk[2] = adj(chunk[2]);
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Grayscale(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(2) {
                    if chunk[1] == 0 {
                        continue;
                    }
                    chunk[0] = adj(chunk[0]);
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(_) => {}
        }
    }
    // For indexed-only docs, flatten→RGBA adjust→write back as RGBA layer.
    if n == 0 && sprite.color_mode == ColorMode::Indexed {
        let frames = sprite.frame_count();
        let mut flats = Vec::with_capacity(frames);
        for f in 0..frames {
            flats.push(flatten_frame_rgba(sprite, f as u16)?);
        }
        sprite.layers.clear();
        sprite.images.clear();
        let layer = sprite.add_image_layer("Adjusted");
        for (fi, mut rgba) in flats.into_iter().enumerate() {
            for chunk in rgba.chunks_exact_mut(4) {
                if chunk[3] == 0 {
                    continue;
                }
                chunk[0] = adj(chunk[0]);
                chunk[1] = adj(chunk[1]);
                chunk[2] = adj(chunk[2]);
                n += 1;
            }
            let img = pxl_ase_doc::RgbaImage::from_rgba(sprite.width, sprite.height, rgba);
            let id = sprite.insert_image(ImageBuffer::Rgba(img));
            sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
        }
        sprite.color_mode = ColorMode::Rgba;
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

/// Outline every frame (keeps timeline); flattens layers.
pub fn outline_all_frames(
    sprite: &mut Sprite,
    color_index: u8,
    mode: OutlineMode,
    brush: Brush,
    width: i32,
) -> Result<()> {
    if sprite.color_mode != ColorMode::Indexed || sprite.palette.is_empty() {
        crate::color::convert_sprite_color_mode(sprite, ColorMode::Indexed)?;
    }
    let pal = crate::sprite_palette(sprite);
    let frames = sprite.frame_count();
    let mut outlined = Vec::with_capacity(frames);
    for f in 0..frames {
        let rgba = flatten_frame_rgba(sprite, f as u16)?;
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
        outlined.push(frame.pixels);
    }
    sprite.layers.clear();
    sprite.images.clear();
    let layer = sprite.add_image_layer("Outlined");
    for (fi, px) in outlined.into_iter().enumerate() {
        let img = pxl_ase_doc::IndexedImage::from_indices(sprite.width, sprite.height, px);
        let id = sprite.insert_image(ImageBuffer::Indexed(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Indexed;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, RgbaImage, Sprite};

    fn rgba_sprite() -> Sprite {
        let mut s = Sprite::new(2, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(2, 1, vec![10, 20, 30, 255, 200, 210, 220, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s
    }

    #[test]
    fn invert_and_replace_tol() {
        let mut s = rgba_sprite();
        invert(&mut s).unwrap();
        replace_color_rgba_tol(
            &mut s,
            Rgba {
                r: 245,
                g: 235,
                b: 225,
                a: 255,
            },
            Rgba::rgb(1, 2, 3),
            5,
        )
        .unwrap();
        brightness_contrast(&mut s, 10, 0).unwrap();
    }
}
