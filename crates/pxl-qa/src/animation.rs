use pxl_core::IndexFrame;

use crate::common::{fail, fmt_pct, pct_bps};
use crate::model::{AnimSeq, Check};

/// Check ids for the first asset that owns an animation name stay as they were
/// (`anim.move.N.tread_step_0`). Later assets reuse the name, so their ids carry
/// the display key (`anim.tank2_move.N.tread_step_0`) and cannot collide.
pub(crate) fn scoped(anim: &AnimSeq, checks: Vec<Check>) -> Vec<Check> {
    if anim.name == anim.base_name {
        return checks;
    }
    let from = format!(".{}.", anim.base_name);
    let to = format!(".{}.", anim.name);
    checks
        .into_iter()
        .map(|mut c| {
            if let Some(rest) = c.id.strip_prefix("anim.") {
                if rest.starts_with(&from[1..]) {
                    c.id = format!("anim{to}{}", &rest[from.len() - 1..]);
                }
            }
            c
        })
        .collect()
}

pub(crate) fn anim_diff(
    anim: &AnimSeq,
    min_bps: u32,
    max_bps: u32,
    include_loop: bool,
) -> Vec<Check> {
    let mut out = Vec::new();
    let label = match &anim.dir {
        Some(d) => format!("{}.{}", anim.name, d),
        None => anim.name.clone(),
    };
    // A declared static anim intentionally has one frame; requiring two would
    // make legal single-frame idle/fly assets unbuildable.
    if anim.static_mode {
        out.push(Check {
            id: format!("anim.{label}.static_single_frame"),
            pass: anim.frames.len() == 1,
            value: anim.frames.len().to_string(),
            limit: "declared static: exactly 1 frame".into(),
        });
        if anim.frames.len() < 2 {
            return out;
        }
        // Declared static but given more frames is a spec error, not a pass.
        out.push(fail(
            &format!("anim.{label}.static_frames"),
            &anim.frames.len().to_string(),
            "static anim must declare exactly 1 frame",
        ));
        return out;
    }
    if anim.frames.len() < 2 {
        out.push(fail(
            &format!("anim.{label}.frames"),
            &anim.frames.len().to_string(),
            ">= 2 frames (or declare mode=static in the recipe)",
        ));
        return out;
    }
    let total = (anim.frames[0].w as usize) * (anim.frames[0].h as usize);
    let mut pairs = Vec::new();
    for i in 0..anim.frames.len() - 1 {
        pairs.push((i, i + 1));
    }
    if include_loop && anim.looped {
        pairs.push((anim.frames.len() - 1, 0));
    }
    for (a, b) in pairs {
        let from = &anim.frames[a];
        let to = &anim.frames[b];
        let changed = from.diff_count(to);
        let bps = pct_bps(changed, total);
        // Reaching a clear end state erases the sprite in one step; only the
        // lower bound (the frames must differ) still applies there. The end
        // state is the declared last frame of a one-shot anim, by identity.
        let to_clear_end = !anim.looped
            && b == anim.frames.len() - 1
            && anim.frames[b].count_opaque() == 0
            && anim.frame_names.len() == anim.frames.len();
        let max_bps = if to_clear_end { 10_000 } else { max_bps };
        // A whole-frame pixel step changes every cell along the silhouette, so a
        // smooth 1px bob scores like a hard cut when counted per cell. Too small
        // is still a failure (the sprite is not moving); too large is a failure
        // only when no integer shift explains it.
        let residual = shift_residual_bps(from, to, total);
        let id = if b == 0 {
            format!("anim.{label}.loop_diff")
        } else {
            format!("anim.{label}.diff_{a}_{b}")
        };
        let pass = bps >= min_bps && (bps <= max_bps || residual <= max_bps);
        let value = if residual < bps {
            format!("{} (shift-adjusted {})", fmt_pct(bps), fmt_pct(residual))
        } else {
            fmt_pct(bps)
        };
        out.push(Check {
            id,
            pass,
            value,
            limit: format!(
                "at least {}, and at most {} unless a 1px shift explains it",
                fmt_pct(min_bps),
                fmt_pct(max_bps)
            ),
        });
    }
    out
}

/// Lifecycle of a one-shot effect (burst, explosion): it grows from its first
/// frame to a peak, then decays, and every visible frame stays centred on the
/// first frame's anchor. Looping effects (flames, sparkles) have no lifecycle
/// and are left to the generic frame-difference rule.
pub(crate) fn effect_lifecycle(anim: &AnimSeq) -> Vec<Check> {
    if anim.looped || anim.static_mode || anim.frames.len() < 2 {
        return Vec::new();
    }
    let label = match &anim.dir {
        Some(d) => format!("{}.{}", anim.name, d),
        None => anim.name.clone(),
    };
    let areas: Vec<usize> = anim.frames.iter().map(|f| f.count_opaque()).collect();
    let peak = areas.iter().copied().max().unwrap_or(0);
    let peak_at = areas.iter().position(|a| *a == peak).unwrap_or(0);
    let last = *areas.last().unwrap_or(&0);
    let curve = areas
        .iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(">");
    let mut out = vec![
        Check {
            id: format!("anim.{label}.effect_grows"),
            pass: peak_at > 0 && areas[0] < peak,
            value: curve.clone(),
            limit: "opaque area peaks after the first frame".into(),
        },
        Check {
            id: format!("anim.{label}.effect_decays"),
            pass: last < peak,
            value: curve,
            limit: "last frame smaller than the peak".into(),
        },
    ];
    let centre = |f: &IndexFrame| {
        f.bbox()
            .map(|b| (2 * b.x + b.w - 1, 2 * b.y + b.h - 1))
    };
    if let Some(anchor) = centre(&anim.frames[0]) {
        let w = anim.frames[0].w as i32;
        let h = anim.frames[0].h as i32;
        // Doubled coordinates keep half-pixel centres exact. Allow drift of
        // an eighth of the canvas (at least 2px) before calling it unanchored.
        let tol = 2 * (w.max(h) / 8).max(2);
        let mut worst = 0;
        for f in &anim.frames {
            if let Some((cx, cy)) = centre(f) {
                worst = worst.max((cx - anchor.0).abs()).max((cy - anchor.1).abs());
            }
        }
        out.push(Check {
            id: format!("anim.{label}.effect_anchor"),
            pass: worst <= tol,
            value: format!("{}px", worst as f32 / 2.0),
            limit: format!("bbox centre within {}px of the first frame", tol / 2),
        });
    }
    out
}

/// Smallest cell-difference left after translating `to` by -1..=1 px on each
/// axis, as a percentage of the canvas. Uncorrelated changes stay large.
pub(crate) fn shift_residual_bps(from: &IndexFrame, to: &IndexFrame, total: usize) -> u32 {
    let mut best = from.diff_count(to);
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let mut changed = 0usize;
            for y in 0..from.h as i32 {
                for x in 0..from.w as i32 {
                    if from.get(x, y) != to.get(x + dx, y + dy) {
                        changed += 1;
                    }
                }
            }
            best = best.min(changed);
        }
    }
    pct_bps(best, total)
}
