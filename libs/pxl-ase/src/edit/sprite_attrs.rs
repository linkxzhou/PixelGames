//! GD-6 sprite attribute setters.

use pxl_ase_doc::{Grid, PixelRatio, Sprite};

use crate::{AseErr, Result};

pub struct SpriteAttrPatch {
    pub transparent_index: Option<u8>,
    pub pixel_ratio: Option<(u8, u8)>,
    pub grid: Option<Grid>,
}

pub fn sprite_set_attrs(sprite: &mut Sprite, patch: SpriteAttrPatch) -> Result<()> {
    if let Some(t) = patch.transparent_index {
        sprite.transparent_index = t;
    }
    if let Some((w, h)) = patch.pixel_ratio {
        if w == 0 || h == 0 {
            return Err(AseErr("pixel ratio sides must be non-zero".into()));
        }
        sprite.pixel_ratio = PixelRatio { width: w, height: h };
    }
    if let Some(g) = patch.grid {
        sprite.grid = g;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::ColorMode;

    #[test]
    fn set_attrs() {
        let mut s = Sprite::new(4, 4, ColorMode::Indexed);
        sprite_set_attrs(
            &mut s,
            SpriteAttrPatch {
                transparent_index: Some(3),
                pixel_ratio: Some((2, 1)),
                grid: Some(Grid { x: 0, y: 0, w: 8, h: 8 }),
            },
        )
        .unwrap();
        assert_eq!(s.transparent_index, 3);
        assert_eq!(s.pixel_ratio.width, 2);
        assert_eq!(s.grid.w, 8);
    }
}
