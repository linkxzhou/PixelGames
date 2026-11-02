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

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, Sprite};
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
}
