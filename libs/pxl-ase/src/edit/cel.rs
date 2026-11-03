//! GD-4 cel move / unlink / clear / position / opacity.

use pxl_ase_doc::{Cel, FrameIndex, Sprite, UserData};

use crate::{AseErr, Result};

fn layer_pos(sprite: &Sprite, name: &str) -> Result<usize> {
    sprite
        .layers
        .iter()
        .position(|l| l.name == name)
        .ok_or_else(|| AseErr(format!("layer '{name}' not found")))
}

pub fn cel_clear(sprite: &mut Sprite, layer: &str, frame: FrameIndex) -> Result<()> {
    let i = layer_pos(sprite, layer)?;
    if sprite.layers[i].cels.remove(&frame).is_none() {
        return Err(AseErr(format!("no cel on layer '{layer}' frame {frame}")));
    }
    Ok(())
}

pub fn cel_unlink(sprite: &mut Sprite, layer: &str, frame: FrameIndex) -> Result<()> {
    let i = layer_pos(sprite, layer)?;
    let cel = sprite.layers[i]
        .cels
        .get(&frame)
        .cloned()
        .ok_or_else(|| AseErr(format!("no cel on layer '{layer}' frame {frame}")))?;
    match cel {
        Cel::Linked { .. } => {
            // Resolve pixels and materialize a private image cel at (0,0).
            let px = sprite.cel_pixels(sprite.layers[i].id, frame)?;
            use pxl_ase_doc::ImageBuffer;
            let (img, x, y, opacity) = match px {
                pxl_ase_doc::CelPixels::Rgba { image, x, y, opacity } => {
                    (ImageBuffer::Rgba(image), x, y, opacity)
                }
                pxl_ase_doc::CelPixels::Indexed { image, x, y, opacity } => {
                    (ImageBuffer::Indexed(image), x, y, opacity)
                }
                pxl_ase_doc::CelPixels::Gray { image, x, y, opacity } => {
                    (ImageBuffer::Grayscale(image), x, y, opacity)
                }
            };
            let id = sprite.insert_image(img);
            let z = sprite.layers[i].cels.get(&frame).map(|c| c.z_index()).unwrap_or(0);
            sprite.layers[i].cels.insert(frame, Cel::Image {
                image: id,
                x,
                y,
                opacity,
                z_index: z,
                user_data: UserData::default(),
                extra: None,
            });
            Ok(())
        }
        Cel::Image { .. } | Cel::Tilemap { .. } => Ok(()), // already unlinked
    }
}

pub fn cel_move(
    sprite: &mut Sprite,
    layer: &str,
    from: FrameIndex,
    to: FrameIndex,
) -> Result<()> {
    let i = layer_pos(sprite, layer)?;
    let cel = sprite.layers[i]
        .cels
        .remove(&from)
        .ok_or_else(|| AseErr(format!("no cel on layer '{layer}' frame {from}")))?;
    sprite.ensure_frames(to as usize + 1);
    sprite.layers[i].cels.insert(to, cel);
    Ok(())
}

pub fn cel_set_position(
    sprite: &mut Sprite,
    layer: &str,
    frame: FrameIndex,
    x: i16,
    y: i16,
) -> Result<()> {
    let i = layer_pos(sprite, layer)?;
    let cel = sprite.layers[i]
        .cels
        .get_mut(&frame)
        .ok_or_else(|| AseErr(format!("no cel on layer '{layer}' frame {frame}")))?;
    match cel {
        Cel::Image { x: cx, y: cy, .. } | Cel::Tilemap { x: cx, y: cy, .. } => {
            *cx = x;
            *cy = y;
        }
        Cel::Linked { .. } => {
            return Err(AseErr("unlink cel before setting position on a linked cel".into()));
        }
    }
    Ok(())
}

pub fn cel_set_opacity(
    sprite: &mut Sprite,
    layer: &str,
    frame: FrameIndex,
    opacity: u8,
) -> Result<()> {
    let i = layer_pos(sprite, layer)?;
    let cel = sprite.layers[i]
        .cels
        .get_mut(&frame)
        .ok_or_else(|| AseErr(format!("no cel on layer '{layer}' frame {frame}")))?;
    match cel {
        Cel::Image { opacity: o, .. } | Cel::Tilemap { opacity: o, .. } => *o = opacity,
        Cel::Linked { .. } => {
            return Err(AseErr("unlink cel before setting opacity on a linked cel".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{ColorMode, ImageBuffer, RgbaImage};

    #[test]
    fn cel_unlink_and_clear() {
        let mut s = Sprite::new(2, 2, ColorMode::Rgba);
        s.frame_durations_ms = vec![100, 100];
        let layer = s.add_image_layer("L");
        let img = RgbaImage::from_rgba(2, 2, vec![255u8; 16]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 1, 2, 200));
        s.set_cel(layer, 1, Cel::linked(0));
        cel_unlink(&mut s, "L", 1).unwrap();
        assert!(matches!(s.layers[0].cels.get(&1), Some(Cel::Image { .. })));
        cel_set_position(&mut s, "L", 1, 3, 4).unwrap();
        cel_clear(&mut s, "L", 1).unwrap();
        assert!(s.layers[0].cels.get(&1).is_none());
    }
}
