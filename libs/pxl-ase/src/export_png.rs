//! Per-frame / slice PNG export and filename-format expansion.
use std::path::Path;

use pxl_ase_doc::{Slice, Sprite};
use pxl_core::{Palette, Rgba};

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

#[derive(Debug, Clone, Default)]
pub struct FilenameFormatCtx<'a> {
    pub frame: u16,
    pub tag: &'a str,
    pub layer: &'a str,
    pub title: &'a str,
    pub path: &'a str,
    pub slice: &'a str,
    pub duration_ms: u32,
    pub tagframe: u16,
    pub innertagframe: u16,
    pub startframe: u16,
    pub endframe: u16,
}

/// Expand filename-format placeholders toward the Aseprite public list.
/// Supported: `{frame}` `{frame1}` `{frame01}` `{frame001}` `{frame0001}`,
/// `{tag}` `{innertag}` `{layer}` `{title}` `{path}` `{fullname}` `{name}`,
/// `{slice}` `{duration}` `{tagframe}` `{innertagframe}` `{startframe}` `{endframe}`.
pub fn expand_filename_format(
    fmt: &str,
    frame: u16,
    tag: &str,
    layer: &str,
    title: &str,
    path: &str,
) -> String {
    expand_filename_format_ctx(
        fmt,
        &FilenameFormatCtx {
            frame,
            tag,
            layer,
            title,
            path,
            ..Default::default()
        },
    )
}

pub fn expand_filename_format_ctx(fmt: &str, cx: &FilenameFormatCtx<'_>) -> String {
    let fullname = if cx.path.is_empty() {
        cx.title.to_string()
    } else {
        cx.path.to_string()
    };
    let name = Path::new(cx.title)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(cx.title);
    let innertag = if cx.tag.is_empty() { "" } else { cx.tag };
    fmt.replace("{frame0001}", &format!("{:04}", cx.frame))
        .replace("{frame001}", &format!("{:03}", cx.frame))
        .replace("{frame01}", &format!("{:02}", cx.frame))
        .replace("{frame1}", &(cx.frame + 1).to_string())
        .replace("{frame}", &cx.frame.to_string())
        .replace("{tagframe}", &cx.tagframe.to_string())
        .replace("{innertagframe}", &cx.innertagframe.to_string())
        .replace("{startframe}", &cx.startframe.to_string())
        .replace("{endframe}", &cx.endframe.to_string())
        .replace("{duration}", &cx.duration_ms.to_string())
        .replace("{innertag}", innertag)
        .replace("{tag}", cx.tag)
        .replace("{layer}", cx.layer)
        .replace("{slice}", cx.slice)
        .replace("{title}", cx.title)
        .replace("{name}", name)
        .replace("{path}", cx.path)
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

/// Split the sprite canvas by `sprite.grid` cell size into PNGs under `out_dir`.
pub fn split_grid_to_dir(
    sprite: &Sprite,
    out_dir: &std::path::Path,
    frame: u16,
    scale: u32,
) -> Result<Vec<String>> {
    let gw = sprite.grid.w as i32;
    let gh = sprite.grid.h as i32;
    if gw <= 0 || gh <= 0 {
        return Err(AseErr(
            "--split-grid needs sprite grid w/h > 0 (set in .aseprite)".into(),
        ));
    }
    let ox = sprite.grid.x as i32;
    let oy = sprite.grid.y as i32;
    let sw = sprite.width as i32;
    let sh = sprite.height as i32;
    std::fs::create_dir_all(out_dir).map_err(|e| AseErr(e.to_string()))?;
    let rgba = flatten_frame_rgba(sprite, frame)?;
    let mut written = Vec::new();
    let mut row = 0i32;
    let mut y = oy;
    while y < sh {
        let mut col = 0i32;
        let mut x = ox;
        while x < sw {
            let cw = gw.min(sw - x).max(0) as u16;
            let ch = gh.min(sh - y).max(0) as u16;
            if cw == 0 || ch == 0 {
                break;
            }
            let mut crop = vec![0u8; (cw as usize) * (ch as usize) * 4];
            for dy in 0..ch as i32 {
                for dx in 0..cw as i32 {
                    let sx = x + dx;
                    let sy = y + dy;
                    if sx < 0 || sy < 0 || sx >= sw || sy >= sh {
                        continue;
                    }
                    let si = ((sy * sw + sx) * 4) as usize;
                    let di = ((dy * cw as i32 + dx) * 4) as usize;
                    crop[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
                }
            }
            let scale = scale.max(1);
            let scaled = if scale == 1 {
                crop
            } else {
                scale_rgba(cw, ch, &crop, scale)
            };
            let png = pxl_export::encode_rgba_png(cw as u32 * scale, ch as u32 * scale, &scaled)
                .map_err(|e| AseErr(e.message.clone()))?;
            let path = out_dir.join(format!("grid_{}_{}.png", row, col));
            std::fs::write(&path, png).map_err(|e| AseErr(e.to_string()))?;
            written.push(path.display().to_string());
            col += 1;
            x += gw;
        }
        row += 1;
        y += gh;
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
