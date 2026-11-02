use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::ColorRef;

use super::region::full_rect;

pub(super) fn color_of(palette: &Palette, color: &ColorRef) -> PxlResult<u8> {
    match color {
        ColorRef::Index(i) => {
            palette.get(*i)?;
            Ok(*i)
        }
        ColorRef::Name(name) => palette.resolve_color_name(name),
    }
}

enum SwapRule {
    One { from: u8, to: u8 },
    All(u8),
    Ramp { name: String, delta: i32 },
    RampMap { from: String, to: String },
}

pub(super) fn palette_swap(
    frame: &IndexFrame,
    map: &str,
    palette: &Palette,
    region: Option<[i32; 4]>,
) -> PxlResult<IndexFrame> {
    let mut rules = Vec::new();
    for raw in map.split(|c| c == ',' || c == ';') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some(rest) = raw.strip_prefix("ramp:") {
            if let Some((src, dst)) = rest.split_once("->") {
                let from = src.trim().to_string();
                let to = dst.trim().to_string();
                let a = palette.ramps.get(&from).ok_or_else(|| {
                    PxlError::semantic(format!("unknown ramp '{from}'"))
                        .with_hint("ramp names are declared on the palette")
                })?;
                let b = palette.ramps.get(&to).ok_or_else(|| {
                    PxlError::semantic(format!("unknown ramp '{to}'"))
                        .with_hint("ramp names are declared on the palette")
                })?;
                if a.len() != b.len() {
                    return Err(PxlError::semantic(format!(
                        "ramp '{from}' has {} steps and '{to}' has {}",
                        a.len(),
                        b.len()
                    ))
                    .with_hint(
                        "ramp:src->dst maps by step, so the ramps must be the same length",
                    ));
                }
                rules.push(SwapRule::RampMap { from, to });
                continue;
            }
            let (name, delta) = split_ramp_delta(rest)?;
            if !palette.ramps.contains_key(&name) {
                return Err(PxlError::semantic(format!("unknown ramp '{name}'"))
                    .with_hint("ramp names are declared on the palette"));
            }
            rules.push(SwapRule::Ramp { name, delta });
        } else if let Some((from, to)) = raw.split_once("->") {
            if from.trim() == "all" {
                rules.push(SwapRule::All(palette.resolve_color_name(to.trim())?));
            } else {
                rules.push(SwapRule::One {
                    from: palette.resolve_color_name(from.trim())?,
                    to: palette.resolve_color_name(to.trim())?,
                });
            }
        } else {
            return Err(PxlError::schema(format!("bad palette_swap map '{raw}'"))
                .with_hint("use 3->7, all->flash, ramp:olive+1, or ramp:hair->blonde"));
        }
    }
    if rules.is_empty() {
        return Err(PxlError::schema("palette_swap map is empty"));
    }
    let rect = region.unwrap_or_else(|| full_rect(frame));
    let mut out = frame.clone();
    for y in rect[1]..rect[1] + rect[3] {
        for x in rect[0]..rect[0] + rect[2] {
            if !frame.in_bounds(x, y) {
                continue;
            }
            let mut c = frame.get(x, y);
            for rule in &rules {
                c = match rule {
                    SwapRule::One { from, to } if c == *from => *to,
                    SwapRule::All(to) if c != 0 => *to,
                    SwapRule::Ramp { name, delta } if palette.in_ramp(name, c) => {
                        // Shift inside this ramp only.
                        if let Some(pos) = palette.ramp_pos(name, c) {
                            let ramp = &palette.ramps[name];
                            let next = (pos as i32 + delta).clamp(0, ramp.len() as i32 - 1);
                            ramp[next as usize]
                        } else {
                            c
                        }
                    }
                    SwapRule::RampMap { from, to } => {
                        if let Some(pos) = palette.ramp_pos(from, c) {
                            palette.ramps[to][pos]
                        } else {
                            c
                        }
                    }
                    _ => c,
                };
            }
            out.set(x, y, c);
        }
    }
    Ok(out)
}

fn split_ramp_delta(s: &str) -> PxlResult<(String, i32)> {
    if let Some(i) = s.rfind('+') {
        let name = s[..i].to_string();
        let delta: i32 = s[i + 1..].parse().map_err(|_| {
            PxlError::schema(format!("bad ramp shift '{s}'")).with_hint("example: ramp:olive+1")
        })?;
        return Ok((name, delta));
    }
    if let Some(i) = s.rfind('-') {
        if i > 0 {
            let name = s[..i].to_string();
            let delta: i32 = s[i..].parse().map_err(|_| {
                PxlError::schema(format!("bad ramp shift '{s}'")).with_hint("example: ramp:olive-1")
            })?;
            return Ok((name, delta));
        }
    }
    Err(PxlError::schema(format!("ramp shift '{s}' needs +N or -N"))
        .with_hint("example: ramp:skin+1"))
}

pub(super) fn remap(
    frame: &IndexFrame,
    map: &BTreeMap<u8, u8>,
    region: Option<[i32; 4]>,
) -> IndexFrame {
    let rect = region.unwrap_or_else(|| full_rect(frame));
    let mut out = frame.clone();
    for y in rect[1]..rect[1] + rect[3] {
        for x in rect[0]..rect[0] + rect[2] {
            if !frame.in_bounds(x, y) {
                continue;
            }
            let c = frame.get(x, y);
            if let Some(&n) = map.get(&c) {
                out.set(x, y, n);
            }
        }
    }
    out
}

pub(super) fn shade(
    frame: &IndexFrame,
    dir: &str,
    amount: i32,
    palette: &Palette,
    region: Option<[i32; 4]>,
    ramp: Option<&str>,
) -> PxlResult<IndexFrame> {
    let (lx, ly) = match dir {
        "top_left" | "nw" => (-1, -1),
        "top" | "n" => (0, -1),
        "top_right" | "ne" => (1, -1),
        "left" | "w" => (-1, 0),
        "right" | "e" => (1, 0),
        "bottom_left" | "sw" => (-1, 1),
        "bottom" | "s" => (0, 1),
        "bottom_right" | "se" => (1, 1),
        other => {
            return Err(PxlError::schema(format!("unknown shade dir '{other}'"))
                .with_hint("dir is top_left, top, left, bottom, right, or a diagonal"));
        }
    };
    let rect = region.unwrap_or_else(|| full_rect(frame));
    let mut out = frame.clone();
    for y in rect[1]..rect[1] + rect[3] {
        for x in rect[0]..rect[0] + rect[2] {
            if !frame.in_bounds(x, y) {
                continue;
            }
            let c = frame.get(x, y);
            // The outline colour is not a surface: shading never repaints it.
            if c == 0 || c == palette.special.outline {
                continue;
            }
            let mut delta = 0;
            if !frame.opaque_at(x + lx, y + ly) {
                delta += amount;
            }
            if !frame.opaque_at(x - lx, y - ly) {
                delta -= amount;
            }
            if delta != 0 {
                let next = match ramp {
                    Some(name) => palette.shift_in_ramp(name, c, delta),
                    None => palette.shift_on_unique_ramp(c, delta),
                };
                out.set(x, y, next);
            }
        }
    }
    Ok(out)
}
