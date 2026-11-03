use crate::{Check, PixelLocation, QaConfig, Severity};
use pxl_core::{IndexFrame, Palette, Rgba};

pub(crate) fn ratio(n: usize, total: usize) -> u32 {
    if total == 0 {
        0
    } else {
        (n as u64 * 10000 / total as u64) as u32
    }
}

/// Compare exact counts; rounded basis points are for display only.
pub(crate) fn ratio_at_most(n: usize, total: usize, limit: u32) -> bool {
    n as u128 * 10000 <= total as u128 * limit as u128
}

pub(crate) fn ratio_at_least(n: usize, total: usize, limit: u32) -> bool {
    if total == 0 {
        limit == 0
    } else {
        n as u128 * 10000 >= total as u128 * limit as u128
    }
}

pub(crate) fn style(config: &QaConfig) -> Severity {
    if config.strict {
        Severity::Error
    } else {
        Severity::Warning
    }
}

pub(crate) fn delta(a: Rgba, b: Rgba) -> u32 {
    u32::from(
        a.r.abs_diff(b.r)
            .max(a.g.abs_diff(b.g))
            .max(a.b.abs_diff(b.b))
            .max(a.a.abs_diff(b.a)),
    )
}

pub(crate) fn sample(check: &mut Check, x: i32, y: i32, config: &QaConfig) {
    if check.samples.len() < config.max_samples {
        check.samples.push(PixelLocation {
            x: x as u16,
            y: y as u16,
        });
    }
}

pub(crate) fn valid_frame(frame: &IndexFrame, config: &QaConfig) -> bool {
    frame.w > 0
        && frame.h > 0
        && frame.pixels.len() == frame.w as usize * frame.h as usize
        && frame.pixels.len() <= config.max_frame_pixels
}

pub(crate) fn palette_checks(palette: &Palette, config: &QaConfig) -> Vec<Check> {
    let mut out = vec![
        Check::new(
            "palette.size",
            (1..=256).contains(&palette.len()),
            palette.len(),
            "1..=256 entries",
            Severity::Error,
        ),
        Check::new(
            "palette.alpha",
            palette.colors.first().is_some_and(|c| c.a == 0)
                && palette.colors.iter().skip(1).all(|c| c.a == 255),
            "indexed alpha",
            "only index 0 transparent; all other entries opaque",
            Severity::Error,
        ),
        Check::new(
            "palette.transparent",
            palette.special.transparent == 0,
            palette.special.transparent,
            "transparent index 0",
            Severity::Error,
        ),
    ];
    for (name, indices) in palette
        .ramps
        .iter()
        .map(|(n, r)| (format!("ramp.{n}"), r))
        .chain(
            palette
                .cycles
                .iter()
                .map(|(n, c)| (format!("cycle.{n}"), &c.indices)),
        )
    {
        let mut seen = [false; 256];
        let valid = !indices.is_empty()
            && indices.iter().all(|i| {
                let i = *i as usize;
                let ok = i > 0 && i < palette.len() && !seen[i];
                seen[i] = true;
                ok
            });
        out.push(Check::new(
            format!("palette.{name}.indices"),
            valid,
            format!(
                "{} entries, sample {:?}",
                indices.len(),
                &indices[..indices.len().min(config.max_samples)]
            ),
            "nonempty, unique, opaque indices within palette",
            Severity::Error,
        ));
    }
    for (name, cycle) in &palette.cycles {
        out.push(Check::new(
            format!("palette.cycle.{name}.period"),
            cycle.period > 0,
            cycle.period,
            "> 0",
            Severity::Error,
        ));
    }
    let mut duplicates = 0;
    for (i, c) in palette.colors.iter().take(256).enumerate() {
        duplicates += palette.colors[..i]
            .iter()
            .filter(|other| *other == c)
            .count();
    }
    out.push(Check::new(
        "palette.duplicates",
        config.max_duplicate_colors.is_none_or(|v| duplicates <= v),
        duplicates,
        format!(
            "<= {:?} identical RGBA pairs (None disables)",
            config.max_duplicate_colors
        ),
        style(config),
    ));
    out
}
