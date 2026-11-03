//! Clean-room document model aligned with Aseprite *concepts* and the public
//! `.aseprite` field names. No Aseprite C++ sources are embedded.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod blend;
mod bridge;
mod color_mode;
mod error;
mod image;
mod layer;
mod sprite;
mod types;
mod user_data;
mod pixel_ratio;
mod cel_extra;

pub use blend::BlendMode;
pub use bridge::{ensure_transparent_palette, indexed_cel_to_frame, palette_from_rgba, sprite_from_index_frame};
pub use color_mode::ColorMode;
pub use error::{AseError, AseResult};
pub use image::{GrayImage, ImageBuffer, IndexedImage, RgbaImage};
pub use layer::{Cel, CelPixels, Layer, LayerFlags, LayerKind};
pub use sprite::{AseDoc, Grid, Slice, SliceKey, Sprite, Tag, TagDirection};
pub use types::{CelId, FrameIndex, ImageId, LayerId};
pub use user_data::UserData;
pub use pixel_ratio::PixelRatio;
pub use cel_extra::CelExtra;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use pxl_core::{IndexFrame, Palette, Rgba};

    #[test]
    fn linked_cels_share_pixels() {
        let mut sprite = Sprite::new(4, 4, ColorMode::Indexed);
        let layer = sprite.add_image_layer("L0");
        let img = IndexedImage::filled(4, 4, 1);
        let id = sprite.insert_image(ImageBuffer::Indexed(img));
        sprite.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        sprite.set_cel(layer, 1, Cel::linked(0));
        let a = sprite.cel_pixels(layer, 0).expect("f0");
        let b = sprite.cel_pixels(layer, 1).expect("f1");
        match (a, b) {
            (CelPixels::Indexed { image: x, .. }, CelPixels::Indexed { image: y, .. }) => {
                assert!(Arc::ptr_eq(&x.pixels, &y.pixels));
            }
            _ => panic!("expected indexed"),
        }
        sprite.ensure_frames(3);
        assert_eq!(sprite.frame_count(), 3);
    }

    #[test]
    fn bridge_roundtrip_hash() {
        let mut frame = IndexFrame::new(2, 2).unwrap();
        frame.pixels = alloc::vec![1, 2, 1, 2];
        let pal = Palette {
            id: alloc::string::String::from("t"),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(255, 0, 0),
                Rgba::rgb(0, 255, 0),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: Default::default(),
        };
        let sprite = sprite_from_index_frame(&frame, &pal, "main").unwrap();
        let back = indexed_cel_to_frame(&sprite, LayerId(0), 0).unwrap();
        assert_eq!(frame.pixels, back.pixels);
        assert_eq!(frame.hash_bytes(), back.hash_bytes());
    }

    #[test]
    fn tag_slice_grid_fields() {
        let mut s = Sprite::new(8, 8, ColorMode::Rgba);
        s.tags.push(Tag {
            name: alloc::string::String::from("walk"),
            from: 0,
            to: 2,
            direction: TagDirection::Forward,
            repeat: 0,
            user_data: UserData::default(),
        });
        s.slices.push(Slice {
            name: alloc::string::String::from("body"),
            keys: alloc::vec![SliceKey {
                frame: 0,
                x: 1,
                y: 1,
                w: 4,
                h: 4,
                center: None,
                pivot: Some((2, 3)),
            }],
            user_data: UserData::default(),
        });
        s.grid = Grid {
            x: 0,
            y: 0,
            w: 16,
            h: 16,
        };
        assert_eq!(s.tags[0].name, "walk");
        assert_eq!(s.slices[0].keys[0].pivot, Some((2, 3)));
    }
}
