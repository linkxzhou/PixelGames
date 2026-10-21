//! Animation macros expand to concrete op lists. Each frame's ops are applied
//! to the source frame independently (they do not accumulate).

use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::{Op, PartsFile, Region};

pub const MACRO_NAMES: &[&str] = &[
    "tread_cycle",
    "idle_rumble",
    "idle_breathe",
    "bob",
    "walk_side",
    "run_side",
    "hurt_flash",
    "death_dissolve",
    "blink",
    "spin_projectile",
    "pulse_glow",
    "doll",
];

pub fn parse_macro_call(spec: &str) -> PxlResult<(String, BTreeMap<String, String>)> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(PxlError::usage("missing macro").with_hint("example: tread_cycle(steps=4)"));
    }
    let (name, rest) = if let Some(open) = spec.find('(') {
        if !spec.ends_with(')') {
            return Err(PxlError::schema(format!("macro '{spec}' is missing ')'"))
                .with_hint("write name(arg=value, arg=value)"));
        }
        (&spec[..open], &spec[open + 1..spec.len() - 1])
    } else {
        (spec, "")
    };
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(PxlError::schema(format!("bad macro name '{name}'")));
    }
    let mut args = BTreeMap::new();
    if !rest.trim().is_empty() {
        for part in rest.split(',') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                return Err(
                    PxlError::schema(format!("macro arg '{part}' needs key=value"))
                        .with_hint("example: steps=4, region=auto_tread"),
                );
            };
            args.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Ok((name.to_string(), args))
}

pub fn expand_macro(
    spec: &str,
    dir: Option<&str>,
    frame: &IndexFrame,
    palette: &Palette,
    parts: Option<&PartsFile>,
) -> PxlResult<Vec<Vec<Op>>> {
    let (name, args) = parse_macro_call(spec)?;
    if !MACRO_NAMES.contains(&name.as_str()) {
        return Err(PxlError::semantic(format!("unknown macro '{name}'"))
            .with_hint("run `pxl list macros`"));
    }
    match name.as_str() {
        "tread_cycle" => tread_cycle(&args, dir, frame, palette),
        "idle_rumble" => idle_rumble(&args, dir),
        "idle_breathe" => idle_breathe(&args, frame),
        "bob" => bob(&args),
        "walk_side" => walk_side(&args, frame, false),
        "run_side" => walk_side(&args, frame, true),
        "hurt_flash" => hurt_flash(&args),
        "death_dissolve" => death_dissolve(&args),
        "blink" => blink(&args, frame),
        "spin_projectile" => spin_projectile(&args),
        "pulse_glow" => pulse_glow(&args),
        "doll" => doll(&args, dir, parts),
        _ => unreachable!(),
    }
}

fn doll(
    args: &BTreeMap<String, String>,
    dir: Option<&str>,
    parts: Option<&PartsFile>,
) -> PxlResult<Vec<Vec<Op>>> {
    let rig = parts.ok_or_else(|| {
        PxlError::semantic("doll() needs a parts spec")
            .with_hint("set the sprite base to gen parts and pass --spec farmer.parts.json")
    })?;
    let anim = arg_str(args, "anim", "idle");
    let facing = dir.unwrap_or("down");
    let (frames, order) = rig.anim_pose(anim, facing)?;
    if frames.is_empty() || frames.len() > 16 {
        return Err(
            PxlError::schema(format!("doll anim '{anim}' has {} frames", frames.len()))
                .with_hint("a doll anim has 1..=16 keyframes"),
        );
    }
    let mut out = Vec::with_capacity(frames.len());
    for frame_pose in frames {
        let mut ops = Vec::new();
        for (part, key) in &frame_pose {
            rig.part(part)?;
            let identity = key.dx == 0 && key.dy == 0 && key.rot90.is_none() && key.flip.is_none();
            if identity {
                continue;
            }
            ops.push(Op::PartTransform {
                part: part.clone(),
                dx: key.dx,
                dy: key.dy,
                rot90: key.rot90.clone(),
                flip: key.flip.clone(),
            });
        }
        ops.push(Op::Compose {
            parts: order.clone(),
            dir: Some(facing.to_string()),
        });
        out.push(ops);
    }
    Ok(out)
}

fn arg_i32(args: &BTreeMap<String, String>, key: &str, default: i32) -> PxlResult<i32> {
    match args.get(key) {
        None => Ok(default),
        Some(v) => v.parse::<i32>().map_err(|_| {
            PxlError::schema(format!("macro arg {key}={v} is not an integer"))
                .with_path(format!("/{key}"))
        }),
    }
}

fn arg_str<'a>(args: &'a BTreeMap<String, String>, key: &str, default: &'a str) -> &'a str {
    args.get(key).map(|s| s.as_str()).unwrap_or(default)
}

fn steps_of(args: &BTreeMap<String, String>, default: i32) -> PxlResult<usize> {
    let n = arg_i32(args, "steps", default)?;
    if !(1..=16).contains(&n) {
        return Err(PxlError::schema(format!("steps {n} is outside 1..=16"))
            .with_hint("an animation has at most 16 frames"));
    }
    Ok(n as usize)
}

fn tread_cycle(
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

fn detect_treads(frame: &IndexFrame, palette: &Palette, columns: bool) -> PxlResult<Vec<[i32; 4]>> {
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

fn is_tread(palette: &Palette, c: u8) -> bool {
    c != 0 && c != palette.special.outline && palette.in_ramp("tread", c)
}

fn runs_to_rects(
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

fn hull_rect(frame: &IndexFrame, treads: &[[i32; 4]], dx: i32, dy: i32) -> Option<[i32; 4]> {
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

fn tread_opaque(treads: &[[i32; 4]], frame: &IndexFrame, x: i32, y: i32) -> bool {
    treads.iter().any(|r| contains(r, x, y)) && frame.opaque_at(x, y)
}

fn grow_span(mut edge: i32, steps: i32, limit: i32, blocked: impl Fn(i32) -> bool) -> i32 {
    for _ in 0..steps {
        if edge + 1 > limit || blocked(edge + 1) {
            break;
        }
        edge += 1;
    }
    edge
}

fn shrink_span(mut edge: i32, steps: i32, limit: i32, blocked: impl Fn(i32) -> bool) -> i32 {
    for _ in 0..steps {
        if edge - 1 < limit || blocked(edge - 1) {
            break;
        }
        edge -= 1;
    }
    edge
}

fn contains(r: &[i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

fn idle_rumble(args: &BTreeMap<String, String>, dir: Option<&str>) -> PxlResult<Vec<Vec<Op>>> {
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

fn idle_breathe(args: &BTreeMap<String, String>, frame: &IndexFrame) -> PxlResult<Vec<Vec<Op>>> {
    let period = steps_of(args, 4)?;
    let amp = arg_i32(args, "amp", 1)?;
    let mut frames = Vec::new();
    for i in 0..period {
        frames.push(vec![Op::RowShift {
            rows: Some([0, frame.h as i32 - 1]),
            dx: amp,
            dy: 0,
            pattern: Some("sine4".into()),
            phase: i as i32,
            region: None,
            axis: None,
            wrap: Some(false),
        }]);
    }
    Ok(frames)
}

fn bob(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let amp = arg_i32(args, "amp", 1)?;
    let seq = [0, -amp, 0, amp];
    Ok(seq
        .into_iter()
        .map(|dy| {
            vec![Op::Shift {
                dx: 0,
                dy,
                wrap: false,
                region: None,
            }]
        })
        .collect())
}

fn walk_side(
    args: &BTreeMap<String, String>,
    frame: &IndexFrame,
    run: bool,
) -> PxlResult<Vec<Vec<Op>>> {
    let stride = arg_i32(args, "stride", if run { 2 } else { 1 })?;
    let bob_amp = arg_i32(args, "bob", 1)?;
    let leg_y = frame.h as i32 / 2;
    let leg = [0, leg_y, frame.w as i32, frame.h as i32 - leg_y];
    let mut frames = Vec::new();
    for i in 0..4 {
        let dx = if i % 2 == 0 { stride } else { -stride };
        let dy = if i % 2 == 0 { 0 } else { -bob_amp };
        frames.push(vec![
            Op::Shift {
                dx,
                dy: 0,
                wrap: false,
                region: Some(Region::Rect(leg)),
            },
            Op::Shift {
                dx: 0,
                dy,
                wrap: false,
                region: None,
            },
        ]);
    }
    Ok(frames)
}

fn hurt_flash(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let n = steps_of(args, 2)?;
    let mut frames = Vec::new();
    for i in 0..n {
        if i % 2 == 0 {
            frames.push(vec![Op::PaletteSwap {
                map: "all->flash".into(),
                region: None,
            }]);
        } else {
            frames.push(vec![Op::Noop {}]);
        }
    }
    Ok(frames)
}

fn death_dissolve(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 6)?;
    let seed = arg_i32(args, "seed", 7)? as u64;
    let pattern = arg_str(args, "pattern", "bayer4").to_string();
    let mut frames = Vec::new();
    for i in 0..steps {
        frames.push(vec![Op::Dissolve {
            seed,
            steps: steps as u32,
            step: i as u32,
            pattern: pattern.clone(),
        }]);
    }
    Ok(frames)
}

fn blink(args: &BTreeMap<String, String>, frame: &IndexFrame) -> PxlResult<Vec<Vec<Op>>> {
    let _ = arg_i32(args, "amp", 1)?;
    let top = [0, 0, frame.w as i32, (frame.h as i32 / 3).max(1)];
    Ok(vec![
        vec![Op::Noop {}],
        vec![Op::Shade {
            dir: "top".into(),
            amount: 1,
            region: Some(Region::Rect(top)),
        }],
    ])
}

fn spin_projectile(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 4)?;
    if steps != 4 && steps != 8 {
        return Err(PxlError::schema("spin_projectile steps must be 4 or 8"));
    }
    let mut frames = Vec::new();
    for i in 0..steps {
        let turns = if steps == 8 { i / 2 } else { i };
        if turns == 0 {
            frames.push(vec![Op::Noop {}]);
        } else {
            frames.push(
                (0..turns)
                    .map(|_| Op::Rotate90 { dir: "cw".into() })
                    .collect(),
            );
        }
    }
    Ok(frames)
}

fn pulse_glow(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let ramp = arg_str(args, "ramp", "olive");
    let map = format!("ramp:{ramp}+1");
    Ok(vec![
        vec![Op::Noop {}],
        vec![Op::PaletteSwap { map, region: None }],
    ])
}
