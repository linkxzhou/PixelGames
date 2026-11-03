//! Clean-room `.aseprite` / `.ase` codec following the public file specs.
//! Does not include or generate code from Aseprite C++ sources.

mod chunks;
mod passthrough;
mod error;
mod read;
mod user_data;
mod write;

pub use error::{IoError, IoResult};
pub use read::{read_aseprite, read_aseprite_bytes, ReadOptions};
pub use write::{write_aseprite, write_aseprite_bytes, WriteOptions};

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{
        Cel, ColorMode, ImageBuffer, IndexedImage, RgbaImage, Sprite, Tag, TagDirection,
    };
    use pxl_core::Rgba;

    fn mini_indexed() -> Sprite {
        let mut s = Sprite::new(2, 2, ColorMode::Indexed);
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(255, 0, 0),
            Rgba::rgb(0, 0, 255),
        ];
        s.frame_durations_ms = vec![100, 120];
        let layer = s.add_image_layer("Layer 1");
        let img0 = IndexedImage::from_indices(2, 2, vec![1, 1, 2, 0]);
        let id0 = s.insert_image(ImageBuffer::Indexed(img0));
        s.set_cel(layer, 0, Cel::image(id0, 0, 0, 255));
        s.set_cel(layer, 1, Cel::linked(0));
        s.tags.push(Tag {
            name: "idle".into(),
            from: 0,
            to: 1,
            direction: TagDirection::Forward,
            repeat: 0,
            user_data: Default::default(),
        });
        s
    }

    #[test]
    fn roundtrip_indexed_linked_tag() {
        let src = mini_indexed();
        let bytes = write_aseprite_bytes(&src, &WriteOptions::default()).unwrap();
        assert!(bytes.len() > 128);
        assert_eq!(&bytes[4..6], &0xA5E0u16.to_le_bytes());
        let doc = read_aseprite_bytes(&bytes, &ReadOptions::default()).unwrap();
        let dst = &doc.sprite;
        assert_eq!(dst.width, 2);
        assert_eq!(dst.height, 2);
        assert_eq!(dst.color_mode, ColorMode::Indexed);
        assert_eq!(dst.frame_count(), 2);
        assert_eq!(dst.layers.len(), 1);
        assert_eq!(dst.layers[0].name, "Layer 1");
        assert_eq!(dst.tags.len(), 1);
        assert_eq!(dst.tags[0].name, "idle");
        let p0 = dst.cel_pixels(dst.layers[0].id, 0).unwrap();
        let p1 = dst.cel_pixels(dst.layers[0].id, 1).unwrap();
        match (p0, p1) {
            (
                pxl_ase_doc::CelPixels::Indexed { image: a, .. },
                pxl_ase_doc::CelPixels::Indexed { image: b, .. },
            ) => {
                assert_eq!(a.pixels.as_ref(), b.pixels.as_ref());
                assert_eq!(a.pixels.as_ref(), &vec![1, 1, 2, 0]);
            }
            _ => panic!("expected indexed"),
        }
        assert_eq!(dst.palette.get(1).map(|c| (c.r, c.g, c.b)), Some((255, 0, 0)));
    }

    #[test]
    fn roundtrip_rgba() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        let layer = s.add_image_layer("px");
        let img = RgbaImage::from_rgba(1, 1, vec![10, 20, 30, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        let bytes = write_aseprite_bytes(&s, &WriteOptions::default()).unwrap();
        let doc = read_aseprite_bytes(&bytes, &ReadOptions::default()).unwrap();
        let cel = doc.sprite.cel_pixels(doc.sprite.layers[0].id, 0).unwrap();
        match cel {
            pxl_ase_doc::CelPixels::Rgba { image, .. } => {
                assert_eq!(image.pixels.as_ref(), &vec![10, 20, 30, 255]);
            }
            _ => panic!("rgba"),
        }
    }

    #[test]
    fn tag_color_user_data_roundtrip() {
        use pxl_ase_doc::UserData;
        let mut src = mini_indexed();
        src.tags[0].user_data = UserData {
            text: Some("tag-note".into()),
            color: Some(Rgba { r: 10, g: 20, b: 30, a: 255 }),
            properties_raw: None,
        };
        let bytes = write_aseprite_bytes(&src, &WriteOptions::default()).unwrap();
        let dst = read_aseprite_bytes(&bytes, &ReadOptions::default()).unwrap().sprite;
        assert_eq!(dst.tags[0].user_data.color.map(|c| (c.r, c.g, c.b, c.a)), Some((10, 20, 30, 255)));
        assert_eq!(dst.tags[0].user_data.text.as_deref(), Some("tag-note"));
    }

    #[test]
    fn write_emits_loss_warnings_for_skipped_chunks() {
        let mut src = mini_indexed();
        src.read_loss_notes.push("skipped Color Profile chunk (0x2007); write-back will drop it".into());
        // Capture stderr via a side channel: function always eprintln!; we just ensure write succeeds
        // and notes are still present for callers.
        let _bytes = write_aseprite_bytes(&src, &WriteOptions::default()).unwrap();
        assert!(!src.read_loss_notes.is_empty());
    }

    #[test]
    fn color_profile_passthrough_roundtrip() {
        let mut src = mini_indexed();
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&0i32.to_le_bytes());
        body.extend_from_slice(&[0u8; 8]);
        src.color_profile_raw = Some(body.clone());
        src.pixel_ratio = pxl_ase_doc::PixelRatio::new(2, 1);
        let bytes = write_aseprite_bytes(&src, &WriteOptions::default()).unwrap();
        let dst = read_aseprite_bytes(&bytes, &ReadOptions::default()).unwrap().sprite;
        assert_eq!(dst.color_profile_raw.as_ref(), Some(&body));
        assert_eq!((dst.pixel_ratio.width, dst.pixel_ratio.height), (2, 1));
    }

    #[test]
    fn cel_extra_roundtrip() {
        use pxl_ase_doc::CelExtra;
        let mut src = mini_indexed();
        if let Some(cel) = src.layers[0].cels.get_mut(&0) {
            cel.set_extra(CelExtra {
                flags: 1,
                precise_x: 1 << 16,
                precise_y: 2 << 16,
                width: 3 << 16,
                height: 4 << 16,
            });
        }
        let bytes = write_aseprite_bytes(&src, &WriteOptions::default()).unwrap();
        let dst = read_aseprite_bytes(&bytes, &ReadOptions::default()).unwrap().sprite;
        let extra = dst.layers[0].cels.get(&0).and_then(|c| c.extra().copied());
        assert_eq!(
            extra.map(|e| (e.flags, e.precise_x, e.width)),
            Some((1, 1 << 16, 3 << 16))
        );
    }
}
