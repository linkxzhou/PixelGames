//! GD-5 flatten visible layers into one image layer (write-back).

use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, RgbaImage, Sprite};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

/// Replace all layers with a single "Flattened" RGBA layer preserving each frame.
pub fn flatten_layers(sprite: &mut Sprite) -> Result<()> {
    let frames = sprite.frame_count();
    if frames == 0 {
        return Err(AseErr("sprite has no frames".into()));
    }
    let w = sprite.width;
    let h = sprite.height;
    let mut flats = Vec::with_capacity(frames);
    for f in 0..frames {
        flats.push(flatten_frame_rgba(sprite, f as u16)?);
    }
    sprite.layers.clear();
    sprite.images.clear();
    sprite.color_mode = ColorMode::Rgba;
    let layer = sprite.add_image_layer("Flattened");
    for (fi, px) in flats.into_iter().enumerate() {
        let img = RgbaImage::from_rgba(w, h, px);
        let id = sprite.insert_image(ImageBuffer::Rgba(img));
        sprite.set_cel(layer, fi as u16, Cel::image(id, 0, 0, 255));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_core::Rgba;

    #[test]
    fn flatten_merges_to_one_layer() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        s.palette = vec![Rgba::TRANSPARENT];
        let l0 = s.add_image_layer("a");
        let l1 = s.add_image_layer("b");
        let img = RgbaImage::from_rgba(1, 1, vec![10, 20, 30, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(l0, 0, Cel::image(id, 0, 0, 255));
        let img2 = RgbaImage::from_rgba(1, 1, vec![0, 0, 0, 0]);
        let id2 = s.insert_image(ImageBuffer::Rgba(img2));
        s.set_cel(l1, 0, Cel::image(id2, 0, 0, 255));
        flatten_layers(&mut s).unwrap();
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].name, "Flattened");
    }
}
