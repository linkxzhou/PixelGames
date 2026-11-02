//! Spritesheet / GIF export and layer visibility helpers.
use pxl_ase_doc::{ColorMode, Sprite};
use pxl_core::{IndexFrame, Palette};
use serde_json::{json, Value};

use crate::color;
use crate::export_png::sprite_palette;
use crate::render::flatten_frame_rgba;
use crate::{slice_json, tag_direction_name, AseErr, Result};

fn sheet_layout_name(layout: &str) -> Result<&'static str> {
    match layout.to_ascii_lowercase().as_str() {
        "rows" | "rows-by-anim" | "horizontal" | "h" => Ok("rows-by-anim"),
        "grid" | "packed" | "vertical" | "v" | "columns" => Ok("grid"),
        other => Err(AseErr(format!(
            "unknown sheet layout '{other}' (horizontal|vertical|packed|rows|grid)"
        ))),
    }
}

#[derive(Debug, Clone, Default)]
pub struct SheetExportOpts {
    pub padding: u32,
    pub extrude: u32,
    pub trim: bool,
    pub merge_duplicates: bool,
}

/// Drop consecutive frames whose flattened RGBA bytes are identical.
pub fn merge_duplicate_frames(sprite: &Sprite, frames: &[u16]) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    let mut prev: Option<Vec<u8>> = None;
    for &f in frames {
        let rgba = flatten_frame_rgba(sprite, f)?;
        if prev.as_ref() == Some(&rgba) {
            continue;
        }
        prev = Some(rgba);
        out.push(f);
    }
    Ok(out)
}

fn trim_index_frame(frame: &IndexFrame) -> Result<IndexFrame> {
    let Some(bb) = frame.bbox() else {
        return Ok(frame.clone());
    };
    let w = bb.w.max(1) as u16;
    let h = bb.h.max(1) as u16;
    let mut out = IndexFrame::canvas(w, h)?;
    for y in 0..bb.h {
        for x in 0..bb.w {
            out.set(x, y, frame.get(bb.x + x, bb.y + y));
        }
    }
    Ok(out)
}

fn extrude_index_frame(frame: &IndexFrame, n: u32) -> Result<IndexFrame> {
    if n == 0 {
        return Ok(frame.clone());
    }
    let n = n as i32;
    let w = frame.w as i32 + 2 * n;
    let h = frame.h as i32 + 2 * n;
    let mut out = IndexFrame::canvas(w as u16, h as u16)?;
    for y in 0..h {
        for x in 0..w {
            let sx = (x - n).clamp(0, frame.w as i32 - 1);
            let sy = (y - n).clamp(0, frame.h as i32 - 1);
            out.set(x, y, frame.get(sx, sy));
        }
    }
    Ok(out)
}

pub fn export_sheet_png_json(
    sprite: &Sprite,
    frames: &[u16],
    image_name: &str,
) -> Result<(Vec<u8>, String)> {
    export_sheet_png_json_layout(sprite, frames, image_name, "rows-by-anim")
}

pub fn export_sheet_png_json_layout(
    sprite: &Sprite,
    frames: &[u16],
    image_name: &str,
    layout: &str,
) -> Result<(Vec<u8>, String)> {
    export_sheet_png_json_layout_opts(sprite, frames, image_name, layout, &SheetExportOpts::default())
}

pub fn export_sheet_png_json_layout_opts(
    sprite: &Sprite,
    frames: &[u16],
    image_name: &str,
    layout: &str,
    opts: &SheetExportOpts,
) -> Result<(Vec<u8>, String)> {
    use pxl_export::{pack_sheet, sheet_json, SheetCell};
    let layout = sheet_layout_name(layout)?;
    let mut frames = frames.to_vec();
    if opts.merge_duplicates {
        frames = merge_duplicate_frames(sprite, &frames)?;
    }
    let pal = if sprite.color_mode == ColorMode::Indexed && !sprite.palette.is_empty() {
        sprite_palette(sprite)
    } else {
        let mut samples = Vec::new();
        for &f in &frames {
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
    for &f in &frames {
        let rgba = flatten_frame_rgba(sprite, f)?;
        let indices = pxl_art::quantize::map_to_palette(
            sprite.width as usize,
            sprite.height as usize,
            &rgba,
            &pal,
            0,
        );
        let mut frame = IndexFrame::canvas(sprite.width, sprite.height)?;
        frame.pixels = indices;
        if opts.trim {
            frame = trim_index_frame(&frame)?;
        }
        if opts.extrude > 0 {
            frame = extrude_index_frame(&frame, opts.extrude)?;
        }
        let dur = sprite
            .frame_durations_ms
            .get(f as usize)
            .copied()
            .unwrap_or(100);
        let tag_name = sprite
            .tags
            .iter()
            .find(|t| f >= t.from && f <= t.to)
            .map(|t| t.name.clone());
        cells.push(SheetCell {
            name: format!("frame_{f}"),
            frame,
            duration_ms: dur as u32,
            tag: tag_name,
        });
        durations.push(dur);
    }
    let packed = pack_sheet(&[cells], layout, opts.padding)?;
    let png = pxl_export::encode_indexed_png(&packed.image, &pal, 1)?;
    let meta = json!({
        "frameTags": sprite.tags.iter().map(|t| json!({
            "name": t.name,
            "from": t.from,
            "to": t.to,
            "direction": tag_direction_name(t.direction),
        })).collect::<Vec<_>>(),
        "meta": {
            "slices": sprite.slices.iter().map(slice_json).collect::<Vec<_>>(),
            "layers": sprite.layers.iter().map(|l| json!({
                "name": l.name,
                "visible": l.flags.visible,
            })).collect::<Vec<_>>(),
        },
        "durations_ms": durations,
        "layout": layout,
    });
    let mut js = sheet_json(&packed, image_name, meta)?;
    // Prefer document tag directions over packer defaults ("forward").
    if let Ok(mut v) = serde_json::from_str::<Value>(&js) {
        if let Some(meta_obj) = v.get_mut("meta").and_then(|m| m.as_object_mut()) {
            meta_obj.insert(
                "frameTags".into(),
                json!(sprite.tags.iter().map(|t| json!({
                    "name": t.name,
                    "from": t.from,
                    "to": t.to,
                    "direction": tag_direction_name(t.direction),
                })).collect::<Vec<_>>()),
            );
        }
        js = serde_json::to_string_pretty(&v).unwrap_or(js);
        if !js.ends_with('\n') {
            js.push('\n');
        }
    }
    Ok((png, js))
}

/// Flatten selected frames to an animated GIF (indexed via current/quantized palette).
pub fn export_gif(sprite: &Sprite, frames: &[u16], scale: u32) -> Result<Vec<u8>> {
    let pal = if sprite.color_mode == ColorMode::Indexed && !sprite.palette.is_empty() {
        sprite_palette(sprite)
    } else {
        let mut samples = Vec::new();
        for &f in frames {
            samples.extend_from_slice(&flatten_frame_rgba(sprite, f)?);
        }
        let colors = color::quantize_rgba(&samples, 32);
        Palette {
            id: "ase-gif".into(),
            colors,
            ramps: Default::default(),
            cycles: Default::default(),
            special: Default::default(),
        }
    };
    let mut index_frames = Vec::new();
    let mut delays_cs = Vec::new();
    for &f in frames {
        let rgba = flatten_frame_rgba(sprite, f)?;
        let indices = pxl_art::quantize::map_to_palette(
            sprite.width as usize,
            sprite.height as usize,
            &rgba,
            &pal,
            0,
        );
        let mut frame = IndexFrame::canvas(sprite.width, sprite.height)?;
        frame.pixels = indices;
        index_frames.push(frame);
        let ms = sprite
            .frame_durations_ms
            .get(f as usize)
            .copied()
            .unwrap_or(100) as u32;
        // GIF delay unit = centiseconds.
        delays_cs.push(((ms + 5) / 10).clamp(1, u16::MAX as u32) as u16);
    }
    Ok(pxl_export::encode_gif_timed(
        &index_frames,
        &pal,
        scale.max(1),
        &delays_cs,
        true,
    )?)
}

/// Force layers visible/hidden for flatten (--all-layers includes hidden).
pub fn set_all_layers_visible(sprite: &mut Sprite, visible: bool) {
    for layer in &mut sprite.layers {
        layer.flags.visible = visible;
    }
}

pub fn set_layer_visibility_by_name(sprite: &mut Sprite, name: &str, visible: bool) -> Result<()> {
    let mut found = false;
    for layer in &mut sprite.layers {
        if layer.name == name {
            layer.flags.visible = visible;
            found = true;
        }
    }
    if !found {
        return Err(AseErr(format!("layer '{name}' not found")));
    }
    Ok(())
}

