use pxl_core::{op_seed, IndexFrame, Palette, Pcg32, PxlError, PxlResult};
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
        check_patch_size(rows, "mask")?;
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

/// Pixels a mask or region selects. A mask is rows of `.`/`0` (outside) and
/// anything else (inside) anchored at `(ox, oy)`; a region is a rectangle; with
/// neither, the whole frame. Selection is by position only: what lies there is
/// decided by the caller.
fn selection(
    frame: &IndexFrame,
    region: &Option<Region>,
    mask: &Option<Vec<String>>,
    ox: i32,
    oy: i32,
) -> PxlResult<Vec<bool>> {
    let mut sel = vec![false; frame.pixels.len()];
    if let Some(rows) = mask {
        check_patch_size(rows, "mask")?;
        for (ry, row) in rows.iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                if ch == '.' || ch == '0' {
                    continue;
                }
                if let Some(i) = frame.idx(ox + rx as i32, oy + ry as i32) {
                    sel[i] = true;
                }
            }
        }
        return Ok(sel);
    }
    let rect = resolve_region(region, frame)?.unwrap_or_else(|| full_rect(frame));
    for y in rect[1]..rect[1].saturating_add(rect[3]) {
        for x in rect[0]..rect[0].saturating_add(rect[2]) {
            if let Some(i) = frame.idx(x, y) {
                sel[i] = true;
            }
        }
    }
    Ok(sel)
}

/// Move pixels of ramp `from` that lie in the selection along that ramp, or
/// onto the same step of ramp `to`. Pixels of other colours, the outline colour
/// and transparency are never touched, whatever the selection covers.
#[allow(clippy::too_many_arguments)]
pub(super) fn material(
    frame: &IndexFrame,
    palette: &Palette,
    from: &str,
    to: Option<&str>,
    steps: i32,
    region: &Option<Region>,
    mask: &Option<Vec<String>>,
    ox: i32,
    oy: i32,
) -> PxlResult<IndexFrame> {
    let src = palette.ramps.get(from).ok_or_else(|| {
        PxlError::semantic(format!("material ramp '{from}' is missing"))
            .with_path("/from")
            .with_hint("name a ramp declared on the palette")
    })?;
    if !(-8..=8).contains(&steps) {
        return Err(PxlError::schema("material steps must be in -8..=8").with_path("/steps"));
    }
    let dst = match to {
        Some(name) => {
            let r = palette.ramps.get(name).ok_or_else(|| {
                PxlError::semantic(format!("material target ramp '{name}' is missing"))
                    .with_path("/to")
                    .with_hint("name a ramp declared on the palette")
            })?;
            if r.len() != src.len() {
                return Err(PxlError::semantic(format!(
                    "ramp '{from}' has {} steps and '{name}' has {}",
                    src.len(),
                    r.len()
                ))
                .with_hint("material maps by step, so the ramps must be the same length"));
            }
            r
        }
        None => src,
    };
    let sel = selection(frame, region, mask, ox, oy)?;
    let mut out = frame.clone();
    for (i, &c) in frame.pixels.iter().enumerate() {
        if !sel[i] || c == 0 || c == palette.special.outline {
            continue;
        }
        let Some(pos) = src.iter().position(|&r| r == c) else {
            continue;
        };
        let next = (pos as i32 + steps).clamp(0, dst.len() as i32 - 1) as usize;
        let target = dst[next];
        if target != 0 {
            out.pixels[i] = target;
        }
    }
    Ok(out)
}

/// Deterministic detail pixels. Eligible pixels belong to `ramp`, are inside
/// `rect` and at least `inset` pixels (4-neighbour distance) from transparency.
/// A pixel is chosen when `hash(seed, x, y) % 10000 < density`; the hash depends
/// on the position only, so adding or moving other detail never reshuffles it.
#[allow(clippy::too_many_arguments)]
pub(super) fn detail(
    frame: &IndexFrame,
    palette: &Palette,
    ramp: &str,
    color: u8,
    rect: [i32; 4],
    density: u32,
    inset: i32,
    seed: u64,
) -> PxlResult<IndexFrame> {
    let list = palette.ramps.get(ramp).ok_or_else(|| {
        PxlError::semantic(format!("detail ramp '{ramp}' is missing"))
            .with_path("/ramp")
            .with_hint("name a ramp declared on the palette")
    })?;
    if density > 10_000 {
        return Err(PxlError::schema("detail density must be 0..=10000").with_path("/density"));
    }
    if !(0..=8).contains(&inset) {
        return Err(PxlError::schema("detail inset must be 0..=8").with_path("/inset"));
    }
    let mut out = frame.clone();
    for y in rect[1]..rect[1].saturating_add(rect[3]) {
        for x in rect[0]..rect[0].saturating_add(rect[2]) {
            let c = frame.get(x, y);
            if c == 0 || !list.contains(&c) {
                continue;
            }
            let near_edge = (1..=inset).any(|d| {
                !frame.opaque_at(x - d, y)
                    || !frame.opaque_at(x + d, y)
                    || !frame.opaque_at(x, y - d)
                    || !frame.opaque_at(x, y + d)
            });
            if near_edge {
                continue;
            }
            let h = op_seed(seed, (y as i64 as u64) << 32 | (x as u32 as u64), "detail");
            if (h % 10_000) < density as u64 {
                out.set(x, y, color);
            }
        }
    }
    Ok(out)
}

/// A mask or patch may be as large as the canvas it edits, up to the shared
/// frame budget: a 96 or 128 wide sprite needs more than the old 16x16.
fn check_patch_size(rows: &[String], what: &str) -> PxlResult<()> {
    let max = pxl_core::MAX_DIM as usize;
    let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    if rows.len() > max || width > max {
        return Err(PxlError::schema(format!(
            "{what} is {}x{width}, larger than the {max}x{max} limit",
            rows.len()
        ))
        .with_hint("split a larger edit into several ops"));
    }
    if rows.len() * width > pxl_core::MAX_FRAME_PIXELS {
        return Err(PxlError::schema(format!(
            "{what} is {}x{width}, over the {} pixel budget",
            rows.len(),
            pxl_core::MAX_FRAME_PIXELS
        )));
    }
    Ok(())
}

pub(super) fn patch(frame: &IndexFrame, x: i32, y: i32, rows: &[String]) -> PxlResult<IndexFrame> {
    check_patch_size(rows, "patch_pixels")?;
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
