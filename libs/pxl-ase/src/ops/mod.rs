//! Thin bridge to `pxl-ops` for indexed frames extracted from a sprite.

use std::cell::RefCell;
use std::collections::BTreeMap;

use pxl_ase_doc::{ColorMode, ImageBuffer, Sprite};
use pxl_core::IndexFrame;
use pxl_format::Op;
use pxl_ops::{apply_op, ApplyCx};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

/// Apply one `pxl-format::Op` to a flattened indexed view of `frame`, then rebuild layers.
pub fn apply_op_to_frame(sprite: &mut Sprite, frame: u16, op: &Op, seed: u64) -> Result<()> {
    apply_op_to_frames(sprite, &[frame], op, seed)
}

/// Apply one op to every listed frame (e.g. flip/rotate across the timeline).
pub fn apply_op_to_frames(sprite: &mut Sprite, frames: &[u16], op: &Op, seed: u64) -> Result<()> {
    if sprite.color_mode != ColorMode::Indexed {
        crate::color::convert_sprite_color_mode(sprite, ColorMode::Indexed)?;
    }
    let pal = crate::sprite_palette(sprite);
    let target: std::collections::BTreeSet<u16> = frames.iter().copied().collect();
    let total = sprite.frame_count();
    let mut all = Vec::with_capacity(total);
    for f in 0..total {
        let rgba = flatten_frame_rgba(sprite, f as u16)?;
        let indices = pxl_art::quantize::map_to_palette(
            sprite.width as usize,
            sprite.height as usize,
            &rgba,
            &pal,
            0,
        );
        let mut index =
            IndexFrame::canvas(sprite.width, sprite.height).map_err(|e| AseErr(e.message))?;
        index.pixels = indices;
        if target.contains(&(f as u16)) {
            let cx = ApplyCx {
                palette: &pal,
                seed: seed.wrapping_add(f as u64),
                frame_name: "ase",
                load_template: None,
                parts: None,
                pose: RefCell::new(BTreeMap::new()),
            };
            index = apply_op(&index, op, &cx, 0).map_err(|e| AseErr(e.message))?;
        }
        all.push(index.pixels);
    }
    sprite.layers.clear();
    sprite.images.clear();
    let layer = sprite.add_image_layer("Ops");
    for (fi, px) in all.into_iter().enumerate() {
        let img = pxl_ase_doc::IndexedImage::from_indices(sprite.width, sprite.height, px);
        let id = sprite.insert_image(ImageBuffer::Indexed(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Indexed;
    Ok(())
}

/// Convenience: flip every frame on `axis` (`h`/`v`/`horizontal`/`vertical`).
pub fn flip_all_frames(sprite: &mut Sprite, axis: &str) -> Result<()> {
    let axis = match axis.to_ascii_lowercase().as_str() {
        "h" | "x" | "horizontal" => "h".to_string(),
        "v" | "y" | "vertical" => "v".to_string(),
        other => {
            return Err(AseErr(format!(
                "unknown flip axis '{other}' (h|v|horizontal|vertical)"
            )))
        }
    };
    let frames: Vec<u16> = (0..sprite.frame_count() as u16).collect();
    apply_op_to_frames(sprite, &frames, &Op::Flip { axis }, 0)
}

/// Convenience: rotate every frame 90° (`cw`/`ccw`).
pub fn rotate90_all_frames(sprite: &mut Sprite, dir: &str) -> Result<()> {
    let dir = match dir.to_ascii_lowercase().as_str() {
        "cw" | "clockwise" | "right" => "cw".to_string(),
        "ccw" | "counterclockwise" | "left" => "ccw".to_string(),
        other => {
            return Err(AseErr(format!(
                "unknown rotate dir '{other}' (cw|ccw)"
            )))
        }
    };
    let frames: Vec<u16> = (0..sprite.frame_count() as u16).collect();
    apply_op_to_frames(sprite, &frames, &Op::Rotate90 { dir }, 0)
}


/// Crop the sprite canvas to `[x,y,w,h)` (document space). Updates slices.
/// Flattens layers into one RGBA layer per frame.
pub fn crop_sprite(sprite: &mut Sprite, x: i32, y: i32, w: u16, h: u16) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(AseErr("crop size must be non-zero".into()));
    }
    let ow = sprite.width as i32;
    let oh = sprite.height as i32;
    let frames = sprite.frame_count();
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        let src = flatten_frame_rgba(sprite, f as u16)?;
        let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let sx = x + dx;
                let sy = y + dy;
                if sx < 0 || sy < 0 || sx >= ow || sy >= oh {
                    continue;
                }
                let si = ((sy * ow + sx) * 4) as usize;
                let di = ((dy * w as i32 + dx) * 4) as usize;
                out[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
        flats.push(out);
    }
    for slice in &mut sprite.slices {
        for key in &mut slice.keys {
            key.x -= x;
            key.y -= y;
            if let Some((px, py)) = key.pivot.as_mut() {
                *px -= x;
                *py -= y;
            }
            if let Some((cx, cy, _cw, _ch)) = key.center.as_mut() {
                *cx -= x;
                *cy -= y;
            }
        }
    }
    sprite.layers.clear();
    sprite.images.clear();
    sprite.width = w;
    sprite.height = h;
    let layer = sprite.add_image_layer("Cropped");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = pxl_ase_doc::RgbaImage::from_rgba(w, h, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Rgba;
    Ok(())
}

/// Resize canvas to `new_w`×`new_h`, placing old content at `(ox, oy)`.
/// Content outside the new canvas is clipped. Updates slices.
pub fn canvas_size(
    sprite: &mut Sprite,
    new_w: u16,
    new_h: u16,
    ox: i32,
    oy: i32,
) -> Result<()> {
    if new_w == 0 || new_h == 0 {
        return Err(AseErr("canvas size must be non-zero".into()));
    }
    let ow = sprite.width as i32;
    let oh = sprite.height as i32;
    let frames = sprite.frame_count();
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        let src = flatten_frame_rgba(sprite, f as u16)?;
        let mut out = vec![0u8; (new_w as usize) * (new_h as usize) * 4];
        for sy in 0..oh {
            for sx in 0..ow {
                let dx = sx + ox;
                let dy = sy + oy;
                if dx < 0 || dy < 0 || dx >= new_w as i32 || dy >= new_h as i32 {
                    continue;
                }
                let si = ((sy * ow + sx) * 4) as usize;
                let di = ((dy * new_w as i32 + dx) * 4) as usize;
                out[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
        flats.push(out);
    }
    for slice in &mut sprite.slices {
        for key in &mut slice.keys {
            key.x += ox;
            key.y += oy;
            if let Some((px, py)) = key.pivot.as_mut() {
                *px += ox;
                *py += oy;
            }
        }
    }
    sprite.layers.clear();
    sprite.images.clear();
    sprite.width = new_w;
    sprite.height = new_h;
    let layer = sprite.add_image_layer("Canvas");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = pxl_ase_doc::RgbaImage::from_rgba(new_w, new_h, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Rgba;
    Ok(())
}

/// Trim transparent borders across all frames (union bbox), then crop.
pub fn trim_sprite(sprite: &mut Sprite) -> Result<Option<(i32, i32, u16, u16)>> {
    let ow = sprite.width as i32;
    let oh = sprite.height as i32;
    let mut min_x = ow;
    let mut min_y = oh;
    let mut max_x = -1;
    let mut max_y = -1;
    for f in 0..sprite.frame_count() {
        let rgba = flatten_frame_rgba(sprite, f as u16)?;
        for y in 0..oh {
            for x in 0..ow {
                let i = ((y * ow + x) * 4) as usize;
                if rgba[i + 3] >= 8 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
    }
    if max_x < min_x {
        return Ok(None);
    }
    let w = (max_x - min_x + 1) as u16;
    let h = (max_y - min_y + 1) as u16;
    crop_sprite(sprite, min_x, min_y, w, h)?;
    Ok(Some((min_x, min_y, w, h)))
}


/// Shrink sprite to fit inside `max_w`×`max_h` (never enlarges). Nearest-neighbor.
/// Rebuilds as a single RGBA layer (same pattern as [`crop_sprite`]).
pub fn shrink_sprite_to(sprite: &mut Sprite, max_w: u16, max_h: u16) -> Result<(u16, u16)> {
    if max_w == 0 || max_h == 0 {
        return Err(AseErr("--shrink-to size must be non-zero".into()));
    }
    let (tw, th) = pxl_export::fit_dims(
        sprite.width as u32,
        sprite.height as u32,
        max_w as u32,
        max_h as u32,
    );
    let (tw, th) = (tw as u16, th as u16);
    if tw == sprite.width && th == sprite.height {
        return Ok((tw, th));
    }
    let ow = sprite.width;
    let oh = sprite.height;
    let frames = sprite.frame_count();
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        let src = flatten_frame_rgba(sprite, f as u16)?;
        let out = pxl_export::resize_rgba(ow as u32, oh as u32, &src, tw as u32, th as u32)
            .map_err(|e| AseErr(e.message.clone()))?;
        flats.push(out);
    }
    // Scale slice rects proportionally (integer).
    for slice in &mut sprite.slices {
        for key in &mut slice.keys {
            key.x = ((key.x as i64) * tw as i64 / ow as i64) as i32;
            key.y = ((key.y as i64) * th as i64 / oh as i64) as i32;
            key.w = (key.w * tw as u32 / ow as u32).max(1);
            key.h = (key.h * th as u32 / oh as u32).max(1);
            if let Some((px, py)) = key.pivot.as_mut() {
                *px = ((*px as i64) * tw as i64 / ow as i64) as i32;
                *py = ((*py as i64) * th as i64 / oh as i64) as i32;
            }
            if let Some((cx, cy, cw, ch)) = key.center.as_mut() {
                *cx = ((*cx as i64) * tw as i64 / ow as i64) as i32;
                *cy = ((*cy as i64) * th as i64 / oh as i64) as i32;
                *cw = (*cw * tw as u32 / ow as u32).max(1);
                *ch = (*ch * th as u32 / oh as u32).max(1);
            }
        }
    }
    sprite.layers.clear();
    sprite.images.clear();
    sprite.width = tw;
    sprite.height = th;
    let layer = sprite.add_image_layer("Shrink");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = pxl_ase_doc::RgbaImage::from_rgba(tw, th, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
    }
    sprite.color_mode = ColorMode::Rgba;
    Ok((tw, th))
}

/// Snap `v` down to the nearest grid line at or below `v`.
fn grid_align_down(v: i32, origin: i32, cell: i32) -> i32 {
    if cell <= 0 {
        return v;
    }
    let rel = v - origin;
    let q = if rel >= 0 {
        rel / cell
    } else {
        // floor division for negatives
        -(((-rel) + cell - 1) / cell)
    };
    origin + q * cell
}

/// Snap exclusive end `v` up to the nearest grid line at or above `v`.
fn grid_align_up(v: i32, origin: i32, cell: i32) -> i32 {
    if cell <= 0 {
        return v;
    }
    let rel = v - origin;
    let q = if rel >= 0 {
        (rel + cell - 1) / cell
    } else {
        -((-rel) / cell)
    };
    origin + q * cell
}

/// Trim transparent borders, then expand the crop rect to the sprite [`Grid`].
///
/// Uses content alpha (>=8) union bbox across frames, then snaps edges outward
/// onto `sprite.grid` lines (`x`/`y` origin, `w`/`h` cell). If `grid.w` or
/// `grid.h` is 0, falls back to plain [`trim_sprite`].
pub fn trim_sprite_by_grid(sprite: &mut Sprite) -> Result<Option<(i32, i32, u16, u16)>> {
    let ow = sprite.width as i32;
    let oh = sprite.height as i32;
    let mut min_x = ow;
    let mut min_y = oh;
    let mut max_x = -1i32;
    let mut max_y = -1i32;
    for f in 0..sprite.frame_count() {
        let rgba = flatten_frame_rgba(sprite, f as u16)?;
        for y in 0..oh {
            for x in 0..ow {
                let i = ((y * ow + x) * 4) as usize;
                if rgba[i + 3] >= 8 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
    }
    if max_x < min_x {
        return Ok(None);
    }
    let gw = sprite.grid.w as i32;
    let gh = sprite.grid.h as i32;
    let gx = sprite.grid.x as i32;
    let gy = sprite.grid.y as i32;
    if gw > 0 && gh > 0 {
        min_x = grid_align_down(min_x, gx, gw).max(0);
        min_y = grid_align_down(min_y, gy, gh).max(0);
        // exclusive end after inclusive max
        let end_x = grid_align_up(max_x + 1, gx, gw).min(ow);
        let end_y = grid_align_up(max_y + 1, gy, gh).min(oh);
        max_x = end_x - 1;
        max_y = end_y - 1;
    }
    if max_x < min_x || max_y < min_y {
        return Ok(None);
    }
    let w = (max_x - min_x + 1) as u16;
    let h = (max_y - min_y + 1) as u16;
    crop_sprite(sprite, min_x, min_y, w, h)?;
    Ok(Some((min_x, min_y, w, h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, RgbaImage, Sprite};
    use pxl_core::Rgba;

    #[test]
    fn flip_all_frames_runs() {
        let mut s = Sprite::new(2, 2, ColorMode::Indexed);
        s.palette = vec![Rgba::TRANSPARENT, Rgba::rgb(255, 0, 0), Rgba::rgb(0, 0, 255)];
        s.frame_durations_ms = vec![100, 100];
        let layer = s.add_image_layer("a");
        let img = IndexedImage::from_indices(2, 2, vec![0, 1, 1, 2]);
        let id = s.insert_image(ImageBuffer::Indexed(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s.set_cel(layer, 1, Cel::linked(0));
        flip_all_frames(&mut s, "h").unwrap();
        assert_eq!(s.frame_count(), 2);
    }

    #[test]
    fn crop_and_canvas_size() {
        let mut s = Sprite::new(4, 4, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let mut px = vec![0u8; 4 * 4 * 4];
        // opaque pixel at (2,2)
        let i = (2 * 4 + 2) * 4;
        px[i] = 9;
        px[i + 1] = 8;
        px[i + 2] = 7;
        px[i + 3] = 255;
        let img = RgbaImage::from_rgba(4, 4, px);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        crop_sprite(&mut s, 1, 1, 2, 2).unwrap();
        assert_eq!((s.width, s.height), (2, 2));
        let flat = flatten_frame_rgba(&s, 0).unwrap();
        assert_eq!(&flat[12..16], &[9, 8, 7, 255]); // local (1,1) after crop from (2,2)
        canvas_size(&mut s, 4, 4, 1, 1).unwrap();
        assert_eq!((s.width, s.height), (4, 4));
    }
    #[test]
    fn trim_by_grid_snaps_to_cells() {
        let mut s = Sprite::new(8, 8, ColorMode::Rgba);
        s.grid = pxl_ase_doc::Grid { x: 0, y: 0, w: 4, h: 4 };
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let mut px = vec![0u8; 8 * 8 * 4];
        // opaque blob at (5,5)-(5,5) → content in cell (4..8,4..8)
        let i = (5 * 8 + 5) * 4;
        px[i] = 1; px[i+1] = 2; px[i+2] = 3; px[i+3] = 255;
        let img = RgbaImage::from_rgba(8, 8, px);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        let bb = trim_sprite_by_grid(&mut s).unwrap().unwrap();
        assert_eq!(bb, (4, 4, 4, 4));
        assert_eq!((s.width, s.height), (4, 4));
    }

    #[test]
    fn shrink_to_fits_without_upscale() {
        let mut s = Sprite::new(8, 8, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let mut px = vec![0u8; 8 * 8 * 4];
        px[0] = 1; px[1] = 2; px[2] = 3; px[3] = 255;
        let img = RgbaImage::from_rgba(8, 8, px);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        let (w, h) = shrink_sprite_to(&mut s, 16, 16).unwrap();
        assert_eq!((w, h), (8, 8), "already fits: unchanged");
        let (w, h) = shrink_sprite_to(&mut s, 4, 2).unwrap();
        assert_eq!((w, h), (2, 2), "aspect-preserving fit inside 4x2");
    }

}
