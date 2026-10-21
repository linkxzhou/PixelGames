use std::cell::RefCell;
use std::collections::BTreeMap;

use pxl_core::{
    op_seed, paint_outline, Brush, IndexFrame, OutlineMode, Palette, Pcg32, PxlError, PxlResult,
};
use pxl_format::{ColorRef, Op, PartsFile, Region};
use pxl_gen::{generate_bollinger, RuntimeMask};

use crate::parts::{self, PartPose};

pub struct ApplyCx<'a> {
    pub palette: &'a Palette,
    pub seed: u64,
    pub frame_name: &'a str,
    pub load_template: Option<&'a dyn Fn(&str) -> PxlResult<RuntimeMask>>,
    /// Paper-doll rig used by `part_transform` and `compose`.
    pub parts: Option<&'a PartsFile>,
    /// Pose accumulated by `part_transform` inside one `apply_ops` call.
    pub pose: RefCell<BTreeMap<String, PartPose>>,
}

pub fn apply_ops(frame: &IndexFrame, ops: &[Op], cx: &ApplyCx) -> PxlResult<IndexFrame> {
    cx.pose.borrow_mut().clear();
    let mut cur = frame.clone();
    for (i, op) in ops.iter().enumerate() {
        cur = apply_op(&cur, op, cx, i as u64)?;
    }
    Ok(cur)
}

pub fn apply_op(frame: &IndexFrame, op: &Op, cx: &ApplyCx, index: u64) -> PxlResult<IndexFrame> {
    match op {
        Op::Noop {} => Ok(frame.clone()),
        Op::Shift {
            dx,
            dy,
            wrap,
            region,
        } => {
            check_delta(*dx, *dy)?;
            let rect = resolve_region(region, frame)?;
            Ok(translate(frame, *dx, *dy, *wrap, rect))
        }
        Op::RowShift {
            rows,
            dx,
            dy,
            pattern,
            phase,
            region,
            axis,
            wrap,
        } => {
            check_delta(*dx, *dy)?;
            if let Some(rect) = resolve_region(region, frame)? {
                let axis = axis.as_deref().unwrap_or("x");
                let amount = if axis == "y" || axis == "vertical" {
                    if *dy != 0 {
                        *dy
                    } else {
                        *dx
                    }
                } else {
                    *dx
                };
                let do_wrap = wrap.unwrap_or(true);
                return scroll_rect(frame, rect, axis, amount, do_wrap);
            }
            let [y0, y1] = rows.unwrap_or([0, frame.h as i32 - 1]);
            let do_wrap = wrap.unwrap_or(false);
            Ok(shift_rows(
                frame,
                y0,
                y1,
                *dx,
                pattern.as_deref(),
                *phase,
                do_wrap,
            )?)
        }
        Op::Shear {
            axis,
            k,
            pivot,
            region,
        } => {
            if *k == 0 {
                return Err(PxlError::schema("shear k must be non-zero").with_path("/k"));
            }
            let rect = resolve_region(region, frame)?.unwrap_or(full_rect(frame));
            Ok(shear(frame, axis, *k, *pivot, rect))
        }
        Op::Squash {
            sx,
            sy,
            anchor,
            region,
        } => {
            check_delta(*sx, *sy)?;
            let rect = resolve_region(region, frame)?.unwrap_or(full_rect(frame));
            squash(frame, *sx, *sy, anchor, rect)
        }
        Op::Flip { axis } => Ok(flip(frame, axis)?),
        Op::Rotate90 { dir } => rotate90(frame, dir),
        Op::Rotate { .. } => Err(PxlError::semantic(
            "arbitrary-angle rotate is not implemented yet",
        )
        .with_hint("use rotate90 (cw, ccw, 180). cleanEdge angles arrive in M2")),
        Op::PaletteSwap { map, region } => {
            let rect = resolve_region(region, frame)?;
            palette_swap(frame, map, cx.palette, rect)
        }
        Op::PaletteCycle {
            cycle,
            step,
            region,
        } => {
            let rect = resolve_region(region, frame)?;
            let map = cx.palette.cycle_map(cycle, *step)?;
            Ok(remap(frame, &map, rect))
        }
        Op::Shade {
            dir,
            amount,
            region,
        } => {
            if !(-4..=4).contains(amount) {
                return Err(PxlError::schema("shade amount must be in -4..=4"));
            }
            let rect = resolve_region(region, frame)?;
            Ok(shade(frame, dir, *amount, cx.palette, rect)?)
        }
        Op::Outline {
            color,
            mode,
            brush,
            width,
        } => {
            let mut out = frame.clone();
            let c = color_of(cx.palette, color)?;
            paint_outline(
                &mut out,
                c,
                OutlineMode::parse(mode)?,
                Brush::parse(brush)?,
                *width,
            )?;
            Ok(out)
        }
        Op::Dissolve {
            seed,
            steps,
            step,
            pattern,
        } => {
            let sub = op_seed(cx.seed ^ seed, index, cx.frame_name);
            dissolve(frame, sub, *steps, *step, pattern)
        }
        Op::MaskErase { region, mask, x, y } => Ok(mask_edit(frame, region, mask, *x, *y, None)?),
        Op::MaskFill {
            color,
            region,
            mask,
            x,
            y,
        } => {
            let c = color_of(cx.palette, color)?;
            Ok(mask_edit(frame, region, mask, *x, *y, Some(c))?)
        }
        Op::PatchPixels { x, y, rows } => patch(frame, *x, *y, rows),
        Op::Texture {
            gen,
            template,
            region,
            ramp,
            seed,
        } => {
            let rect = resolve_region(&Some(region.clone()), frame)?.ok_or_else(|| {
                PxlError::semantic("texture region must be a rectangle").with_hint("use [x,y,w,h]")
            })?;
            let sub = seed.unwrap_or_else(|| op_seed(cx.seed, index, cx.frame_name));
            texture(frame, gen, template, rect, ramp.as_deref(), sub, cx)
        }
        Op::PartTransform {
            part,
            dx,
            dy,
            rot90,
            flip,
        } => {
            check_delta(*dx, *dy)?;
            let rig = cx.parts.ok_or_else(|| {
                PxlError::semantic("part_transform needs a parts spec")
                    .with_hint("use base.gen parts, or set meta.parts to a .parts.json file")
            })?;
            rig.part(part)?;
            let (flip_x, flip_y) = parts::parse_flip(flip.as_deref())?;
            cx.pose.borrow_mut().insert(
                part.clone(),
                PartPose {
                    dx: *dx,
                    dy: *dy,
                    rot: parts::parse_rot(rot90.as_deref())?,
                    flip_x,
                    flip_y,
                },
            );
            Ok(frame.clone())
        }
        Op::Compose { parts: order, dir } => {
            let rig = cx.parts.ok_or_else(|| {
                PxlError::semantic("compose needs a parts spec")
                    .with_hint("use base.gen parts, or set meta.parts to a .parts.json file")
            })?;
            let facing = dir.as_deref().unwrap_or("down");
            let pose = cx.pose.borrow().clone();
            let drawn = parts::compose_rig(frame, rig, facing, order, &pose)?;
            cx.pose.borrow_mut().clear();
            Ok(drawn)
        }
    }
}

fn check_delta(dx: i32, dy: i32) -> PxlResult<()> {
    if !(-32..=32).contains(&dx) || !(-32..=32).contains(&dy) {
        return Err(
            PxlError::schema(format!("offset ({dx},{dy}) is outside -32..=32"))
                .with_path("/dx")
                .with_hint("dx and dy must be in -32..32"),
        );
    }
    Ok(())
}

fn full_rect(frame: &IndexFrame) -> [i32; 4] {
    [0, 0, frame.w as i32, frame.h as i32]
}

fn resolve_region(region: &Option<Region>, frame: &IndexFrame) -> PxlResult<Option<[i32; 4]>> {
    let Some(region) = region else {
        return Ok(None);
    };
    match region.to_rect() {
        Some(r) => {
            if r[2] <= 0 || r[3] <= 0 {
                return Err(PxlError::schema("region width and height must be positive")
                    .with_path("/region"));
            }
            if r[0] < -(frame.w as i32)
                || r[1] < -(frame.h as i32)
                || r[0] > frame.w as i32
                || r[1] > frame.h as i32
            {
                return Err(PxlError::semantic("region is outside the frame").with_path("/region"));
            }
            Ok(Some(r))
        }
        None => Err(PxlError::semantic(format!(
            "named region '{}' is unresolved",
            region.name().unwrap_or("?")
        ))
        .with_hint("pass [x,y,w,h]. auto_tread is resolved inside anim macros")),
    }
}

fn translate(
    frame: &IndexFrame,
    dx: i32,
    dy: i32,
    wrap: bool,
    region: Option<[i32; 4]>,
) -> IndexFrame {
    let rect = region.unwrap_or_else(|| full_rect(frame));
    let [rx, ry, rw, rh] = rect;
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            let (sx, sy) = if wrap {
                let sx = rx + (lx - dx).rem_euclid(rw);
                let sy = ry + (ly - dy).rem_euclid(rh);
                (sx, sy)
            } else {
                (x - dx, y - dy)
            };
            let c = if wrap || (sx >= rx && sy >= ry && sx < rx + rw && sy < ry + rh) {
                frame.get(sx, sy)
            } else {
                0
            };
            if frame.in_bounds(x, y) {
                out.set(x, y, c);
            }
        }
    }
    out
}

fn scroll_rect(
    frame: &IndexFrame,
    rect: [i32; 4],
    axis: &str,
    amount: i32,
    wrap: bool,
) -> PxlResult<IndexFrame> {
    let vertical = match axis {
        "y" | "vertical" => true,
        "x" | "horizontal" => false,
        other => {
            return Err(
                PxlError::schema(format!("unknown row_shift axis '{other}'"))
                    .with_hint("axis is x or y"),
            );
        }
    };
    let dx = if vertical { 0 } else { amount };
    let dy = if vertical { amount } else { 0 };
    Ok(translate(frame, dx, dy, wrap, Some(rect)))
}

fn shift_rows(
    frame: &IndexFrame,
    y0: i32,
    y1: i32,
    amp: i32,
    pattern: Option<&str>,
    phase: i32,
    wrap: bool,
) -> PxlResult<IndexFrame> {
    const SINE4: [i32; 4] = [0, 1, 0, -1];
    let mut out = frame.clone();
    let y0 = y0.max(0);
    let y1 = y1.min(frame.h as i32 - 1);
    for y in y0..=y1 {
        let dx = match pattern {
            None | Some("none") | Some("") => amp,
            Some("sine4") => amp * SINE4[(y + phase).rem_euclid(4) as usize],
            Some(other) => {
                return Err(
                    PxlError::schema(format!("unknown row_shift pattern '{other}'"))
                        .with_hint("pattern is sine4 or omitted"),
                );
            }
        };
        let w = frame.w as i32;
        for x in 0..w {
            let sx = if wrap { (x - dx).rem_euclid(w) } else { x - dx };
            let c = if sx >= 0 && sx < w {
                frame.get(sx, y)
            } else {
                0
            };
            out.set(x, y, c);
        }
    }
    Ok(out)
}

fn shear(frame: &IndexFrame, axis: &str, k: i32, pivot: i32, rect: [i32; 4]) -> IndexFrame {
    let [rx, ry, rw, rh] = rect;
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            if !frame.in_bounds(x, y) {
                continue;
            }
            out.set(x, y, 0);
        }
    }
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            let c = frame.get(x, y);
            if c == 0 || !frame.in_bounds(x, y) {
                continue;
            }
            let (nx, ny) = match axis {
                "y" | "vertical" => {
                    let shift = (x - pivot) / k;
                    (x, y + shift)
                }
                _ => {
                    let shift = (y - pivot) / k;
                    (x + shift, y)
                }
            };
            if nx >= rx && ny >= ry && nx < rx + rw && ny < ry + rh {
                out.set(nx, ny, c);
            }
        }
    }
    out
}

fn squash(
    frame: &IndexFrame,
    sx: i32,
    sy: i32,
    anchor: &str,
    rect: [i32; 4],
) -> PxlResult<IndexFrame> {
    let [rx, ry, rw, rh] = rect;
    let dw = (rw + sx).max(1);
    let dh = (rh + sy).max(1);
    let (hx, hy) = match anchor {
        "bottom" => ("center", "end"),
        "top" => ("center", "start"),
        "left" => ("start", "center"),
        "right" => ("end", "center"),
        "center" => ("center", "center"),
        other => {
            return Err(PxlError::schema(format!("unknown squash anchor '{other}'"))
                .with_hint("anchor is bottom, top, left, right, or center"));
        }
    };
    let ox = place(rw, dw, hx);
    let oy = place(rh, dh, hy);
    let mut out = frame.clone();
    for ly in 0..rh {
        for lx in 0..rw {
            let x = rx + lx;
            let y = ry + ly;
            if frame.in_bounds(x, y) {
                out.set(x, y, 0);
            }
        }
    }
    for dy in 0..dh {
        for dx in 0..dw {
            let sx_i = sample_index(dx, rw, dw);
            let sy_i = sample_index(dy, rh, dh);
            let c = frame.get(rx + sx_i, ry + sy_i);
            let x = rx + ox + dx;
            let y = ry + oy + dy;
            if frame.in_bounds(x, y) {
                out.set(x, y, c);
            }
        }
    }
    Ok(out)
}

fn sample_index(i: i32, src_len: i32, dst_len: i32) -> i32 {
    if dst_len <= 1 || src_len <= 1 {
        return 0;
    }
    i * (src_len - 1) / (dst_len - 1)
}

fn place(src_len: i32, dst_len: i32, how: &str) -> i32 {
    match how {
        "start" => 0,
        "end" => src_len - dst_len,
        _ => (src_len - dst_len) / 2,
    }
}

/// Left-right when `axis` is x (Piskel's VERTICAL). Up-down when `axis` is y
/// (Piskel's HORIZONTAL).
pub fn flip(frame: &IndexFrame, axis: &str) -> PxlResult<IndexFrame> {
    let mut out = frame.blank_like();
    let horizontal = match axis {
        "x" | "h" | "lr" | "horizontal" => true,
        "y" | "v" | "tb" | "vertical" => false,
        other => {
            return Err(PxlError::schema(format!("unknown flip axis '{other}'"))
                .with_hint("axis x mirrors left-right, axis y mirrors up-down"));
        }
    };
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let (nx, ny) = if horizontal {
                (frame.w as i32 - 1 - x, y)
            } else {
                (x, frame.h as i32 - 1 - y)
            };
            out.set(nx, ny, frame.get(x, y));
        }
    }
    Ok(out)
}

/// Piskel `TransformUtils.rotate`: pad to a square with `ceil`, rotate, crop
/// back to the original width and height. Square sprites are a plain 90° turn.
pub fn rotate90(frame: &IndexFrame, dir: &str) -> PxlResult<IndexFrame> {
    match dir {
        "180" | "half" => {
            let once = flip(frame, "x")?;
            flip(&once, "y")
        }
        "cw" | "clockwise" | "ccw" | "counterclockwise" => {
            let cw = dir == "cw" || dir == "clockwise";
            let w = frame.w as i32;
            let h = frame.h as i32;
            let max = w.max(h);
            let x_delta = (max - w + 1) / 2;
            let y_delta = (max - h + 1) / 2;
            let mut out = frame.blank_like();
            for dy in 0..h {
                for dx in 0..w {
                    let x = dx + x_delta;
                    let y = dy + y_delta;
                    let (sx, sy) = if cw {
                        (y, max - 1 - x)
                    } else {
                        (max - 1 - y, x)
                    };
                    let sx = sx - x_delta;
                    let sy = sy - y_delta;
                    let c = if frame.in_bounds(sx, sy) {
                        frame.get(sx, sy)
                    } else {
                        0
                    };
                    out.set(dx, dy, c);
                }
            }
            Ok(out)
        }
        other => Err(PxlError::schema(format!("unknown rotate90 dir '{other}'"))
            .with_hint("dir is cw, ccw, or 180")),
    }
}

fn color_of(palette: &Palette, color: &ColorRef) -> PxlResult<u8> {
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

fn palette_swap(
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

fn remap(frame: &IndexFrame, map: &BTreeMap<u8, u8>, region: Option<[i32; 4]>) -> IndexFrame {
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

fn shade(
    frame: &IndexFrame,
    dir: &str,
    amount: i32,
    palette: &Palette,
    region: Option<[i32; 4]>,
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
            if c == 0 {
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
                out.set(x, y, palette.shift_on_ramp(c, delta));
            }
        }
    }
    Ok(out)
}

fn dissolve(
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

fn mask_edit(
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

fn patch(frame: &IndexFrame, x: i32, y: i32, rows: &[String]) -> PxlResult<IndexFrame> {
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

fn texture(
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::*;

    fn grid(rows: &[&str]) -> IndexFrame {
        let owned: Vec<String> = rows.iter().map(|s| (*s).to_string()).collect();
        pxl_core::decode_matrix(&owned).unwrap()
    }

    fn text(frame: &IndexFrame) -> Vec<String> {
        pxl_core::encode_matrix(frame).unwrap()
    }

    fn pal() -> Palette {
        pxl_gen::builtin_palette("tank").unwrap()
    }

    fn cx(pal: &Palette) -> ApplyCx<'_> {
        ApplyCx {
            palette: pal,
            seed: 1,
            frame_name: "base",
            load_template: None,
            parts: None,
            pose: RefCell::new(BTreeMap::new()),
        }
    }

    #[test]
    fn flip_matches_piskel_axes() {
        let f = grid(&["12", "34"]);
        // axis x is left-right, which Piskel calls VERTICAL.
        assert_eq!(text(&flip(&f, "x").unwrap()), vec!["21", "43"]);
        // axis y is up-down, which Piskel calls HORIZONTAL.
        assert_eq!(text(&flip(&f, "y").unwrap()), vec!["34", "12"]);
    }

    #[test]
    fn rotate90_square_matches_hand_trace() {
        let f = grid(&["12", "34"]);
        assert_eq!(text(&rotate90(&f, "cw").unwrap()), vec!["31", "42"]);
        assert_eq!(text(&rotate90(&f, "ccw").unwrap()), vec!["24", "13"]);
        assert_eq!(text(&rotate90(&f, "180").unwrap()), vec!["43", "21"]);
    }

    #[test]
    fn rotate90_nonsquare_matches_piskel_pad() {
        // Values 1..8 on a 4×2 canvas. Hand-traced from TransformUtils.rotate.
        let f = grid(&["1234", "5678"]);
        assert_eq!(text(&rotate90(&f, "cw").unwrap()), vec![".62.", ".73."]);
    }

    #[test]
    fn shift_matches_piskel_move() {
        let pal = pal();
        let f = grid(&["12", "34"]);
        let out = apply_op(
            &f,
            &Op::Shift {
                dx: 1,
                dy: 0,
                wrap: false,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(text(&out), vec![".1", ".3"]);
        let up = apply_op(
            &f,
            &Op::Shift {
                dx: 0,
                dy: -1,
                wrap: false,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(text(&up), vec!["34", ".."]);
    }

    #[test]
    fn shift_wrap_and_region() {
        let pal = pal();
        let f = grid(&["1234", "5678"]);
        let out = apply_op(
            &f,
            &Op::Shift {
                dx: 1,
                dy: 0,
                wrap: true,
                region: Some(Region::Rect([0, 0, 4, 1])),
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(text(&out)[0], "4123");
        assert_eq!(text(&out)[1], "5678");
    }

    #[test]
    fn palette_cycle_and_swap() {
        let pal = pal();
        let f = grid(&["67", ".."]);
        let out = apply_op(
            &f,
            &Op::PaletteCycle {
                cycle: "tread".into(),
                step: 1,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        // 6 -> 7, 7 -> 1
        assert_eq!(text(&out)[0], "71");
        let swapped = apply_op(
            &f,
            &Op::PaletteSwap {
                map: "6->4,all->flash".into(),
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        // 6 becomes 4 first, then `all` sends every opaque pixel to flash (11 = b).
        assert_eq!(text(&swapped)[0], "bb");
    }

    #[test]
    fn outline_shade_dissolve_noop() {
        let pal = pal();
        let f = grid(&[".2.", "222", ".2."]);
        let outlined = apply_op(
            &f,
            &Op::Outline {
                color: ColorRef::Index(1),
                mode: "outer".into(),
                brush: "diamond".into(),
                width: 1,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(outlined.get(0, 0), 1);
        assert_eq!(outlined.get(1, 1), 2);
        assert_eq!(outlined.get(1, 0), 2);
        let shaded = apply_op(
            &f,
            &Op::Shade {
                dir: "top".into(),
                amount: 1,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_ne!(shaded.pixels, f.pixels);
        let same = apply_op(&f, &Op::Noop {}, &cx(&pal), 0).unwrap();
        assert_eq!(same.pixels, f.pixels);
        let gone = apply_op(
            &f,
            &Op::Dissolve {
                seed: 1,
                steps: 1,
                step: 1,
                pattern: "bayer4".into(),
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(gone.count_opaque(), 0);
    }

    #[test]
    fn row_shift_shear_squash_patch() {
        let pal = pal();
        let f = grid(&["1111", "2222", "3333", "4444"]);
        let rows = apply_op(
            &f,
            &Op::RowShift {
                rows: Some([1, 1]),
                dx: 1,
                dy: 0,
                pattern: None,
                phase: 0,
                region: None,
                axis: None,
                wrap: Some(true),
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(text(&rows)[1], "2222");
        // wrap of a solid row stays solid. Use a varied row.
        let f2 = grid(&["1234", "5678"]);
        let rows = apply_op(
            &f2,
            &Op::RowShift {
                rows: Some([0, 0]),
                dx: 1,
                dy: 0,
                pattern: Some("sine4".into()),
                phase: 1,
                region: None,
                axis: None,
                wrap: Some(false),
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        // phase 1 => SINE4[1] = 1, so the row shifts by +1.
        assert_eq!(text(&rows)[0], ".123");
        let sheared = apply_op(
            &f,
            &Op::Shear {
                axis: "x".into(),
                k: 1,
                pivot: 0,
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(sheared.get(1, 1), 2);
        let squashed = apply_op(
            &f,
            &Op::Squash {
                sx: 0,
                sy: -1,
                anchor: "bottom".into(),
                region: None,
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(squashed.h, 4);
        assert_eq!(squashed.get(0, 3), 4);
        let patched = apply_op(
            &f,
            &Op::PatchPixels {
                x: 0,
                y: 0,
                rows: vec!["99".into()],
            },
            &cx(&pal),
            0,
        )
        .unwrap();
        assert_eq!(patched.get(0, 0), 9);
        assert_eq!(patched.get(1, 0), 9);
    }
}
