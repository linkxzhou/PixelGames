use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, Slice, SliceKey, Sprite, Tag, TagDirection};
use pxl_ase_io::{write_aseprite, WriteOptions};
use pxl_core::Rgba;
use std::path::PathBuf;

fn main() {
    let mut s = Sprite::new(4, 4, ColorMode::Indexed);
    s.palette = vec![
        Rgba::TRANSPARENT,
        Rgba::rgb(255, 64, 64),
        Rgba::rgb(64, 128, 255),
        Rgba::rgb(255, 220, 64),
    ];
    s.frame_durations_ms = vec![100, 100, 100];
    let layer = s.add_image_layer("hero");
    let img = IndexedImage::from_indices(
        4,
        4,
        vec![
            0, 1, 1, 0,
            1, 2, 2, 1,
            1, 2, 3, 1,
            0, 1, 1, 0,
        ],
    );
    let id = s.insert_image(ImageBuffer::Indexed(img));
    s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
    s.set_cel(layer, 1, Cel::linked(0));
    s.set_cel(layer, 2, Cel::linked(0));
    s.tags.push(Tag {
        name: "idle".into(),
        from: 0,
        to: 2,
        direction: TagDirection::Forward,
        repeat: 0,
    });
    s.slices.push(Slice {
        name: "body".into(),
        keys: vec![SliceKey {
            frame: 0,
            x: 1,
            y: 1,
            w: 2,
            h: 2,
            center: None,
            pivot: Some((1, 1)),
        }],
    });
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fix = root.join("tests/fixtures/mini_indexed.aseprite");
    write_aseprite(&fix, &s, &WriteOptions::default()).unwrap();
    let pilot = root
        .join("../../work/v8-ase-pilot");
    std::fs::create_dir_all(&pilot).unwrap();
    write_aseprite(pilot.join("mini_indexed.aseprite"), &s, &WriteOptions::default()).unwrap();
    println!("wrote {}", fix.display());
}
