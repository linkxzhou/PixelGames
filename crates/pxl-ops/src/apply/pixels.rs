use pxl_core::{IndexFrame, Pcg32, PxlError, PxlResult};
use pxl_format::Region;
use pxl_gen::generate_bollinger;

use super::region::{full_rect, resolve_region};
use super::ApplyCx;

pub(super) fn dissolve(
    frame: &IndexFrame,
    seed: u64,
    steps: u32,
    step: u32,
    pattern: &str,
) -> PxlResult<IndexFrame> {
    if steps == 0 || steps > 16 {
        return Err(PxlError::schema("dissolve steps must be in 1..=16"));
    }
    if step > steps {
        return Err(PxlError::schema("dissolve step must be <= steps"));
    }
    const BAYER4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    let mut out = frame.clone();
    let mut rng = Pcg32::from_seed(seed);
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            let rank = match pattern {
                "bayer4" | "bayer" => BAYER4[((y & 3) * 4 + (x & 3)) as usize] as u32,
                "random" => rng.gen_range(16),
                other => {
                    return Err(
                        PxlError::schema(format!("unknown dissolve pattern '{other}'"))
                            .with_hint("pattern is bayer4 or random"),
                    );
                }
            };
            // Consume a draw on bayer too so both patterns advance? No: bayer
            // must not depend on call count. Only random consumes.
            if c != 0 {
                let limit = step.saturating_mul(16) / steps;
                if rank < limit {
                    out.set(x, y, 0);
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn mask_edit(
    frame: &IndexFrame,
    region: &Option<Region>,
    mask: &Option<Vec<String>>,
    ox: i32,
    oy: i32,
    fill: Option<u8>,
) -> PxlResult<IndexFrame> {
    let mut out = frame.clone();
    if let Some(rows) = mask {
        if rows.len() > 16 || rows.iter().any(|r| r.chars().count() > 16) {
            return Err(PxlError::schema("mask is larger than 16×16"));
        }
        for (ry, row) in rows.iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                if ch != '.' && ch != '0' {
                    let x = ox + rx as i32;
                    let y = oy + ry as i32;
                    out.set(x, y, fill.unwrap_or(0));
                }
            }
        }
        return Ok(out);
    }
    let rect = resolve_region(region, frame)?.unwrap_or_else(|| full_rect(frame));
    for y in rect[1]..rect[1] + rect[3] {
        for x in rect[0]..rect[0] + rect[2] {
            if frame.opaque_at(x, y) || fill.is_some() {
                out.set(x, y, fill.unwrap_or(0));
            }
        }
    }
    Ok(out)
}

pub(super) fn patch(frame: &IndexFrame, x: i32, y: i32, rows: &[String]) -> PxlResult<IndexFrame> {
    if rows.len() > 16 || rows.iter().any(|r| r.chars().count() > 16) {
        return Err(PxlError::schema("patch_pixels is limited to 16×16")
            .with_hint("split a larger edit into several patches"));
    }
    let mut out = frame.clone();
    for (ry, row) in rows.iter().enumerate() {
        for (rx, ch) in row.chars().enumerate() {
            let idx = pxl_core::decode_index(ch)?;
            out.set(x + rx as i32, y + ry as i32, idx);
        }
    }
    Ok(out)
}

pub(super) fn texture(
    frame: &IndexFrame,
    gen: &str,
    template: &str,
    rect: [i32; 4],
    ramp: Option<&str>,
    seed: u64,
    cx: &ApplyCx,
) -> PxlResult<IndexFrame> {
    if gen != "bollinger" {
        return Err(
            PxlError::semantic(format!("texture gen '{gen}' is not available"))
                .with_hint("texture gen is bollinger"),
        );
    }
    let loader = cx
        .load_template
        .ok_or_else(|| PxlError::semantic("no template loader configured for texture"))?;
    let mask = loader(template)?;
    let generated = generate_bollinger(&mask, seed, cx.palette, ramp)?;
    let [rx, ry, rw, rh] = rect;
    let mut out = frame.clone();
    let gw = generated.w as i32;
    let gh = generated.h as i32;
    if gw == 0 || gh == 0 {
        return Ok(out);
    }
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            if !frame.opaque_at(x, y) {
                continue;
            }
            let sx = (lx * gw / rw).clamp(0, gw - 1);
            let sy = (ly * gh / rh).clamp(0, gh - 1);
            let sample = generated.get(sx, sy);
            if sample != 0 {
                out.set(x, y, sample);
            }
        }
    }
    Ok(out)
}
