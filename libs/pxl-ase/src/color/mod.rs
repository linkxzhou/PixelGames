//! Color-mode conversion, remap, quantize (merged `pxl-ase-color`).

use std::sync::Arc;

use pxl_ase_doc::{ColorMode, ImageBuffer, IndexedImage, RgbaImage, Sprite};
use pxl_art::quantize::{map_to_palette, median_cut};
use pxl_core::Rgba;

use crate::render::flatten_frame_rgba;
use crate::{sprite_palette, AseErr, Result};

pub fn quantize_rgba(rgba_stream: &[u8], max_colors: usize) -> Vec<Rgba> {
    median_cut(rgba_stream, max_colors.max(1), 4)
}

pub fn convert_sprite_color_mode(sprite: &mut Sprite, target: ColorMode) -> Result<()> {
    if sprite.color_mode == target {
        return Ok(());
    }
    let frames = sprite.frame_count();
    let mut flat = Vec::with_capacity(frames);
    for f in 0..frames {
        flat.push(flatten_frame_rgba(sprite, f as u16)?);
    }
    match target {
        ColorMode::Rgba => {
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let img = RgbaImage::from_rgba(sprite.width, sprite.height, rgba);
                let id = sprite.insert_image(ImageBuffer::Rgba(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Rgba;
        }
        ColorMode::Indexed => {
            let mut samples = Vec::new();
            for rgba in &flat {
                samples.extend_from_slice(rgba);
            }
            let colors = quantize_rgba(&samples, 32);
            sprite.palette = colors;
            let pal = sprite_palette(sprite);
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let indices = map_to_palette(
                    sprite.width as usize,
                    sprite.height as usize,
                    &rgba,
                    &pal,
                    0,
                );
                let img = IndexedImage::from_indices(sprite.width, sprite.height, indices);
                let id = sprite.insert_image(ImageBuffer::Indexed(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Indexed;
        }
        ColorMode::Grayscale => {
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let mut va = Vec::with_capacity(rgba.len() / 2);
                for chunk in rgba.chunks_exact(4) {
                    let y = ((chunk[0] as u16 * 77 + chunk[1] as u16 * 150 + chunk[2] as u16 * 29)
                        / 256) as u8;
                    va.push(y);
                    va.push(chunk[3]);
                }
                let img = pxl_ase_doc::GrayImage::from_va(sprite.width, sprite.height, va);
                let id = sprite.insert_image(ImageBuffer::Grayscale(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Grayscale;
        }
    }
    Ok(())
}

pub fn remap_palette_indices(sprite: &mut Sprite, map: &[u8]) -> Result<()> {
    if sprite.color_mode != ColorMode::Indexed {
        return Err(AseErr("remap requires indexed color mode".into()));
    }
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Indexed(ix) = img {
            let mut px = ix.pixels.as_ref().clone();
            for p in &mut px {
                *p = map.get(*p as usize).copied().unwrap_or(0);
            }
            ix.pixels = Arc::new(px);
        }
    }
    Ok(())
}
