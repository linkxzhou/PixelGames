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

/// Arguments each macro reads. A name outside this list is a typo or an option
/// the macro does not implement, and silently ignoring it hides the mistake.
fn allowed_args(name: &str) -> &'static [&'static str] {
    match name {
        "tread_cycle" => &["steps", "cycle", "bob", "region"],
        "idle_rumble" => &["steps", "amp"],
        "idle_breathe" => &["steps", "period", "amp"],
        "bob" => &["amp"],
        "walk_side" | "run_side" => &["stride", "bob"],
        "hurt_flash" => &["steps"],
        "death_dissolve" => &["steps", "seed", "pattern"],
        "blink" => &["x", "y", "w", "h", "lid"],
        "spin_projectile" => &["steps"],
        "pulse_glow" => &["ramp"],
        "doll" => &["anim"],
        _ => &[],
    }
}

fn check_args(name: &str, args: &BTreeMap<String, String>) -> PxlResult<()> {
    let allowed = allowed_args(name);
    for key in args.keys() {
        if !allowed.contains(&key.as_str()) {
            let hint = if allowed.is_empty() {
                format!("{name} takes no arguments")
            } else {
                format!("{name} takes: {}", allowed.join(", "))
            };
            return Err(
                PxlError::schema(format!("macro {name} has no argument '{key}'"))
                    .with_path(format!("/{key}"))
                    .with_hint(hint),
            );
        }
    }
    Ok(())
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
    check_args(&name, &args)?;
    match name.as_str() {
        "tread_cycle" => tread_cycle(&args, dir, frame, palette),
        "idle_rumble" => idle_rumble(&args, dir),
        "idle_breathe" => idle_breathe(&args, frame),
        "bob" => bob(&args),
        "walk_side" => walk_side(&args, frame, false),
        "run_side" => walk_side(&args, frame, true),
        "hurt_flash" => hurt_flash(&args),
        "death_dissolve" => death_dissolve(&args),
        "blink" => blink(&args, frame, palette),
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
    // `period` is the documented name for the frame count; `steps` stays valid.
    let period = if args.contains_key("period") {
        let n = arg_i32(args, "period", 4)?;
        if !(1..=16).contains(&n) {
            return Err(
                PxlError::schema(format!("period {n} is outside 1..=16")).with_path("/period")
            );
        }
        n as usize
    } else {
        steps_of(args, 4)?
    };
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

/// Frame 0 is the untouched sprite and the last frame is empty. `steps` is the
/// frame count, so a 6-frame fade removes 1/5 of the sprite per frame.
fn death_dissolve(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 6)?;
    if steps < 2 {
        return Err(PxlError::schema("death_dissolve needs steps >= 2")
            .with_path("/steps")
            .with_hint("one frame cannot show both the sprite and its absence"));
    }
    let seed = arg_i32(args, "seed", 7)? as u64;
    let pattern = arg_str(args, "pattern", "bayer4").to_string();
    let transitions = (steps - 1) as u32;
    let mut frames = Vec::new();
    for i in 0..steps {
        frames.push(vec![Op::Dissolve {
            seed,
            steps: transitions,
            step: i as u32,
            pattern: pattern.clone(),
        }]);
    }
    Ok(frames)
}

/// Close the eyes inside the rect `x,y,w,h` by painting its opaque pixels with
/// the lid colour. `lid` defaults to the most common surrounding skin tone: the
/// commonest opaque, non-outline colour on the ring just outside the rect.
fn blink(
    args: &BTreeMap<String, String>,
    frame: &IndexFrame,
    palette: &Palette,
) -> PxlResult<Vec<Vec<Op>>> {
    let mut rect = [0i32; 4];
    for (slot, key) in ["x", "y", "w", "h"].iter().enumerate() {
        if !args.contains_key(*key) {
            return Err(PxlError::schema(format!("blink needs {key}"))
                .with_path(format!("/{key}"))
                .with_hint("blink(x=5, y=6, w=4, h=1) names the eye rect to close"));
        }
        rect[slot] = arg_i32(args, key, 0)?;
    }
    let [x, y, w, h] = rect;
    if w < 1 || h < 1 || x < 0 || y < 0 || x + w > frame.w as i32 || y + h > frame.h as i32 {
        return Err(PxlError::schema(format!(
            "blink rect [{x},{y},{w},{h}] is outside the {}x{} frame",
            frame.w, frame.h
        ))
        .with_hint("x,y,w,h must lie inside the sprite"));
    }
    let eyes_open = (y..y + h).any(|py| (x..x + w).any(|px| frame.opaque_at(px, py)));
    if !eyes_open {
        return Err(PxlError::semantic("blink rect holds no opaque pixels")
            .with_hint("point x,y,w,h at the eyes of the source frame"));
    }
    let lid = match args.get("lid") {
        Some(name) => palette.resolve_color_name(name)?,
        None => ring_color(frame, rect, palette).ok_or_else(|| {
            PxlError::semantic("blink found no skin colour around the eyes")
                .with_hint("pass lid=<palette index>")
        })?,
    };
    Ok(vec![
        vec![Op::Noop {}],
        vec![Op::PaletteSwap {
            map: format!("all->{lid}"),
            region: Some(Region::Rect(rect)),
        }],
    ])
}

fn ring_color(frame: &IndexFrame, rect: [i32; 4], palette: &Palette) -> Option<u8> {
    let [x, y, w, h] = rect;
    let mut counts = [0u32; 256];
    for py in y - 1..=y + h {
        for px in x - 1..=x + w {
            let inside = px >= x && px < x + w && py >= y && py < y + h;
            if inside || !frame.in_bounds(px, py) {
                continue;
            }
            let c = frame.get(px, py);
            if c != 0 && c != palette.special.outline {
                counts[c as usize] += 1;
            }
        }
    }
    let mut best: Option<(u32, u8)> = None;
    for (c, n) in counts.iter().enumerate() {
        if *n > 0 && best.map_or(true, |(bn, _)| *n > bn) {
            best = Some((*n, c as u8));
        }
    }
    best.map(|(_, c)| c)
}

/// Four poses: 0, 90, 180, 270 degrees. Finer steps need arbitrary-angle
/// rotation, which `rotate` does not implement yet, so they are refused rather
/// than faked with repeated frames.
fn spin_projectile(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
    let steps = steps_of(args, 4)?;
    if steps != 4 {
        return Err(
            PxlError::schema(format!("spin_projectile steps={steps} is not supported"))
                .with_path("/steps")
                .with_hint("only steps=4 (0/90/180/270 degrees); 8 poses need 45-degree rotation"),
        );
    }
    let mut frames = Vec::new();
    for turns in 0..steps {
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::{apply_ops, ApplyCx};

    fn pal() -> Palette {
        pxl_gen::builtin_palette("tank").unwrap()
    }

    fn grid(rows: &[&str]) -> IndexFrame {
        let owned: Vec<String> = rows.iter().map(|s| (*s).to_string()).collect();
        pxl_core::decode_matrix(&owned).unwrap()
    }

    fn run(frame: &IndexFrame, ops: &[Op], pal: &Palette) -> IndexFrame {
        let cx = ApplyCx {
            palette: pal,
            seed: 1,
            frame_name: "f",
            load_template: None,
            parts: None,
            pose: RefCell::new(BTreeMap::new()),
        };
        apply_ops(frame, ops, &cx).unwrap()
    }

    fn expand(spec: &str, frame: &IndexFrame) -> PxlResult<Vec<Vec<Op>>> {
        expand_macro(spec, None, frame, &pal(), None)
    }

    #[test]
    fn unknown_macro_argument_is_an_error_not_a_silent_default() {
        let f = grid(&["22", "22"]);
        let err = expand("idle_rumble(step=2)", &f).unwrap_err();
        assert!(err.message.contains("step"), "{err:?}");
        let err = expand("hurt_flash(amount=3)", &f).unwrap_err();
        assert!(err.message.contains("amount"), "{err:?}");
        // The documented spellings keep working.
        assert!(expand("idle_rumble(steps=2, amp=1)", &f).is_ok());
        assert!(expand("idle_breathe(period=4)", &f).is_ok());
    }

    #[test]
    fn idle_breathe_honours_period() {
        let f = grid(&["22", "22"]);
        assert_eq!(expand("idle_breathe(period=3)", &f).unwrap().len(), 3);
        assert_eq!(expand("idle_breathe(steps=5)", &f).unwrap().len(), 5);
        assert!(expand("idle_breathe(period=0)", &f).is_err());
    }

    #[test]
    fn death_dissolve_starts_whole_and_ends_empty() {
        let pal = pal();
        let f = grid(&["2222", "2222", "2222", "2222"]);
        for steps in [2, 6, 16] {
            let frames = expand(&format!("death_dissolve(steps={steps})"), &f).unwrap();
            assert_eq!(frames.len(), steps);
            let first = run(&f, &frames[0], &pal);
            let last = run(&f, frames.last().unwrap(), &pal);
            assert_eq!(
                first.pixels, f.pixels,
                "steps={steps}: first frame is the sprite"
            );
            assert_eq!(last.count_opaque(), 0, "steps={steps}: last frame is empty");
        }
        assert!(expand("death_dissolve(steps=1)", &f).is_err());
    }

    #[test]
    fn blink_closes_only_the_named_eye_rect() {
        let pal = pal();
        // Skin 3 around two eye pixels coloured 11.
        let f = grid(&["3333", "3B3B", "3333"]);
        let frames = expand("blink(x=0, y=1, w=4, h=1)", &f).unwrap();
        assert_eq!(frames.len(), 2);
        let open = run(&f, &frames[0], &pal);
        let shut = run(&f, &frames[1], &pal);
        assert_eq!(open.pixels, f.pixels);
        assert_ne!(shut.pixels, f.pixels, "closed eyes must differ");
        // Rows outside the rect never change.
        for x in 0..4 {
            assert_eq!(shut.get(x, 0), f.get(x, 0));
            assert_eq!(shut.get(x, 2), f.get(x, 2));
        }
        // The lid takes the surrounding skin colour.
        assert!((0..4).all(|x| shut.get(x, 1) == 3));
    }

    #[test]
    fn blink_needs_a_rect_inside_the_frame() {
        let f = grid(&["33", "33"]);
        assert!(expand("blink", &f).is_err());
        assert!(expand("blink(x=0, y=0, w=9, h=1)", &f).is_err());
    }

    #[test]
    fn spin_projectile_only_offers_four_distinct_poses() {
        let pal = pal();
        let f = grid(&["2222", "2...", "2...", "...."]);
        let frames = expand("spin_projectile(steps=4)", &f).unwrap();
        let poses: Vec<_> = frames.iter().map(|o| run(&f, o, &pal).pixels).collect();
        for i in 0..poses.len() {
            for j in i + 1..poses.len() {
                assert_ne!(poses[i], poses[j], "poses {i} and {j} repeat");
            }
        }
        assert!(expand("spin_projectile(steps=8)", &f).is_err());
    }
}
