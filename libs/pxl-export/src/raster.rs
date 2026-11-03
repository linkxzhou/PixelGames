use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};

/// Output size of a `w x h` image copied up by an integer `scale`, computed in
/// 64-bit so a hostile `scale` cannot wrap. Refuses a result over
/// [`MAX_SCALED_PIXELS`] or a side that no longer fits a `u32`.
pub fn scaled_dims(w: u32, h: u32, scale: u32) -> PxlResult<(u32, u32)> {
    let scale = scale.max(1);
    let ow = w as u64 * scale as u64;
    let oh = h as u64 * scale as u64;
    if ow > u32::MAX as u64 || oh > u32::MAX as u64 || ow * oh > MAX_SCALED_PIXELS {
        return Err(PxlError::semantic(format!(
            "scaled output {ow}x{oh} is over the {MAX_SCALED_PIXELS} pixel limit"
        ))
        .with_hint("use a smaller --scale"));
    }
    Ok((ow as u32, oh as u32))
}

pub fn scale_nearest(frame: &IndexFrame, scale: u32) -> PxlResult<IndexFrame> {
    let scale = scale.max(1);
    let (w, h) = scaled_dims(frame.w as u32, frame.h as u32, scale)?;
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

/// [`composite`] with the output size checked first.
pub fn composite_checked(
    frame: &IndexFrame,
    palette: &Palette,
    scale: u32,
    grid: bool,
) -> PxlResult<(u32, u32, Vec<u8>)> {
    scaled_dims(frame.w as u32, frame.h as u32, scale)?;
    Ok(composite(frame, palette, scale, grid))
}

/// Unchecked: prefer [`composite_checked`] for caller-supplied `scale`.
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
    scaled_dims(w, h, scale)?;
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

/// Target size that fits inside `max_w`×`max_h`, preserving aspect (no upscale).
pub fn fit_dims(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 || max_w == 0 || max_h == 0 {
        return (1, 1);
    }
    if src_w <= max_w && src_h <= max_h {
        return (src_w, src_h);
    }
    let sx = max_w as f64 / src_w as f64;
    let sy = max_h as f64 / src_h as f64;
    let s = sx.min(sy);
    let w = ((src_w as f64) * s).floor().max(1.0) as u32;
    let h = ((src_h as f64) * s).floor().max(1.0) as u32;
    (w.min(max_w).max(1), h.min(max_h).max(1))
}

/// Nearest-neighbor resize of an indexed frame to exact `new_w`×`new_h`.
pub fn resize_nearest(frame: &IndexFrame, new_w: u16, new_h: u16) -> PxlResult<IndexFrame> {
    if new_w == 0 || new_h == 0 {
        return Err(PxlError::usage("resize size must be non-zero"));
    }
    let mut out = IndexFrame::canvas(new_w, new_h)?;
    if frame.w == 0 || frame.h == 0 {
        return Ok(out);
    }
    for y in 0..new_h as i32 {
        for x in 0..new_w as i32 {
            let sx = (x as i64 * frame.w as i64) / new_w as i64;
            let sy = (y as i64 * frame.h as i64) / new_h as i64;
            out.set(x, y, frame.get(sx as i32, sy as i32));
        }
    }
    Ok(out)
}

/// Shrink (never enlarge) an indexed frame to fit `max_w`×`max_h`.
pub fn fit_index_frame(frame: &IndexFrame, max_w: u32, max_h: u32) -> PxlResult<IndexFrame> {
    let (w, h) = fit_dims(frame.w as u32, frame.h as u32, max_w, max_h);
    if w == frame.w as u32 && h == frame.h as u32 {
        return Ok(frame.clone());
    }
    resize_nearest(frame, w as u16, h as u16)
}

/// Nearest-neighbor resize of an RGBA8 buffer.
pub fn resize_rgba(src_w: u32, src_h: u32, rgba: &[u8], new_w: u32, new_h: u32) -> PxlResult<Vec<u8>> {
    if new_w == 0 || new_h == 0 || src_w == 0 || src_h == 0 {
        return Err(PxlError::usage("resize size must be non-zero"));
    }
    if rgba.len() < (src_w as usize) * (src_h as usize) * 4 {
        return Err(PxlError::semantic("rgba buffer shorter than src_w*src_h*4"));
    }
    scaled_dims(new_w, new_h, 1)?;
    let mut out = vec![0u8; (new_w as usize) * (new_h as usize) * 4];
    for y in 0..new_h {
        for x in 0..new_w {
            let sx = (x as u64 * src_w as u64) / new_w as u64;
            let sy = (y as u64 * src_h as u64) / new_h as u64;
            let si = ((sy * src_w as u64 + sx) * 4) as usize;
            let di = (y as usize * new_w as usize + x as usize) * 4;
            out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
        }
    }
    Ok(out)
}

/// Preview scaling output cap, in pixels.
const MAX_SCALED_PIXELS: u64 = 64 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::support::palette;

    #[test]
    fn scale_rgba_rejects_a_short_buffer() {
        assert!(scale_rgba_checked(8, 1, &[0u8; 8], 2, false).is_err());
    }

    #[test]
    fn scaled_dims_rejects_overflow_and_over_budget() {
        assert_eq!(scaled_dims(24, 24, 16).unwrap(), (384, 384));
        assert_eq!(scaled_dims(24, 24, 0).unwrap(), (24, 24), "0 means 1");
        assert!(scaled_dims(64, 64, u32::MAX).is_err(), "u32 overflow");
        assert!(scaled_dims(2048, 2048, 64).is_err(), "over pixel budget");
    }

    fn sample() -> IndexFrame {
        let mut f = IndexFrame::new(3, 2).unwrap();
        f.set(0, 0, 2);
        f.set(1, 0, 3);
        f.set(2, 1, 4);
        f
    }

    /// Scaling copies source pixels: sampling the scaled image at the centre of
    /// every block gives the source back, for every factor.
    #[test]
    fn nearest_scaling_never_changes_content() {
        let src = sample();
        for scale in [1u32, 2, 4, 8, 16] {
            let big = scale_nearest(&src, scale).unwrap();
            assert_eq!((big.w as u32, big.h as u32), (3 * scale, 2 * scale));
            for y in 0..src.h as i32 {
                for x in 0..src.w as i32 {
                    for dy in 0..scale as i32 {
                        for dx in 0..scale as i32 {
                            assert_eq!(
                                big.get(x * scale as i32 + dx, y * scale as i32 + dy),
                                src.get(x, y),
                                "scale {scale} at {x},{y}+{dx},{dy}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Without `grid` the preview is a pure block copy; `grid` only recolours
    /// the last row and column of each block.
    #[test]
    fn composite_without_grid_is_a_pure_block_copy_and_grid_only_marks_edges() {
        let pal = palette();
        let src = sample();
        let (w, _h, plain) = composite_checked(&src, &pal, 4, false).unwrap();
        let (_, _, gridded) = composite_checked(&src, &pal, 4, true).unwrap();
        let px = |buf: &[u8], x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
        };
        for y in 0..8u32 {
            for x in 0..12u32 {
                let (bx, by) = (x % 4, y % 4);
                let edge = bx == 3 || by == 3;
                if edge {
                    assert_eq!(px(&gridded, x, y), [20, 18, 16, 255]);
                } else {
                    assert_eq!(px(&gridded, x, y), px(&plain, x, y));
                }
                assert_eq!(
                    px(&plain, x, y),
                    px(&plain, x / 4 * 4, y / 4 * 4),
                    "plain block {x},{y} is uniform"
                );
            }
        }
    }
}
