use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};

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

/// Nearest-neighbour preview scaling over a validated RGBA buffer. Refuses a
/// buffer whose length does not match `w * h * 4` and an output over the pixel
/// budget, so neither can index out of range or exhaust memory.
pub fn scale_rgba_checked(
    w: u32,
    h: u32,
    rgba: &[u8],
    scale: u32,
    grid: bool,
) -> PxlResult<(u32, u32, Vec<u8>)> {
    let scale = scale.max(1);
    if rgba.len() as u64 != w as u64 * h as u64 * 4 {
        return Err(PxlError::semantic(format!(
            "rgba buffer has {} bytes, expected {} for {w}x{h}",
            rgba.len(),
            w as u64 * h as u64 * 4
        )));
    }
    let out_pixels = (w as u64 * scale as u64) * (h as u64 * scale as u64);
    if out_pixels > MAX_SCALED_PIXELS {
        return Err(PxlError::semantic(format!(
            "scaled preview would be {out_pixels} pixels, over the {MAX_SCALED_PIXELS} limit"
        ))
        .with_hint("use a smaller --scale"));
    }
    Ok(scale_rgba(w, h, rgba, scale, grid))
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

/// Preview scaling output cap, in pixels.
const MAX_SCALED_PIXELS: u64 = 64 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::scale_rgba_checked;

    #[test]
    fn scale_rgba_rejects_a_short_buffer() {
        assert!(scale_rgba_checked(8, 1, &[0u8; 8], 2, false).is_err());
    }
}
