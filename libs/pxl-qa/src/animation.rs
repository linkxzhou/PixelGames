use crate::common::{ratio, ratio_at_most, sample, style};
use crate::{AnimSeq, Check, QaConfig, Severity};
use pxl_core::{IndexFrame, Palette};

pub(crate) fn inspect(
    anim: &AnimSeq,
    label: &str,
    palette: &Palette,
    config: &QaConfig,
) -> Vec<Check> {
    let prefix = format!("anim.{label}");
    let mut canonical = [0usize; 256];
    for (index, slot) in canonical.iter_mut().enumerate() {
        *slot = palette
            .colors
            .get(index)
            .and_then(|color| {
                palette.colors[..index]
                    .iter()
                    .position(|other| other == color)
            })
            .unwrap_or(index);
    }
    let mut out = Vec::new();
    let n = anim.frames.len();
    if anim.static_mode {
        out.push(Check::new(
            format!("{prefix}.static_single_frame"),
            n == 1,
            n,
            "static mode: exactly 1 frame",
            Severity::Error,
        ));
        return out;
    }
    out.push(Check::new(
        format!("{prefix}.frames"),
        n >= 2,
        n,
        ">=2 frames or declare mode=static",
        Severity::Error,
    ));
    if n < 2 {
        return out;
    }
    let dimensions = anim
        .frames
        .iter()
        .all(|f| (f.w, f.h) == (anim.frames[0].w, anim.frames[0].h));
    out.push(Check::new(
        format!("{prefix}.dimensions"),
        dimensions,
        dimensions,
        "identical dimensions across sequence",
        Severity::Error,
    ));
    if !dimensions {
        return out;
    }
    let base = &anim.frames[0];
    let base_anchor = bottom_center(base);
    for (index, frame) in anim.frames.iter().enumerate() {
        let terminal_empty = !anim.looped
            && index + 1 == n
            && index > 0
            && anim
                .allow_terminal_empty
                .unwrap_or(config.allow_terminal_empty);
        let empty_ok =
            anim.allow_empty_frames.unwrap_or(config.allow_empty_frames) || terminal_empty;
        if frame.count_opaque() == 0 && empty_ok {
            continue;
        }
        if let Some(expected) = anim.anchor {
            let actual = bottom_center(frame);
            let tolerance = anim.anchor_tolerance_px.unwrap_or(0) as i32;
            let ok = actual.is_some_and(|point| {
                (point[0] - expected[0]).abs() <= tolerance
                    && (point[1] - expected[1]).abs() <= tolerance
            }) && base_anchor.is_some();
            out.push(Check::new(
                format!("{prefix}.anchor_{index}"),
                ok,
                format!("expected {expected:?}, actual {actual:?}"),
                format!("bottom-center anchor within {tolerance}px"),
                Severity::Error,
            ));
        }
        for region in &anim.protected_regions {
            let mut changed = 0usize;
            for y in region.y..region.y + region.h {
                for x in region.x..region.x + region.w {
                    if base.get(x, y) != frame.get(x, y) {
                        changed += 1;
                    }
                }
            }
            out.push(Check::new(
                format!("{prefix}.protected_{index}_{}_{}", region.x, region.y),
                changed == 0,
                changed,
                "protected region pixels remain identical",
                Severity::Error,
            ));
        }
        if !anim.change_regions.is_empty() {
            let mut outside = 0usize;
            for y in 0..frame.h as i32 {
                for x in 0..frame.w as i32 {
                    if base.get(x, y) != frame.get(x, y)
                        && !anim
                            .change_regions
                            .iter()
                            .any(|region| region.contains(x, y))
                    {
                        outside += 1;
                    }
                }
            }
            out.push(Check::new(
                format!("{prefix}.change_region_{index}"),
                outside == 0,
                outside,
                "no pixel changes outside declared change_regions",
                Severity::Error,
            ));
        }
    }
    let pairs = (0..n - 1)
        .map(|i| (i, i + 1))
        .chain(anim.looped.then_some((n - 1, 0)));
    for (a, b) in pairs {
        let (from, to) = (&anim.frames[a], &anim.frames[b]);
        let suffix = if b == 0 {
            "loop_diff".to_string()
        } else {
            format!("diff_{a}_{b}")
        };
        let visual_eq = |a: u8, b: u8| {
            a == b
                || palette
                    .colors
                    .get(a as usize)
                    .zip(palette.colors.get(b as usize))
                    .is_some_and(|(a, b)| a == b)
        };
        let changed = from
            .pixels
            .iter()
            .zip(&to.pixels)
            .filter(|(a, b)| !visual_eq(**a, **b))
            .count();
        out.push(Check::new(
            format!("{prefix}.duplicate_{a}_{b}"),
            anim.allow_duplicate_frames
                .unwrap_or(config.allow_duplicate_frames)
                || changed > 0,
            changed,
            "adjacent rendered frames differ unless holds are enabled",
            Severity::Error,
        ));
        let mut residual = changed;
        // Compare the union canvas including translated pixels outside the original
        // bounds, so clipping cannot masquerade as a lossless translation.
        for dy in -1..=1 {
            for dx in -1..=1 {
                let mut diff = 0;
                for y in -1..=from.h as i32 {
                    for x in -1..=from.w as i32 {
                        if !visual_eq(from.get(x, y), to.get(x + dx, y + dy)) {
                            diff += 1;
                        }
                    }
                }
                residual = residual.min(diff);
            }
        }
        let terminal = anim
            .allow_terminal_empty
            .unwrap_or(config.allow_terminal_empty)
            && !anim.looped
            && b == n - 1
            && to.count_opaque() == 0;
        let raw = ratio(changed, from.pixels.len());
        let adjusted = ratio(residual, from.pixels.len());
        let mut check = Check::new(
            format!("{prefix}.{suffix}"),
            terminal
                || config
                    .max_animation_diff_bps
                    .is_none_or(|v| ratio_at_most(residual, from.pixels.len(), v)),
            format!("raw={raw} bps, shift-adjusted={adjusted} bps"),
            format!("<= {:?} bps", config.max_animation_diff_bps),
            style(config),
        );
        for (i, (a, b)) in from.pixels.iter().zip(&to.pixels).enumerate() {
            if !visual_eq(*a, *b) {
                sample(
                    &mut check,
                    (i % from.w as usize) as i32,
                    (i / from.w as usize) as i32,
                    config,
                );
            }
            if check.samples.len() == config.max_samples {
                break;
            }
        }
        out.push(check);
        let area_a = from.count_opaque();
        let area_b = to.count_opaque();
        let area_change = ratio(area_a.abs_diff(area_b), area_a.max(area_b));
        out.push(Check::new(
            format!("{prefix}.area_{a}_{b}"),
            terminal
                || config
                    .max_area_change_bps
                    .is_none_or(|v| ratio_at_most(area_a.abs_diff(area_b), area_a.max(area_b), v)),
            area_change,
            format!(
                "<= {:?} bps relative to larger opaque area",
                config.max_area_change_bps
            ),
            style(config),
        ));
        if let (Some((ax, ay)), Some((bx, by))) = (centroid(from), centroid(to)) {
            let shift = ax.abs_diff(bx).max(ay.abs_diff(by));
            out.push(Check::new(
                format!("{prefix}.centroid_{a}_{b}"),
                config
                    .max_centroid_shift_millipixels
                    .is_none_or(|v| shift <= v as u64),
                shift,
                format!(
                    "<= {:?} millipixels (Chebyshev)",
                    config.max_centroid_shift_millipixels
                ),
                style(config),
            ));
        }
        let mut ha = [0usize; 256];
        let mut hb = [0usize; 256];
        for p in &from.pixels {
            ha[canonical[*p as usize]] += 1;
        }
        for p in &to.pixels {
            hb[canonical[*p as usize]] += 1;
        }
        let drift_count = ha.iter().zip(hb).map(|(a, b)| a.abs_diff(b)).sum::<usize>() / 2;
        let drift = ratio(drift_count, from.pixels.len());
        out.push(Check::new(
            format!("{prefix}.palette_drift_{a}_{b}"),
            terminal
                || config
                    .max_palette_change_bps
                    .is_none_or(|v| ratio_at_most(drift_count, from.pixels.len(), v)),
            drift,
            format!(
                "<= {:?} bps histogram distance",
                config.max_palette_change_bps
            ),
            style(config),
        ));
    }
    out
}

fn bottom_center(frame: &IndexFrame) -> Option<[i32; 2]> {
    let mut min_x = frame.w as i32;
    let mut max_x = -1i32;
    let mut bottom = -1i32;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if frame.get(x, y) != 0 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    (max_x >= 0).then_some([(min_x + max_x) / 2, bottom])
}

fn centroid(frame: &IndexFrame) -> Option<(u64, u64)> {
    let (mut sx, mut sy, mut n) = (0u64, 0u64, 0u64);
    for (i, c) in frame.pixels.iter().enumerate() {
        if *c != 0 {
            sx += (i % frame.w as usize) as u64;
            sy += (i / frame.w as usize) as u64;
            n += 1;
        }
    }
    (n > 0).then(|| (sx * 1000 / n, sy * 1000 / n))
}
