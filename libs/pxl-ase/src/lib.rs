//! Headless Aseprite-aligned batch helpers. Modules mirror planned crates
//! `pxl-ase-color` / `filters` / `ops` / `render` (merged for delivery speed).

pub mod batch;
pub mod color;
pub mod filters;
pub mod import;
pub mod ops;
pub mod render;
pub mod script;

mod export_png;
mod frames;
mod sheet;

pub use pxl_ase_doc as doc;
pub use pxl_ase_io as io;

pub use export_png::{
    expand_filename_format, expand_filename_format_ctx, export_frame_png, export_slice_png,
    split_slices_to_dir, sprite_palette, FilenameFormatCtx,
};
pub use frames::{
    expand_tag_frames, frame_is_empty, frames_for_tag, frames_for_tag_opts, parse_frame_range,
    select_frames, select_frames_opts,
};
pub use sheet::{
    export_gif, export_sheet_png_json, export_sheet_png_json_layout,
    export_sheet_png_json_layout_opts, merge_duplicate_frames, set_all_layers_visible,
    set_layer_visibility_by_name, SheetExportOpts,
};


use pxl_ase_doc::{AseDoc, ColorMode, Slice, Tag, TagDirection};
use pxl_ase_io::{read_aseprite, ReadOptions};
use serde_json::{json, Value};


#[derive(Debug)]
pub struct AseErr(pub String);

impl std::fmt::Display for AseErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<pxl_ase_io::IoError> for AseErr {
    fn from(e: pxl_ase_io::IoError) -> Self {
        AseErr(e.to_string())
    }
}

impl From<pxl_ase_doc::AseError> for AseErr {
    fn from(e: pxl_ase_doc::AseError) -> Self {
        AseErr(e.to_string())
    }
}

impl From<pxl_core::PxlError> for AseErr {
    fn from(e: pxl_core::PxlError) -> Self {
        AseErr(e.message)
    }
}

pub type Result<T> = std::result::Result<T, AseErr>;

pub fn open(path: &str, one_frame: bool) -> Result<AseDoc> {
    Ok(read_aseprite(
        path,
        &ReadOptions {
            one_frame,
        },
    )?)
}

pub fn inspect_json(doc: &AseDoc) -> Value {
    let s = &doc.sprite;
    json!({
        "path": doc.path,
        "width": s.width,
        "height": s.height,
        "color_mode": match s.color_mode {
            ColorMode::Rgba => "rgba",
            ColorMode::Grayscale => "grayscale",
            ColorMode::Indexed => "indexed",
        },
        "frames": s.frame_count(),
        "frame_durations_ms": s.frame_durations_ms,
        "transparent_index": s.transparent_index,
        "layers": s.layers.iter().map(|l| json!({
            "id": l.id.0,
            "name": l.name,
            "kind": match l.kind {
                pxl_ase_doc::LayerKind::Image => "image",
                pxl_ase_doc::LayerKind::Group => "group",
                pxl_ase_doc::LayerKind::Tilemap => "tilemap",
            },
            "visible": l.flags.visible,
            "child_level": l.child_level,
            "cels": l.cels.len(),
        })).collect::<Vec<_>>(),
        "tags": s.tags.iter().map(tag_json).collect::<Vec<_>>(),
        "slices": s.slices.iter().map(slice_json).collect::<Vec<_>>(),
        "palette_len": s.palette.len(),
        "grid": {
            "x": s.grid.x,
            "y": s.grid.y,
            "w": s.grid.w,
            "h": s.grid.h,
        }
    })
}

pub(crate) fn tag_direction_name(d: TagDirection) -> &'static str {
    match d {
        TagDirection::Forward => "forward",
        TagDirection::Reverse => "reverse",
        TagDirection::PingPong => "pingpong",
        TagDirection::PingPongReverse => "pingpong_reverse",
    }
}

fn tag_json(t: &Tag) -> Value {
    json!({
        "name": t.name,
        "from": t.from,
        "to": t.to,
        "direction": tag_direction_name(t.direction),
        "direction_u8": t.direction.to_u8(),
        "repeat": t.repeat,
    })
}

pub(crate) fn slice_json(sl: &Slice) -> Value {
    json!({
        "name": sl.name,
        "keys": sl.keys.iter().map(|k| json!({
            "frame": k.frame,
            "x": k.x,
            "y": k.y,
            "w": k.w,
            "h": k.h,
            "pivot": k.pivot,
            "center": k.center,
        })).collect::<Vec<_>>(),
    })
}

pub fn list_layers_text(doc: &AseDoc) -> String {
    let mut lines = Vec::new();
    for l in &doc.sprite.layers {
        let pad = "  ".repeat(l.child_level as usize);
        let vis = if l.flags.visible { "" } else { " (hidden)" };
        lines.push(format!("{pad}{}{vis}", l.name));
    }
    lines.join("\n")
}

pub fn list_tags_text(doc: &AseDoc) -> String {
    doc.sprite
        .tags
        .iter()
        .map(|t| {
            format!(
                "{}: {}-{} ({})",
                t.name,
                t.from,
                t.to,
                tag_direction_name(t.direction)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn list_slices_text(doc: &AseDoc) -> String {
    doc.sprite
        .slices
        .iter()
        .map(|s| {
            let k = s.keys.first();
            match k {
                Some(k) => format!(
                    "{}: {}x{} @({},{}) frame={}",
                    s.name, k.w, k.h, k.x, k.y, k.frame
                ),
                None => format!("{}: (no keys)", s.name),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn list_slices_json(doc: &AseDoc) -> Value {
    json!(doc.sprite.slices.iter().map(slice_json).collect::<Vec<_>>())
}


#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, SliceKey, Sprite, TagDirection};
    use pxl_core::Rgba;

    fn fixture() -> Sprite {
        let mut s = Sprite::new(4, 4, ColorMode::Indexed);
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(255, 64, 64),
            Rgba::rgb(64, 128, 255),
        ];
        s.frame_durations_ms = vec![100, 120];
        let layer = s.add_image_layer("hero");
        let img = IndexedImage::from_indices(
            4,
            4,
            vec![0, 1, 1, 0, 1, 2, 2, 1, 1, 2, 2, 1, 0, 1, 1, 0],
        );
        let id = s.insert_image(ImageBuffer::Indexed(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s.set_cel(layer, 1, Cel::linked(0));
        s.tags.push(Tag {
            name: "idle".into(),
            from: 0,
            to: 1,
            direction: TagDirection::PingPong,
            repeat: 0,
            user_data: Default::default(),
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
            user_data: Default::default(),
        });
        s
    }

    #[test]
    fn frame_range_and_filename_format() {
        let s = fixture();
        assert_eq!(parse_frame_range("0,1", 2).unwrap(), vec![0, 1]);
        assert_eq!(parse_frame_range("0-1", 2).unwrap(), vec![0, 1]);
        let name = expand_filename_format(
            "f{frame}_{tag}.png",
            3,
            "idle",
            "hero",
            "mini",
            "mini.aseprite",
        );
        assert_eq!(name, "f3_idle.png");
        let name2 = expand_filename_format_ctx(
            "{title}_{frame001}_{duration}.png",
            &FilenameFormatCtx {
                frame: 7,
                tag: "idle",
                layer: "hero",
                title: "mini",
                path: "mini.aseprite",
                duration_ms: 120,
                ..Default::default()
            },
        );
        assert_eq!(name2, "mini_007_120.png");
        let _ = s;
    }

    #[test]
    fn sheet_padding_and_merge() {
        let s = fixture();
        let opts = SheetExportOpts {
            padding: 1,
            extrude: 1,
            trim: true,
            merge_duplicates: true,
        };
        let (png, js) =
            export_sheet_png_json_layout_opts(&s, &[0, 1], "s.png", "horizontal", &opts).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(js.contains("frame_"));
    }

    #[test]
    fn gif_and_slice_export() {
        let s = fixture();
        let gif = export_gif(&s, &[0, 1], 1).unwrap();
        assert!(gif.starts_with(b"GIF"));
        let png = export_slice_png(&s, 0, &s.slices[0], 1).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let (_png, js) = export_sheet_png_json_layout(&s, &[0, 1], "s.png", "horizontal").unwrap();
        assert!(js.contains("pingpong"));
        assert!(js.contains("body"));
    }
    #[test]
    fn expand_tag_direction_and_play_subtags() {
        let mut s = fixture();
        s.frame_durations_ms = vec![100, 100, 100, 100, 100];
        s.tags.clear();
        s.tags.push(Tag {
            name: "root".into(),
            from: 0,
            to: 4,
            direction: TagDirection::Forward,
            repeat: 0,
            user_data: Default::default(),
        });
        s.tags.push(Tag {
            name: "sub".into(),
            from: 1,
            to: 3,
            direction: TagDirection::Reverse,
            repeat: 0,
            user_data: Default::default(),
        });
        assert_eq!(expand_tag_frames(&s.tags[1]), vec![3, 2, 1]);
        let frames = frames_for_tag_opts(&s, Some("root"), true).unwrap();
        assert_eq!(frames, vec![0, 3, 2, 1, 4]);
        let plain = frames_for_tag_opts(&s, Some("root"), false).unwrap();
        assert_eq!(plain, vec![0, 1, 2, 3, 4]);
    }

}

#[cfg(test)]
mod z_index_render_tests {
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, RgbaImage, Sprite};

    use crate::render::flatten_frame_rgba;

    #[test]
    fn z_index_changes_paint_order() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        let l0 = s.add_image_layer("A");
        let l1 = s.add_image_layer("B");
        let red = s.insert_image(ImageBuffer::Rgba(RgbaImage::from_rgba(
            1, 1, vec![255, 0, 0, 255],
        )));
        let blue = s.insert_image(ImageBuffer::Rgba(RgbaImage::from_rgba(
            1, 1, vec![0, 0, 255, 255],
        )));
        s.set_cel(
            l0,
            0,
            Cel::Image {
                image: red,
                x: 0,
                y: 0,
                opacity: 255,
                z_index: 0,
                user_data: Default::default(),
            extra: None,
        },
        );
        s.set_cel(
            l1,
            0,
            Cel::Image {
                image: blue,
                x: 0,
                y: 0,
                opacity: 255,
                z_index: 0,
                user_data: Default::default(),
            extra: None,
        },
        );
        let flat = flatten_frame_rgba(&s, 0).unwrap();
        assert_eq!(&flat[0..4], &[0, 0, 255, 255], "default: later layer on top");

        if let Some(Cel::Image { z_index, .. }) = s.layers[0].cels.get_mut(&0) {
            *z_index = 2;
        }
        let flat2 = flatten_frame_rgba(&s, 0).unwrap();
        assert_eq!(&flat2[0..4], &[255, 0, 0, 255], "z_index=2 lifts layer 0 above layer 1");
    }
}
