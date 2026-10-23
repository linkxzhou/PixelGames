use crate::model::{Check, ValidateInput};

/// A tile (`kind: tiles`) is a full-bleed surface: placed on a 16px grid it
/// must not show a transparent band. Padding a tile to pass sprite bbox rules
/// is exactly the v5 defect this catches. Frames that only appear in one-shot
/// animations after their first frame (a brick breaking) may lose material;
/// decorations that sit on top of other tiles should be declared `overlay`.
pub(crate) fn tile_coverage(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    for (name, frame) in &input.tiles {
        let mut uses = 0usize;
        let mut transitional = true;
        for anim in &input.anims {
            for (i, n) in anim.frame_names.iter().enumerate() {
                if n != name {
                    continue;
                }
                uses += 1;
                if anim.looped || anim.static_mode || i == 0 {
                    transitional = false;
                }
            }
        }
        if uses > 0 && transitional {
            continue;
        }
        let w = frame.w as i32;
        let h = frame.h as i32;
        let mut clear_edge = 0usize;
        for x in 0..w {
            clear_edge += usize::from(!frame.opaque_at(x, 0));
            clear_edge += usize::from(!frame.opaque_at(x, h - 1));
        }
        for y in 1..h - 1 {
            clear_edge += usize::from(!frame.opaque_at(0, y));
            clear_edge += usize::from(!frame.opaque_at(w - 1, y));
        }
        out.push(Check {
            id: format!("tile.{name}.full_bleed"),
            pass: clear_edge == 0,
            value: format!("{clear_edge} clear edge pixels"),
            limit: "0 transparent pixels on the tile border (declare kind=overlay for decorations)"
                .into(),
        });
    }
    out
}

pub(crate) fn tile_checks(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    out.push(Check {
        id: "tiles.count".into(),
        pass: input.tiles.len() >= 4,
        value: input.tiles.len().to_string(),
        limit: ">= 4".into(),
    });
    out.extend(tile_seams(input));
    out
}

pub(crate) fn tile_seams(input: &ValidateInput) -> Vec<Check> {
    let mut out = Vec::new();
    for (name, frame) in &input.tiles {
        let mut worst = 0usize;
        let mut bad = 0usize;
        let mut checked = 0usize;
        let w = frame.w as i32;
        let h = frame.h as i32;
        let ramp = input.ramp_for(name);
        let on_ramp = |i: u8| input.palette.in_ramp(ramp, i);
        // Compare opposite edges only where both pixels sit on the declared
        // ramp. A layered tile (grass over dirt, an ore vein in stone) puts
        // several materials on the same edge; those bands are exempt from the
        // one-ramp rule instead of failing as "off-ramp".
        //
        // Left and right edges always meet the same tile, so a mixed pair there
        // is a broken edge. Top and bottom edges of a surface tile meet the
        // material below instead: when the entire top row is one material and
        // the entire bottom row is the other, that junction is a deliberate
        // layer boundary and is exempt. A ragged boundary still fails - that is
        // a stray pixel, not a layer.
        let layer_boundary =
            (0..w).all(|x| on_ramp(frame.get(x, 0)) != on_ramp(frame.get(x, h - 1)));
        let mut pairs = Vec::new();
        for y in 0..h {
            pairs.push((frame.get(0, y), frame.get(w - 1, y)));
        }
        let mut layered = 0usize;
        if layer_boundary {
            layered = w as usize;
        } else {
            for x in 0..w {
                pairs.push((frame.get(x, 0), frame.get(x, h - 1)));
            }
        }
        for (a, b) in pairs {
            let a_on = on_ramp(a);
            let b_on = on_ramp(b);
            if !a_on && !b_on {
                // Both sides are another material (grass over dirt, a vein).
                // A layered tile may cross ramps; skipping keeps it honest.
                continue;
            }
            if a_on != b_on {
                // One side of the seam belongs to the tile material and the
                // other does not: that is a hard edge, not a texture step.
                bad += 1;
                continue;
            }
            checked += 1;
            match input.palette.ramp_distance(ramp, a, b) {
                Some(d) if d <= 1 => worst = worst.max(d),
                _ => bad += 1,
            }
        }
        if checked == 0 {
            out.push(Check {
                id: format!("tile.{name}.seam"),
                pass: false,
                value: "no edge pixel on the declared ramp".into(),
                limit: format!("edge pixels on ramp '{ramp}'"),
            });
            continue;
        }
        let worst = worst.max(bad.min(1));
        out.push(Check {
            id: format!("tile.{name}.seam"),
            pass: bad == 0,
            value: if layered == 0 {
                format!("{checked} pairs, {bad} over one step, worst {worst}")
            } else {
                format!("{checked} pairs, {bad} over one step, worst {worst}, {layered} on a layer boundary")
            },
            limit: format!("opposite edge pixels on '{ramp}' match within one step"),
        });
    }
    out
}
