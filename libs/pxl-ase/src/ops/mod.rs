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
    if sprite.color_mode != ColorMode::Indexed {
        crate::color::convert_sprite_color_mode(sprite, ColorMode::Indexed)?;
    }
    let pal = crate::sprite_palette(sprite);
    let rgba = flatten_frame_rgba(sprite, frame)?;
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
    let cx = ApplyCx {
        palette: &pal,
        seed,
        frame_name: "ase",
        load_template: None,
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    };
    index = apply_op(&index, op, &cx, 0).map_err(|e| AseErr(e.message))?;

    let frames = sprite.frame_count();
    let mut all = Vec::with_capacity(frames);
    for f in 0..frames {
        if f as u16 == frame {
            all.push(index.pixels.clone());
        } else {
            let rgba = flatten_frame_rgba(sprite, f as u16)?;
            all.push(pxl_art::quantize::map_to_palette(
                sprite.width as usize,
                sprite.height as usize,
                &rgba,
                &pal,
                0,
            ));
        }
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
