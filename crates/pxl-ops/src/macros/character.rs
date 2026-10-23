use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlError, PxlResult};
use pxl_format::{Op, Region};

use super::args::{arg_i32, steps_of};

pub(super) fn idle_breathe(
    args: &BTreeMap<String, String>,
    frame: &IndexFrame,
) -> PxlResult<Vec<Vec<Op>>> {
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

pub(super) fn bob(args: &BTreeMap<String, String>) -> PxlResult<Vec<Vec<Op>>> {
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

pub(super) fn walk_side(
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

/// Close the eyes inside the rect `x,y,w,h` by painting its opaque pixels with
/// the lid colour. `lid` defaults to the most common surrounding skin tone: the
/// commonest opaque, non-outline colour on the ring just outside the rect.
pub(super) fn blink(
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

pub(super) fn ring_color(frame: &IndexFrame, rect: [i32; 4], palette: &Palette) -> Option<u8> {
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
