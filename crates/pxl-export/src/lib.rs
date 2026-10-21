//! PNG and GIF encoders with fixed settings, plus Aseprite hash-format sheets.
//! No timestamps. Indexed color uses PLTE + tRNS. Filters and compression are pinned.

use std::io::Cursor;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};
use serde::Serialize;
use serde_json::{Map, Value};

pub fn scale_nearest(frame: &IndexFrame, scale: u32) -> PxlResult<IndexFrame> {
    let scale = scale.max(1);
    let w = frame.w as u32 * scale;
    let h = frame.h as u32 * scale;
    if w > 2048 || h > 2048 {
        return Err(
            PxlError::semantic(format!("scaled size {w}x{h} exceeds 2048"))
                .with_hint("use a smaller --scale"),
        );
    }
    let mut out = IndexFrame::canvas(w as u16, h as u16)?;
    for y in 0..frame.h as u32 {
        for x in 0..frame.w as u32 {
            let c = frame.get(x as i32, y as i32);
            for dy in 0..scale {
                for dx in 0..scale {
                    out.set((x * scale + dx) as i32, (y * scale + dy) as i32, c);
                }
            }
        }
    }
    Ok(out)
}

pub fn encode_indexed_png(frame: &IndexFrame, palette: &Palette, scale: u32) -> PxlResult<Vec<u8>> {
    let image = if scale <= 1 {
        frame.clone()
    } else {
        scale_nearest(frame, scale)?
    };
    encode_indexed_bytes(image.w as u32, image.h as u32, &image.pixels, palette)
}

fn encode_indexed_bytes(w: u32, h: u32, pixels: &[u8], palette: &Palette) -> PxlResult<Vec<u8>> {
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);
    let mut pal = Vec::with_capacity(palette.colors.len() * 3);
    let mut trns = Vec::with_capacity(palette.colors.len());
    let colors: Vec<Rgba> = if palette.colors.is_empty() {
        vec![Rgba::TRANSPARENT]
    } else {
        palette.colors.clone()
    };
    for c in &colors {
        pal.extend_from_slice(&[c.r, c.g, c.b]);
        trns.push(if c.a == 0 { 0 } else { 255 });
    }
    encoder.set_palette(pal);
    encoder.set_trns(trns);
    let mut writer = encoder
        .write_header()
        .map_err(|e| PxlError::io(format!("png header: {e}")))?;
    writer
        .write_image_data(pixels)
        .map_err(|e| PxlError::io(format!("png data: {e}")))?;
    drop(writer);
    Ok(buf)
}

pub fn encode_rgba_png(w: u32, h: u32, rgba: &[u8]) -> PxlResult<Vec<u8>> {
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return Err(PxlError::semantic("rgba buffer has the wrong length"));
    }
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::NoFilter);
    let mut writer = encoder
        .write_header()
        .map_err(|e| PxlError::io(format!("png header: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| PxlError::io(format!("png data: {e}")))?;
    drop(writer);
    Ok(buf)
}

/// Preview composite. Transparent pixels become a flat dark backdrop so the
/// PNG is readable, and `--grid` draws a 1px lattice on the scaled cells.
pub fn render_preview_png(
    frame: &IndexFrame,
    palette: &Palette,
    scale: u32,
    grid: bool,
) -> PxlResult<Vec<u8>> {
    let (w, h, rgba) = composite(frame, palette, scale.max(1), grid);
    encode_rgba_png(w, h, &rgba)
}

pub fn composite(
    frame: &IndexFrame,
    palette: &Palette,
    scale: u32,
    grid: bool,
) -> (u32, u32, Vec<u8>) {
    let scale = scale.max(1);
    let w = frame.w as u32 * scale;
    let h = frame.h as u32 * scale;
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..frame.h as u32 {
        for x in 0..frame.w as u32 {
            let idx = frame.get(x as i32, y as i32);
            let col = palette
                .colors
                .get(idx as usize)
                .copied()
                .unwrap_or(Rgba::TRANSPARENT);
            for dy in 0..scale {
                for dx in 0..scale {
                    let lattice = grid && scale >= 2 && (dx + 1 == scale || dy + 1 == scale);
                    let (r, g, b, a) = if lattice {
                        (20u8, 18, 16, 255)
                    } else if col.a == 0 {
                        (28, 26, 24, 255)
                    } else {
                        (col.r, col.g, col.b, 255)
                    };
                    let i = (((y * scale + dy) * w + (x * scale + dx)) * 4) as usize;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = a;
                }
            }
        }
    }
    (w, h, rgba)
}

pub fn scale_rgba(w: u32, h: u32, rgba: &[u8], scale: u32, grid: bool) -> (u32, u32, Vec<u8>) {
    let scale = scale.max(1);
    let ow = w * scale;
    let oh = h * scale;
    let mut out = vec![0u8; (ow as usize) * (oh as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let s = ((y * w + x) * 4) as usize;
            let px = [rgba[s], rgba[s + 1], rgba[s + 2], rgba[s + 3]];
            for dy in 0..scale {
                for dx in 0..scale {
                    let lattice = grid && scale >= 2 && (dx + 1 == scale || dy + 1 == scale);
                    let i = (((y * scale + dy) * ow + (x * scale + dx)) * 4) as usize;
                    if lattice {
                        out[i] = 20;
                        out[i + 1] = 18;
                        out[i + 2] = 16;
                        out[i + 3] = 255;
                    } else if px[3] == 0 {
                        out[i] = 28;
                        out[i + 1] = 26;
                        out[i + 2] = 24;
                        out[i + 3] = 255;
                    } else {
                        out[i..i + 4].copy_from_slice(&px);
                        out[i + 3] = 255;
                    }
                }
            }
        }
    }
    (ow, oh, out)
}

pub fn decode_png_rgba(bytes: &[u8]) -> PxlResult<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| PxlError::io(format!("png read: {e}")))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| PxlError::io(format!("png frame: {e}")))?;
    let w = info.width;
    let h = info.height;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::Indexed => {
            let palette = reader.info().palette.clone().unwrap_or_default();
            let trns = reader.info().trns.clone().unwrap_or_default();
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for idx in buf {
                let i = idx as usize * 3;
                let (r, g, b) = if i + 2 < palette.len() {
                    (palette[i], palette[i + 1], palette[i + 2])
                } else {
                    (0, 0, 0)
                };
                let a = trns.get(idx as usize).copied().unwrap_or(255);
                out.extend_from_slice(&[r, g, b, a]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::new();
            for g in buf {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        other => {
            return Err(
                PxlError::semantic(format!("unsupported png color type {other:?}"))
                    .with_hint("pxl writes indexed or rgba pngs"),
            );
        }
    };
    Ok((w, h, rgba))
}

/// Indexed pixels plus the palette stored in the file.
pub fn decode_indexed(bytes: &[u8]) -> PxlResult<(u32, u32, Vec<u8>, Vec<Rgba>)> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| PxlError::io(format!("png read: {e}")))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| PxlError::io(format!("png frame: {e}")))?;
    if info.color_type != png::ColorType::Indexed {
        return Err(PxlError::semantic("sheet png is not indexed")
            .with_hint("pxl sheet writes an indexed png; re-export with pxl sheet"));
    }
    buf.truncate(info.buffer_size());
    let pal = reader.info().palette.clone().unwrap_or_default();
    let trns = reader.info().trns.clone().unwrap_or_default();
    let mut colors = Vec::new();
    for (i, rgb) in pal.chunks_exact(3).enumerate() {
        let a = trns.get(i).copied().unwrap_or(255);
        colors.push(Rgba {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
            a,
        });
    }
    Ok((info.width, info.height, buf, colors))
}

/// Looping GIF with one fixed delay (centiseconds) for every frame.
pub fn encode_gif(
    frames: &[IndexFrame],
    palette: &Palette,
    scale: u32,
    delay_cs: u16,
) -> PxlResult<Vec<u8>> {
    let delays = vec![delay_cs; frames.len()];
    encode_gif_timed(frames, palette, scale, &delays, true)
}

/// Per-frame delays in centiseconds that add up to the nearest centisecond of
/// `n / fps` seconds. GIF cannot store 1/fps exactly, so the rounding error is
/// spread over the frames instead of drifting by `100 / fps` truncation.
pub fn gif_delays_cs(fps: u32, n: usize) -> Vec<u16> {
    let fps = fps.max(1) as u64;
    let edge = |i: u64| (i * 100 + fps / 2) / fps;
    (0..n as u64)
        .map(|i| (edge(i + 1) - edge(i)).clamp(1, u16::MAX as u64) as u16)
        .collect()
}

/// GIF with a delay per frame. `looped = false` omits the NETSCAPE loop block,
/// which players treat as "play once".
pub fn encode_gif_timed(
    frames: &[IndexFrame],
    palette: &Palette,
    scale: u32,
    delays_cs: &[u16],
    looped: bool,
) -> PxlResult<Vec<u8>> {
    if frames.is_empty() {
        return Err(PxlError::semantic("gif has no frames"));
    }
    if delays_cs.len() != frames.len() {
        return Err(PxlError::semantic(format!(
            "gif has {} frames but {} delays",
            frames.len(),
            delays_cs.len()
        )));
    }
    let first = scale_nearest(&frames[0], scale.max(1))?;
    let w = first.w;
    let h = first.h;
    for frame in frames {
        if frame.w != frames[0].w || frame.h != frames[0].h {
            return Err(PxlError::semantic("gif frames must share a size"));
        }
    }
    let global = gif_palette(&palette.colors);
    let mut buf = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut buf, w, h, &global)
            .map_err(|e| PxlError::io(format!("gif header: {e}")))?;
        if looped {
            encoder
                .set_repeat(gif::Repeat::Infinite)
                .map_err(|e| PxlError::io(format!("gif repeat: {e}")))?;
        }
        for (frame, delay) in frames.iter().zip(delays_cs) {
            let image = scale_nearest(frame, scale.max(1))?;
            let mut gf = gif::Frame::from_indexed_pixels(w, h, image.pixels.clone(), Some(0));
            gf.delay = (*delay).max(1);
            gf.dispose = gif::DisposalMethod::Background;
            encoder
                .write_frame(&gf)
                .map_err(|e| PxlError::io(format!("gif frame: {e}")))?;
        }
    }
    Ok(buf)
}

fn gif_palette(colors: &[Rgba]) -> Vec<u8> {
    let mut n = colors.len().max(2);
    n = n.next_power_of_two().clamp(2, 256);
    let mut out = vec![0u8; n * 3];
    for (i, c) in colors.iter().take(n).enumerate() {
        out[i * 3] = c.r;
        out[i * 3 + 1] = c.g;
        out[i * 3 + 2] = c.b;
    }
    out
}

#[derive(Clone, Debug)]
pub struct SheetCell {
    pub name: String,
    pub frame: IndexFrame,
    pub duration_ms: u32,
    pub tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
    pub tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PackedSheet {
    pub image: IndexFrame,
    pub placements: Vec<Placement>,
}

/// `rows-by-anim` keeps caller row grouping. `grid` packs in a roughly square grid.
pub fn pack_sheet(rows: &[Vec<SheetCell>], layout: &str, pad: u32) -> PxlResult<PackedSheet> {
    let flat: Vec<&SheetCell> = rows.iter().flatten().collect();
    if flat.is_empty() {
        return Err(PxlError::semantic("sheet has no frames"));
    }
    let groups: Vec<Vec<&SheetCell>> = match layout {
        "rows-by-anim" | "rows" => rows
            .iter()
            .map(|r| r.iter().collect())
            .filter(|r: &Vec<_>| !r.is_empty())
            .collect(),
        "grid" => {
            let n = flat.len();
            let mut cols = 1usize;
            while cols * cols < n {
                cols += 1;
            }
            flat.chunks(cols).map(|c| c.to_vec()).collect()
        }
        other => {
            return Err(PxlError::usage(format!("unknown sheet layout '{other}'"))
                .with_hint("layout is rows-by-anim or grid"));
        }
    };
    let pad = pad as i32;
    let mut placements = Vec::new();
    let mut y = 0i32;
    let mut sheet_w = 1i32;
    for row in &groups {
        let mut x = 0i32;
        let mut row_h = 1i32;
        for cell in row {
            let cw = cell.frame.w as i32;
            let ch = cell.frame.h as i32;
            placements.push((x, y, cell));
            row_h = row_h.max(ch);
            x += cw + pad;
        }
        if x > 0 {
            sheet_w = sheet_w.max(x - pad);
        }
        y += row_h + pad;
    }
    let sheet_h = if y > 0 { y - pad } else { 1 };
    let mut image = IndexFrame::canvas(sheet_w.max(1) as u16, sheet_h.max(1) as u16)?;
    let mut out_places = Vec::new();
    for (x, y, cell) in placements {
        for py in 0..cell.frame.h as i32 {
            for px in 0..cell.frame.w as i32 {
                image.set(x + px, y + py, cell.frame.get(px, py));
            }
        }
        out_places.push(Placement {
            name: cell.name.clone(),
            x: x as u32,
            y: y as u32,
            w: cell.frame.w as u32,
            h: cell.frame.h as u32,
            duration_ms: cell.duration_ms,
            tag: cell.tag.clone(),
        });
    }
    Ok(PackedSheet {
        image,
        placements: out_places,
    })
}

#[derive(Serialize)]
struct AseRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct AseFrame {
    frame: AseRect,
    rotated: bool,
    trimmed: bool,
    #[serde(rename = "spriteSourceSize")]
    sprite_source_size: AseRect,
    #[serde(rename = "sourceSize")]
    source_size: Size,
    duration: u32,
}

#[derive(Serialize)]
struct Size {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct FrameTag {
    name: String,
    from: u32,
    to: u32,
    direction: String,
}

pub fn sheet_json(packed: &PackedSheet, image_name: &str, pxl: Value) -> PxlResult<String> {
    let mut frames = Map::new();
    let mut tags: Vec<FrameTag> = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for (i, p) in packed.placements.iter().enumerate() {
        let entry = AseFrame {
            frame: AseRect {
                x: p.x,
                y: p.y,
                w: p.w,
                h: p.h,
            },
            rotated: false,
            trimmed: false,
            sprite_source_size: AseRect {
                x: 0,
                y: 0,
                w: p.w,
                h: p.h,
            },
            source_size: Size { w: p.w, h: p.h },
            duration: p.duration_ms,
        };
        frames.insert(
            p.name.clone(),
            serde_json::to_value(entry).map_err(|e| PxlError::io(e.to_string()))?,
        );
        match (&current, &p.tag) {
            (None, Some(tag)) => current = Some((tag.clone(), i as u32)),
            (Some((name, _from)), Some(tag)) if name == tag => {}
            (Some((name, from)), _) => {
                tags.push(FrameTag {
                    name: name.clone(),
                    from: *from,
                    to: i as u32 - 1,
                    direction: "forward".into(),
                });
                current = p.tag.clone().map(|t| (t, i as u32));
            }
            (None, None) => {}
        }
    }
    if let Some((name, from)) = current {
        tags.push(FrameTag {
            name,
            from,
            to: packed.placements.len() as u32 - 1,
            direction: "forward".into(),
        });
    }
    let meta = serde_json::json!({
        "app": "pxl",
        "version": pxl_core::PXL_VERSION,
        "image": image_name,
        "format": "indexed8",
        "size": { "w": packed.image.w, "h": packed.image.h },
        "scale": "1",
        "frameTags": tags,
        "pxl": pxl,
    });
    let doc = serde_json::json!({
        "frames": frames,
        "meta": meta,
    });
    let mut s = serde_json::to_string_pretty(&doc).map_err(|e| PxlError::io(e.to_string()))?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_core::Pcg32;

    #[test]
    fn png_roundtrip_is_stable() {
        let pal = palette();
        let mut frame = IndexFrame::new(4, 2).unwrap();
        frame.set(1, 0, 3);
        frame.set(2, 1, 6);
        let a = encode_indexed_png(&frame, &pal, 1).unwrap();
        let b = encode_indexed_png(&frame, &pal, 1).unwrap();
        assert_eq!(a, b);
        let (w, h, px, colors) = decode_indexed(&a).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(px[1], 3);
        assert_eq!(colors[0].a, 0);
        assert!(colors[3].a == 255);
        let _ = Pcg32::from_seed(1);
    }

    /// Read back (repeat, per-frame delay in cs) from encoded GIF bytes.
    fn gif_timing(bytes: &[u8]) -> (gif::Repeat, Vec<u16>) {
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::Indexed);
        let mut dec = opts.read_info(bytes).unwrap();
        let mut delays = Vec::new();
        while let Some(frame) = dec.read_next_frame().unwrap() {
            delays.push(frame.delay);
        }
        (dec.repeat(), delays)
    }

    fn two_frames() -> Vec<IndexFrame> {
        let mut a = IndexFrame::new(3, 3).unwrap();
        a.set(1, 1, 3);
        let mut b = IndexFrame::new(3, 3).unwrap();
        b.set(2, 2, 4);
        vec![a, b]
    }

    #[test]
    fn one_shot_gif_does_not_loop_and_looping_gif_does() {
        let pal = palette();
        let frames = two_frames();
        let once = encode_gif_timed(&frames, &pal, 2, &[10, 10], false).unwrap();
        let (repeat, _) = gif_timing(&once);
        assert_eq!(
            repeat,
            gif::Repeat::Finite(0),
            "no loop block means play once"
        );
        let forever = encode_gif_timed(&frames, &pal, 2, &[10, 10], true).unwrap();
        assert_eq!(gif_timing(&forever).0, gif::Repeat::Infinite);
        // The fixed-delay entry point still loops, as before.
        let legacy = encode_gif(&frames, &pal, 2, 7).unwrap();
        let (repeat, delays) = gif_timing(&legacy);
        assert_eq!(repeat, gif::Repeat::Infinite);
        assert_eq!(delays, vec![7, 7]);
    }

    #[test]
    fn gif_delays_follow_the_requested_fps_without_drift() {
        // 100 / 12 truncates to 8cs (12.5 fps). The spread delays must total 100cs.
        for fps in [1u32, 4, 8, 12, 24, 30, 60, 100] {
            let delays = gif_delays_cs(fps, fps as usize);
            let total: u32 = delays.iter().map(|d| *d as u32).sum();
            assert_eq!(total, 100, "fps {fps}: {delays:?}");
            assert!(delays.iter().all(|d| *d >= 1));
        }
        assert_eq!(gif_delays_cs(12, 3), vec![8, 9, 8]);
    }

    #[test]
    fn gif_timed_rejects_a_delay_count_mismatch() {
        let pal = palette();
        let err = encode_gif_timed(&two_frames(), &pal, 1, &[10], true).unwrap_err();
        assert!(err.message.contains("delays"), "{err:?}");
    }

    fn palette() -> Palette {
        // Local palette so export tests do not depend on pxl-gen.
        Palette {
            id: "t".into(),
            colors: vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(20, 16, 12),
                Rgba::rgb(59, 74, 42),
                Rgba::rgb(90, 110, 58),
                Rgba::rgb(134, 160, 82),
                Rgba::rgb(185, 201, 138),
                Rgba::rgb(74, 64, 56),
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special {
                transparent: 0,
                outline: 1,
                flash: 5,
            },
        }
    }
}
