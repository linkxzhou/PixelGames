//! Headless filters (merged `pxl-ase-filters`).


use pxl_ase_doc::{ColorMode, ImageBuffer, Sprite};
use pxl_core::{paint_outline, Brush, IndexFrame, OutlineMode, Rgba};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

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


/// Hue/Saturation/Value adjust on RGBA (and grayscale value-only) buffers.
/// `hue_deg` is degrees added to hue; `sat`/`value` are relative -100..=100.

pub fn flood_fill_frame(
    sprite: &mut Sprite,
    frame: u16,
    x: i32,
    y: i32,
    fill: Rgba,
    tolerance: u8,
) -> Result<usize> {
    let w = sprite.width as i32;
    let h = sprite.height as i32;
    if x < 0 || y < 0 || x >= w || y >= h {
        return Err(AseErr(format!("flood-fill seed ({x},{y}) out of bounds {w}x{h}")));
    }
    let mut rgba = flatten_frame_rgba(sprite, frame)?;
    let idx = |px: i32, py: i32| -> usize { ((py * w + px) * 4) as usize };
    let get = |buf: &[u8], px: i32, py: i32| -> Rgba {
        let i = idx(px, py);
        Rgba {
            r: buf[i],
            g: buf[i + 1],
            b: buf[i + 2],
            a: buf[i + 3],
        }
    };
    let target = get(&rgba, x, y);
    let tol = tolerance as i16;
    let near = |a: Rgba, b: Rgba| {
        (a.r as i16 - b.r as i16).abs() <= tol
            && (a.g as i16 - b.g as i16).abs() <= tol
            && (a.b as i16 - b.b as i16).abs() <= tol
            && (a.a as i16 - b.a as i16).abs() <= tol
    };
    if near(target, fill) {
        return Ok(0);
    }
    let mut visited = vec![false; (w * h) as usize];
    let mut stack = vec![(x, y)];
    let mut n = 0usize;
    while let Some((cx, cy)) = stack.pop() {
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            continue;
        }
        let vi = (cy * w + cx) as usize;
        if visited[vi] {
            continue;
        }
        visited[vi] = true;
        if !near(get(&rgba, cx, cy), target) {
            continue;
        }
        let i = idx(cx, cy);
        rgba[i] = fill.r;
        rgba[i + 1] = fill.g;
        rgba[i + 2] = fill.b;
        rgba[i + 3] = fill.a;
        n += 1;
        stack.push((cx + 1, cy));
        stack.push((cx - 1, cy));
        stack.push((cx, cy + 1));
        stack.push((cx, cy - 1));
    }
    // Rebuild document as RGBA timeline preserving other frames.
    let frames = sprite.frame_count();
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        if f as u16 == frame {
            flats.push(rgba.clone());
        } else {
            flats.push(flatten_frame_rgba(sprite, f as u16)?);
        }
    }
    sprite.layers.clear();
    sprite.images.clear();
    let layer = sprite.add_image_layer("FloodFill");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = pxl_ase_doc::RgbaImage::from_rgba(sprite.width, sprite.height, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Rgba;
    Ok(n)
}
