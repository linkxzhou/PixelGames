//! Tileable ground tiles. Value noise is periodic in integer arithmetic, then
//! opposite edges are snapped so they match or sit on adjacent ramp steps.

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, SplitMix64};

pub fn variant_seed(base: u64, index: u32) -> u64 {
    base ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

pub fn generate_noise_tile(
    kind: &str,
    size: u16,
    seed: u64,
    palette: &Palette,
    ramp_name: Option<&str>,
) -> PxlResult<IndexFrame> {
    if !(4..=64).contains(&size) {
        return Err(PxlError::schema(format!(
            "tile size {size} is outside 4..=64"
        )));
    }
    let (cell, speck_mod, tuft) = match kind {
        "dirt" => (4i32, 17u32, false),
        "grass" => (4, 13, true),
        "sand" => (8, 29, false),
        other => {
            return Err(PxlError::semantic(format!("unknown tile kind '{other}'"))
                .with_hint("kind is dirt, grass, or sand"));
        }
    };
    let ramp_key = ramp_name.unwrap_or(if palette.ramps.contains_key("ground") {
        "ground"
    } else {
        palette
            .ramps
            .keys()
            .next()
            .map(|s| s.as_str())
            .unwrap_or("")
    });
    let ramp = palette.ramps.get(ramp_key).ok_or_else(|| {
        PxlError::semantic(format!("tile ramp '{ramp_key}' is missing"))
            .with_hint("add a ground ramp or pass --ramp")
    })?;
    if ramp.is_empty() {
        return Err(PxlError::schema("tile ramp is empty"));
    }
    let mut frame = IndexFrame::new(size, size)?;
    let period = size as i32;
    let cell = cell.min(period).max(1);
    for y in 0..period {
        for x in 0..period {
            let n = value_noise(seed, x, y, period, cell);
            let mut slot = ((n as usize) * (ramp.len() - 1)) / 255;
            let interior = x > 0 && y > 0 && x + 1 < period && y + 1 < period;
            if interior {
                let h = hash32(seed ^ 0xA5A5_A5A5, x, y);
                if h % speck_mod == 0 && slot > 0 {
                    slot -= 1;
                } else if tuft && h % 11 == 0 && slot + 1 < ramp.len() {
                    slot += 1;
                }
            }
            frame.set(x, y, ramp[slot]);
        }
    }
    enforce_edges(&mut frame, palette, ramp_key)?;
    Ok(frame)
}

fn enforce_edges(frame: &mut IndexFrame, palette: &Palette, ramp: &str) -> PxlResult<()> {
    let w = frame.w as i32;
    let h = frame.h as i32;
    for _ in 0..4 {
        let mut changed = false;
        for y in 0..h {
            if edge_far(palette, ramp, frame.get(0, y), frame.get(w - 1, y))? {
                frame.set(w - 1, y, frame.get(0, y));
                changed = true;
            }
        }
        for x in 0..w {
            if edge_far(palette, ramp, frame.get(x, 0), frame.get(x, h - 1))? {
                frame.set(x, h - 1, frame.get(x, 0));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(())
}

fn edge_far(palette: &Palette, ramp: &str, a: u8, b: u8) -> PxlResult<bool> {
    match palette.ramp_distance(ramp, a, b) {
        Some(d) => Ok(d > 1),
        None => Err(PxlError::semantic(
            "tile pixel fell outside the ground ramp; edge check cannot run",
        )),
    }
}

fn value_noise(seed: u64, x: i32, y: i32, period: i32, cell: i32) -> u32 {
    let x = x.rem_euclid(period);
    let y = y.rem_euclid(period);
    let x0 = (x / cell) * cell;
    let y0 = (y / cell) * cell;
    let x1 = (x0 + cell) % period;
    let y1 = (y0 + cell) % period;
    let fx = ((x - x0) * 256) / cell;
    let fy = ((y - y0) * 256) / cell;
    let sx = smooth(fx as u32);
    let sy = smooth(fy as u32);
    let v00 = hash32(seed, x0, y0);
    let v10 = hash32(seed, x1, y0);
    let v01 = hash32(seed, x0, y1);
    let v11 = hash32(seed, x1, y1);
    lerp(lerp(v00, v10, sx), lerp(v01, v11, sx), sy)
}

fn smooth(t: u32) -> u32 {
    let t = t.min(256);
    (t * t * (768 - 2 * t)) / 65536
}

fn lerp(a: u32, b: u32, t: u32) -> u32 {
    (a * (256 - t) + b * t) / 256
}

fn hash32(seed: u64, x: i32, y: i32) -> u32 {
    let mixed = seed
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (SplitMix64::new(mixed).next_u64() & 255) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirt_is_tileable_and_stable() {
        let pal = crate::builtin_palette("tank").unwrap();
        let a = generate_noise_tile("dirt", 16, 7, &pal, Some("ground")).unwrap();
        let b = generate_noise_tile("dirt", 16, 7, &pal, Some("ground")).unwrap();
        assert_eq!(a.pixels, b.pixels);
        let ramp = "ground";
        for y in 0..16 {
            let d = pal.ramp_distance(ramp, a.get(0, y), a.get(15, y)).unwrap();
            assert!(d <= 1, "vertical seam y={y} dist={d}");
        }
        for x in 0..16 {
            let d = pal.ramp_distance(ramp, a.get(x, 0), a.get(x, 15)).unwrap();
            assert!(d <= 1, "horizontal seam x={x} dist={d}");
        }
        let c = generate_noise_tile("dirt", 16, 8, &pal, Some("ground")).unwrap();
        assert_ne!(a.pixels, c.pixels);
    }
}
