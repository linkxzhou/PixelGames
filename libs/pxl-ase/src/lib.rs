//! Headless Aseprite-aligned batch helpers. Modules mirror planned crates
//! `pxl-ase-color` / `filters` / `ops` / `render` (merged for delivery speed).

pub mod color;
pub mod filters;
pub mod ops;
pub mod render;

pub use pxl_ase_doc as doc;
pub use pxl_ase_io as io;

use pxl_ase_doc::{AseDoc, ColorMode, Slice, Sprite, Tag, TagDirection};
use pxl_ase_io::{read_aseprite, ReadOptions};
use pxl_core::{IndexFrame, Palette, Rgba};
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

fn tag_direction_name(d: TagDirection) -> &'static str {
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

fn slice_json(sl: &Slice) -> Value {
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

/// Parse Aseprite-like frame ranges: `0`, `0,2`, `1-3`, `0,2-4`.
pub fn parse_frame_range(spec: &str, frame_count: u16) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let from: u16 = a
                .trim()
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            let to: u16 = b
                .trim()
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            if from > to {
                return Err(AseErr(format!("frame-range from>to in '{part}'")));
            }
            for f in from..=to {
                if f < frame_count {
                    out.push(f);
                }
            }
        } else {
            let f: u16 = part
                .parse()
                .map_err(|_| AseErr(format!("bad frame-range '{spec}'")))?;
            if f < frame_count {
                out.push(f);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err(AseErr(format!("frame-range '{spec}' selected no frames")));
    }
    Ok(out)
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

pub fn select_frames(
    sprite: &Sprite,
    tag: Option<&str>,
    frame_range: Option<&str>,
    single_frame: Option<u16>,
) -> Result<Vec<u16>> {
    if let Some(spec) = frame_range {
        return parse_frame_range(spec, sprite.frame_count() as u16);
    }
    if tag.is_some() {
        return frames_for_tag(sprite, tag);
    }
    if let Some(f) = single_frame {
        if f as usize >= sprite.frame_count() {
            return Err(AseErr(format!("frame {f} out of range")));
        }
        return Ok(vec![f]);
    }
    frames_for_tag(sprite, None)
}

/// True if every pixel is fully transparent (alpha < 8).
pub fn frame_is_empty(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).all(|c| c[3] < 8)
}

/// Expand a small filename-format subset: `{frame}`, `{tag}`, `{layer}`, `{title}`, `{path}`, `{fullname}`.
pub fn expand_filename_format(
    fmt: &str,
    frame: u16,
    tag: &str,
    layer: &str,
    title: &str,
    path: &str,
) -> String {
    let fullname = if path.is_empty() {
        title.to_string()
    } else {
        path.to_string()
    };
    fmt.replace("{frame}", &frame.to_string())
        .replace("{tag}", tag)
        .replace("{layer}", layer)
        .replace("{title}", title)
        .replace("{path}", path)
        .replace("{fullname}", &fullname)
}

pub fn export_frame_png(sprite: &Sprite, frame: u16, scale: u32) -> Result<Vec<u8>> {
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

/// Crop flattened frame RGBA to slice bounds and encode PNG.
pub fn export_slice_png(sprite: &Sprite, frame: u16, slice: &Slice, scale: u32) -> Result<Vec<u8>> {
    let key = slice
        .keys
        .iter()
        .rev()
        .find(|k| k.frame <= frame as u32)
        .or_else(|| slice.keys.first())
        .ok_or_else(|| AseErr(format!("slice '{}' has no keys", slice.name)))?;
    let rgba = flatten_frame_rgba(sprite, frame)?;
    let sw = sprite.width as i32;
    let sh = sprite.height as i32;
    let x0 = key.x.max(0).min(sw);
    let y0 = key.y.max(0).min(sh);
    let x1 = (key.x + key.w as i32).max(0).min(sw);
    let y1 = (key.y + key.h as i32).max(0).min(sh);
    let cw = (x1 - x0).max(0) as usize;
    let ch = (y1 - y0).max(0) as usize;
    if cw == 0 || ch == 0 {
        return Err(AseErr(format!("slice '{}' crop is empty", slice.name)));
    }
    let mut crop = vec![0u8; cw * ch * 4];
    for row in 0..ch {
        let src_y = (y0 as usize + row) * sprite.width as usize * 4;
        let src_x = x0 as usize * 4;
        let dst = row * cw * 4;
        crop[dst..dst + cw * 4]
            .copy_from_slice(&rgba[src_y + src_x..src_y + src_x + cw * 4]);
    }
    let scale = scale.max(1);
    let scaled = if scale == 1 {
        crop
    } else {
        scale_rgba(cw as u16, ch as u16, &crop, scale)
    };
    Ok(pxl_export::encode_rgba_png(
        cw as u32 * scale,
        ch as u32 * scale,
        &scaled,
    )?)
}

/// Write one PNG per slice (using frame 0 or first key frame). Returns written paths.
pub fn split_slices_to_dir(
    sprite: &Sprite,
    out_dir: &std::path::Path,
    frame: u16,
    scale: u32,
) -> Result<Vec<String>> {
    std::fs::create_dir_all(out_dir).map_err(|e| AseErr(e.to_string()))?;
    let mut written = Vec::new();
    for slice in &sprite.slices {
        let png = export_slice_png(sprite, frame, slice, scale)?;
        let safe = slice
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        let name = if safe.is_empty() { "slice".into() } else { safe };
        let path = out_dir.join(format!("{name}.png"));
        std::fs::write(&path, png).map_err(|e| AseErr(e.to_string()))?;
        written.push(path.display().to_string());
    }
    Ok(written)
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

fn sheet_layout_name(layout: &str) -> Result<&'static str> {
    match layout.to_ascii_lowercase().as_str() {
        "rows" | "rows-by-anim" | "horizontal" | "h" => Ok("rows-by-anim"),
        "grid" | "packed" | "vertical" | "v" | "columns" => Ok("grid"),
        other => Err(AseErr(format!(
            "unknown sheet layout '{other}' (horizontal|vertical|packed|rows|grid)"
        ))),
    }
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
    use pxl_export::{pack_sheet, sheet_json, SheetCell};
    let layout = sheet_layout_name(layout)?;
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
        let mut frame = IndexFrame::canvas(sprite.width, sprite.height)?;
        frame.pixels = indices;
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
    let packed = pack_sheet(&[cells], layout, 0)?;
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
        let _ = s;
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
}
