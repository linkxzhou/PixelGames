use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::{Op, Region};

use super::args::{arg_i32, arg_str, steps_of};

pub(super) fn tread_cycle(
    args: &BTreeMap<String, String>,
    dir: Option<&str>,
    frame: &IndexFrame,
    palette: &Palette,
) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 4)?;
    let cycle = arg_str(args, "cycle", "tread").to_string();
    let bob = arg_i32(args, "bob", 1)?;
    let region = arg_str(args, "region", "auto_tread");
    if region != "auto_tread" {
        return Err(
            PxlError::semantic(format!("tread_cycle region '{region}' is not supported"))
                .with_hint("use region=auto_tread; the macro finds tread pixels from the ramp"),
        );
    }
    if !palette.cycles.contains_key(&cycle) {
        return Err(
            PxlError::semantic(format!("palette has no cycle '{cycle}'"))
                .with_hint("declare cycles.tread on the palette"),
        );
    }
    let along_y = !matches!(dir, Some("E") | Some("W"));
    let axis = if along_y { "y" } else { "x" };
    // N/S sink downward. E/W jolt along the gun, where the rotated tread bands
    // are not in the way, so the hull pixel count stays stable.
    let (bdx, bdy) = if along_y { (0, bob) } else { (bob, 0) };
    let treads = detect_treads(frame, palette, along_y)?;
    if treads.is_empty() {
        return Err(PxlError::semantic("auto_tread found no tread pixels")
            .with_hint("paint tread-ramp colors on the outer columns (N/S) or rows (E/W)"));
    }
    let hull = hull_rect(frame, &treads, bdx, bdy);
    let period = palette
        .cycles
        .get(&cycle)
        .map(|c| c.indices.len().max(1))
        .unwrap_or(1);
    let mut frames = Vec::with_capacity(steps);
    for i in 0..steps {
        let mut ops = Vec::new();
        for rect in &treads {
            ops.push(Op::RowShift {
                rows: None,
                dx: i as i32,
                dy: 0,
                pattern: None,
                phase: 0,
                region: Some(Region::Rect(*rect)),
                axis: Some(axis.to_string()),
                wrap: Some(true),
            });
        }
        let step = (i % period) as i32;
        for rect in &treads {
            ops.push(Op::PaletteCycle {
                cycle: cycle.clone(),
                step,
                region: Some(Region::Rect(*rect)),
            });
        }
        if bob != 0 && i % 2 == 1 {
            if let Some(h) = hull {
                ops.push(Op::Shift {
                    dx: bdx,
                    dy: bdy,
                    wrap: false,
                    region: Some(Region::Rect(h)),
                });
            }
        }
        frames.push(ops);
    }
    Ok(frames)
}

pub(super) fn detect_treads(
    frame: &IndexFrame,
    palette: &Palette,
    columns: bool,
) -> PxlResult<Vec<[i32; 4]>> {
    if !palette.ramps.contains_key("tread") {
        return Err(PxlError::semantic("palette has no tread ramp")
            .with_hint("auto_tread uses ramps.tread, excluding the outline index"));
    }
    let w = frame.w as i32;
    let h = frame.h as i32;
    let mut rects = Vec::new();
    if columns {
        let edge = (w * 2 / 5).max(1);
        let mut hot = vec![false; w as usize];
        for x in 0..w {
            if x >= edge && x < w - edge {
                continue;
            }
            let n = (0..h)
                .filter(|&y| is_tread(palette, frame.get(x, y)))
                .count();
            if n >= 2 {
                hot[x as usize] = true;
            }
        }
        rects.extend(runs_to_rects(&hot, true, frame, palette));
    } else {
        let edge = (h * 2 / 5).max(1);
        let mut hot = vec![false; h as usize];
        for y in 0..h {
            if y >= edge && y < h - edge {
                continue;
            }
            let n = (0..w)
                .filter(|&x| is_tread(palette, frame.get(x, y)))
                .count();
            if n >= 2 {
                hot[y as usize] = true;
            }
        }
        rects.extend(runs_to_rects(&hot, false, frame, palette));
    }
    Ok(rects)
}

pub(super) fn is_tread(palette: &Palette, c: u8) -> bool {
    c != 0 && c != palette.special.outline && palette.in_ramp("tread", c)
}

pub(super) fn runs_to_rects(
    hot: &[bool],
    columns: bool,
    frame: &IndexFrame,
    palette: &Palette,
) -> Vec<[i32; 4]> {
    let mut rects = Vec::new();
    let mut i = 0;
    while i < hot.len() {
        if !hot[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < hot.len() && hot[i] {
            i += 1;
        }
        let end = i as i32;
        let start_i = start as i32;
        if columns {
            let mut min_y = frame.h as i32;
            let mut max_y = -1;
            for x in start_i..end {
                for y in 0..frame.h as i32 {
                    if is_tread(palette, frame.get(x, y)) {
                        min_y = min_y.min(y);
                        max_y = max_y.max(y);
                    }
                }
            }
            if max_y >= min_y {
                rects.push([start_i, min_y, end - start_i, max_y - min_y + 1]);
            }
        } else {
            let mut min_x = frame.w as i32;
            let mut max_x = -1;
            for y in start_i..end {
                for x in 0..frame.w as i32 {
                    if is_tread(palette, frame.get(x, y)) {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                    }
                }
            }
            if max_x >= min_x {
                rects.push([min_x, start_i, max_x - min_x + 1, end - start_i]);
            }
        }
    }
    rects
}

pub(super) fn hull_rect(
    frame: &IndexFrame,
    treads: &[[i32; 4]],
    dx: i32,
    dy: i32,
) -> Option<[i32; 4]> {
    let mut min_x = frame.w as i32;
    let mut min_y = frame.h as i32;
    let mut max_x = -1;
    let mut max_y = -1;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if !frame.opaque_at(x, y) {
                continue;
            }
            if treads.iter().any(|r| contains(r, x, y)) {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if max_x < 0 {
        return None;
    }
    // Grow one landing strip in the shift direction so hull pixels are not clipped.
    if dy > 0 {
        max_y = grow_span(max_y, dy, frame.h as i32 - 1, |row| {
            (min_x..=max_x).any(|x| tread_opaque(treads, frame, x, row))
        });
    } else if dy < 0 {
        min_y = shrink_span(min_y, -dy, 0, |row| {
            (min_x..=max_x).any(|x| tread_opaque(treads, frame, x, row))
        });
    }
    if dx > 0 {
        max_x = grow_span(max_x, dx, frame.w as i32 - 1, |col| {
            (min_y..=max_y).any(|y| tread_opaque(treads, frame, col, y))
        });
    } else if dx < 0 {
        min_x = shrink_span(min_x, -dx, 0, |col| {
            (min_y..=max_y).any(|y| tread_opaque(treads, frame, col, y))
        });
    }
    Some([min_x, min_y, max_x - min_x + 1, max_y - min_y + 1])
}

pub(super) fn tread_opaque(treads: &[[i32; 4]], frame: &IndexFrame, x: i32, y: i32) -> bool {
    treads.iter().any(|r| contains(r, x, y)) && frame.opaque_at(x, y)
}

pub(super) fn grow_span(
    mut edge: i32,
    steps: i32,
    limit: i32,
    blocked: impl Fn(i32) -> bool,
) -> i32 {
    for _ in 0..steps {
        if edge + 1 > limit || blocked(edge + 1) {
            break;
        }
        edge += 1;
    }
    edge
}

pub(super) fn shrink_span(
    mut edge: i32,
    steps: i32,
    limit: i32,
    blocked: impl Fn(i32) -> bool,
) -> i32 {
    for _ in 0..steps {
        if edge - 1 < limit || blocked(edge - 1) {
            break;
        }
        edge -= 1;
    }
    edge
}

pub(super) fn contains(r: &[i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

pub(super) fn idle_rumble(
    args: &BTreeMap<String, String>,
    dir: Option<&str>,
) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 2)?;
    let amp = arg_i32(args, "amp", 1)?;
    let along_x = matches!(dir, Some("E") | Some("W"));
    let mut frames = Vec::new();
    for i in 0..steps {
        if i % 2 == 0 {
            frames.push(vec![Op::Noop {}]);
        } else if along_x {
            frames.push(vec![Op::Shift {
                dx: amp,
                dy: 0,
                wrap: false,
                region: None,
            }]);
        } else {
            frames.push(vec![Op::Shift {
                dx: 0,
                dy: amp,
                wrap: false,
                region: None,
            }]);
        }
    }
    Ok(frames)
}
