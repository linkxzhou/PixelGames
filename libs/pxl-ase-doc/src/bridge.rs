use alloc::string::String;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Palette, Rgba};

use crate::color_mode::ColorMode;
use crate::error::{AseError, AseResult};
use crate::image::{ImageBuffer, IndexedImage};
use crate::layer::Cel;
use crate::sprite::Sprite;
use crate::types::{FrameIndex, LayerId};

/// Build a single-layer indexed sprite from an [`IndexFrame`] + [`Palette`].
pub fn sprite_from_index_frame(
    frame: &IndexFrame,
    palette: &Palette,
    layer_name: &str,
) -> AseResult<Sprite> {
    let mut sprite = Sprite::new(frame.w, frame.h, ColorMode::Indexed);
    sprite.palette = palette.colors.clone();
    let layer = sprite.add_image_layer(layer_name);
    let img = IndexedImage::from_indices(frame.w, frame.h, frame.pixels.clone());
    let id = sprite.insert_image(ImageBuffer::Indexed(img));
    sprite.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
    Ok(sprite)
}

/// Extract an indexed cel as [`IndexFrame`]. Sprite must be Indexed.
pub fn indexed_cel_to_frame(
    sprite: &Sprite,
    layer: LayerId,
    frame: FrameIndex,
) -> AseResult<IndexFrame> {
    if sprite.color_mode != ColorMode::Indexed {
        return Err(AseError::new("sprite is not indexed").with_hint("convert first"));
    }
    let cel = sprite.cel_pixels(layer, frame)?;
    match cel {
        crate::layer::CelPixels::Indexed { image, x, y, .. } => {
            // Place into full canvas.
            let mut out = IndexFrame::canvas(sprite.width, sprite.height).map_err(|e| {
                AseError::new(alloc::format!("index frame: {}", e.message))
            })?;
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = x as i32 + px;
                    let dy = y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= sprite.width as i32 || dy >= sprite.height as i32 {
                        continue;
                    }
                    let idx = (py as usize) * (image.width as usize) + (px as usize);
                    let v = image.pixels[idx];
                    if v != sprite.transparent_index {
                        out.set(dx, dy, v);
                    }
                }
            }
            Ok(out)
        }
        _ => Err(AseError::new("cel is not indexed")),
    }
}

pub fn palette_from_rgba(colors: &[Rgba]) -> Palette {
    Palette {
        id: String::from("ase"),
        colors: colors.to_vec(),
        ramps: Default::default(),
        cycles: Default::default(),
        special: Default::default(),
    }
}

pub fn ensure_transparent_palette(colors: &mut Vec<Rgba>, transparent_index: u8) {
    while colors.len() <= transparent_index as usize {
        colors.push(Rgba::TRANSPARENT);
    }
    if let Some(c) = colors.get_mut(transparent_index as usize) {
        *c = Rgba::TRANSPARENT;
    }
}
