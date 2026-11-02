//! Headless Aseprite-aligned batch helpers. Modules mirror planned crates
//! `pxl-ase-color` / `filters` / `ops` / `render` (merged for delivery speed).

pub mod color;
pub mod filters;
pub mod ops;
pub mod render;

pub use pxl_ase_doc as doc;
pub use pxl_ase_io as io;

use pxl_ase_doc::{AseDoc, ColorMode, Sprite, Tag};
use pxl_ase_io::{read_aseprite, ReadOptions};
use pxl_core::{Palette, Rgba};
use serde_json::{json, Value};

use crate::render::flatten_frame_rgba;

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
        "slices": s.slices.iter().map(|sl| json!({
            "name": sl.name,
            "keys": sl.keys.len(),
        })).collect::<Vec<_>>(),
        "palette_len": s.palette.len(),
        "grid": {
            "x": s.grid.x,
            "y": s.grid.y,
            "w": s.grid.w,
            "h": s.grid.h,
        }
    })
}

fn tag_json(t: &Tag) -> Value {
    json!({
        "name": t.name,
        "from": t.from,
        "to": t.to,
        "direction": t.direction.to_u8(),
        "repeat": t.repeat,
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
        .map(|t| format!("{}: {}-{}", t.name, t.from, t.to))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn frames_for_tag(sprite: &Sprite, tag: Option<&str>) -> Result<Vec<u16>> {
    if let Some(name) = tag {
        let t = sprite
            .tags
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| AseErr(format!("tag '{name}' not found")))?;
        Ok((t.from..=t.to).collect())
    } else {
        Ok((0..sprite.frame_count() as u16).collect())
    }
}

pub fn export_frame_png(
    sprite: &Sprite,
    frame: u16,
    scale: u32,
) -> Result<Vec<u8>> {
    let rgba = flatten_frame_rgba(sprite, frame)?;
    let scaled = if scale <= 1 {
        rgba
    } else {
        scale_rgba(sprite.width, sprite.height, &rgba, scale)
    };
    let w = sprite.width as u32 * scale.max(1);
    let h = sprite.height as u32 * scale.max(1);
    Ok(pxl_export::encode_rgba_png(w, h, &scaled)?)
}

fn scale_rgba(w: u16, h: u16, rgba: &[u8], scale: u32) -> Vec<u8> {
    let sw = w as u32 * scale;
    let sh = h as u32 * scale;
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..h as u32 {
        for x in 0..w as u32 {
            let si = ((y * w as u32 + x) * 4) as usize;
            let px = &rgba[si..si + 4];
            for dy in 0..scale {
                for dx in 0..scale {
                    let di = (((y * scale + dy) * sw + (x * scale + dx)) * 4) as usize;
                    out[di..di + 4].copy_from_slice(px);
                }
            }
        }
    }
    out
}

pub fn sprite_palette(sprite: &Sprite) -> Palette {
    let colors = if sprite.palette.is_empty() {
        vec![Rgba::TRANSPARENT]
    } else {
        sprite.palette.clone()
    };
    Palette {
        id: "ase".into(),
        colors,
        ramps: Default::default(),
        cycles: Default::default(),
        special: Default::default(),
    }
}

pub fn export_sheet_png_json(
    sprite: &Sprite,
    frames: &[u16],
    image_name: &str,
) -> Result<(Vec<u8>, String)> {
    use pxl_export::{pack_sheet, sheet_json, SheetCell};
    let pal = if sprite.color_mode == ColorMode::Indexed && !sprite.palette.is_empty() {
        sprite_palette(sprite)
    } else {
        let mut samples = Vec::new();
        for &f in frames {
            let rgba = flatten_frame_rgba(sprite, f)?;
            samples.extend_from_slice(&rgba);
        }
        let colors = color::quantize_rgba(&samples, 16);
        Palette {
            id: "ase-sheet".into(),
            colors,
            ramps: Default::default(),
            cycles: Default::default(),
            special: Default::default(),
        }
    };

    let mut cells = Vec::new();
    let mut durations = Vec::new();
    for &f in frames {
        let rgba = flatten_frame_rgba(sprite, f)?;
        let indices = pxl_art::quantize::map_to_palette(
            sprite.width as usize,
            sprite.height as usize,
            &rgba,
            &pal,
            0,
        );
        let mut frame = pxl_core::IndexFrame::canvas(sprite.width, sprite.height)?;
        frame.pixels = indices;
        let dur = sprite
            .frame_durations_ms
            .get(f as usize)
            .copied()
            .unwrap_or(100);
        cells.push(SheetCell {
            name: format!("frame_{f}"),
            frame,
            duration_ms: dur as u32,
            tag: None,
        });
        durations.push(dur);
    }
    let packed = pack_sheet(&[cells], "rows-by-anim", 0)?;
    let png = pxl_export::encode_indexed_png(&packed.image, &pal, 1)?;
    let meta = json!({
        "frameTags": sprite.tags.iter().map(|t| json!({
            "name": t.name,
            "from": t.from,
            "to": t.to,
            "direction": "forward"
        })).collect::<Vec<_>>(),
        "durations_ms": durations,
    });
    let js = sheet_json(&packed, image_name, meta)?;
    Ok((png, js))
}
