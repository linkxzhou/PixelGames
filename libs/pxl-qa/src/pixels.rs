use crate::common::{delta, ratio, ratio_at_least, ratio_at_most, sample, style};
use crate::{Check, QaConfig, Severity};
use pxl_core::{IndexFrame, Palette};

pub(crate) fn inspect(
    frame: &IndexFrame,
    palette: &Palette,
    name: &str,
    empty_ok: bool,
    config: &QaConfig,
) -> Vec<Check> {
    let prefix = format!("frame.{name}");
    let mut out = Vec::new();
    let mut indices = Check::new(
        format!("{prefix}.palette"),
        true,
        0,
        "0 out-of-palette pixels",
        Severity::Error,
    );
    let mut bad = 0;
    let mut used = [false; 256];
    let mut border = 0;
    let mut specks = Check::new(
        format!("{prefix}.color_specks"),
        true,
        0,
        "same-color singleton pixels",
        style(config),
    );
    let mut speck_count = 0;
    let mut contrast = Check::new(
        format!("{prefix}.low_contrast"),
        true,
        0,
        "adjacent different colors meet configured channel contrast",
        style(config),
    );
    let mut low_contrast = 0;
    let mut checker = 0;
    let mut outline_gaps = 0;
    let mut outline = Check::new(
        format!("{prefix}.outline_gaps"),
        true,
        0,
        "0 exposed non-outline pixels when outline is used (heuristic)",
        style(config),
    );
    let uses_outline =
        palette.special.outline != 0 && frame.pixels.contains(&palette.special.outline);
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            used[c as usize] = true;
            if c as usize >= palette.len() {
                bad += 1;
                sample(&mut indices, x, y, config);
            }
            if c == 0 {
                continue;
            }
            if x == 0 || y == 0 || x == frame.w as i32 - 1 || y == frame.h as i32 - 1 {
                border += 1;
            }
            if ![
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
            ]
            .iter()
            .any(|(dx, dy)| frame.get(x + dx, y + dy) == c)
            {
                speck_count += 1;
                sample(&mut specks, x, y, config);
            }
            if uses_outline
                && c != palette.special.outline
                && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| frame.get(x + dx, y + dy) == 0)
            {
                outline_gaps += 1;
                sample(&mut outline, x, y, config);
            }
            for (dx, dy) in [(1, 0), (0, 1)] {
                let other = frame.get(x + dx, y + dy);
                if other == 0 || other == c {
                    continue;
                }
                if let (Some(a), Some(b), Some(min)) = (
                    palette.colors.get(c as usize),
                    palette.colors.get(other as usize),
                    config.min_neighbor_contrast,
                ) {
                    if delta(*a, *b) < min {
                        low_contrast += 1;
                        sample(&mut contrast, x, y, config);
                    }
                }
            }
            if x + 1 < frame.w as i32 && y + 1 < frame.h as i32 {
                let b = frame.get(x + 1, y);
                if b != 0 && b != c && frame.get(x, y + 1) == b && frame.get(x + 1, y + 1) == c {
                    checker += 1;
                }
            }
        }
    }
    indices.pass = bad == 0;
    indices.value = bad.to_string();
    out.push(indices);
    let opaque = frame.count_opaque();
    out.push(Check::new(
        format!("{prefix}.visible"),
        empty_ok || opaque > 0,
        opaque,
        "visible pixels or explicitly allowed empty state",
        Severity::Error,
    ));
    let coverage = ratio(opaque, frame.pixels.len());
    out.push(Check::new(
        format!("{prefix}.coverage"),
        config
            .min_coverage_bps
            .is_none_or(|v| ratio_at_least(opaque, frame.pixels.len(), v))
            && config
                .max_coverage_bps
                .is_none_or(|v| ratio_at_most(opaque, frame.pixels.len(), v)),
        coverage,
        format!(
            "coverage bps {:?}..{:?}",
            config.min_coverage_bps, config.max_coverage_bps
        ),
        style(config),
    ));
    out.push(Check::new(
        format!("{prefix}.bbox"),
        true,
        format!("{:?}", frame.bbox()),
        "measurement only; no subject-size assumption",
        Severity::Info,
    ));
    let colors = used
        .iter()
        .enumerate()
        .filter(|(i, v)| *i != 0 && **v)
        .count();
    limit(
        &mut out,
        &prefix,
        "colors",
        colors,
        config.max_colors,
        config,
    );
    limit(
        &mut out,
        &prefix,
        "border",
        border,
        config.max_border_pixels,
        config,
    );
    let (components, isolated, holes, largest, isolated_points, hole_points) =
        topology(frame, config.max_samples);
    limit(
        &mut out,
        &prefix,
        "components",
        components,
        config.max_components,
        config,
    );
    limit(
        &mut out,
        &prefix,
        "isolated",
        isolated,
        config.max_isolated_pixels,
        config,
    );
    if let Some(c) = out.last_mut() {
        c.samples = isolated_points;
    }
    limit(&mut out, &prefix, "holes", holes, config.max_holes, config);
    if let Some(c) = out.last_mut() {
        c.samples = hole_points;
    }
    out.push(Check::new(
        format!("{prefix}.largest_component_bps"),
        true,
        ratio(largest, opaque),
        "measurement only",
        Severity::Info,
    ));
    specks.value = speck_count.to_string();
    specks.pass = config.max_color_specks.is_none_or(|v| speck_count <= v);
    specks.limit = format!(
        "<= {:?} same-color singletons (None disables)",
        config.max_color_specks
    );
    out.push(specks);
    let checker_total = frame.w.saturating_sub(1) as usize * frame.h.saturating_sub(1) as usize;
    let checker_bps = ratio(checker, checker_total);
    out.push(Check::new(
        format!("{prefix}.checkerboard_bps"),
        config
            .max_checkerboard_bps
            .is_none_or(|v| ratio_at_most(checker, checker_total, v)),
        checker_bps,
        format!("<= {:?} bps (None disables)", config.max_checkerboard_bps),
        if config.max_checkerboard_bps.is_some() {
            style(config)
        } else {
            Severity::Info
        },
    ));
    contrast.pass = low_contrast == 0;
    contrast.value = low_contrast.to_string();
    out.push(contrast);
    outline.pass = config.max_outline_gaps.is_none_or(|v| outline_gaps <= v);
    outline.limit = format!(
        "<= {:?} exposed non-outline pixels (None disables)",
        config.max_outline_gaps
    );
    outline.value = outline_gaps.to_string();
    if uses_outline {
        out.push(outline);
    }
    out
}

fn limit(
    out: &mut Vec<Check>,
    prefix: &str,
    name: &str,
    n: usize,
    max: Option<usize>,
    config: &QaConfig,
) {
    out.push(Check::new(
        format!("{prefix}.{name}"),
        max.is_none_or(|v| n <= v),
        n,
        format!("<= {max:?} (None disables)"),
        if max.is_some() {
            style(config)
        } else {
            Severity::Info
        },
    ));
}

type Topology = (
    usize,
    usize,
    usize,
    usize,
    Vec<crate::PixelLocation>,
    Vec<crate::PixelLocation>,
);

/// Four-connected opaque components and enclosed transparent components.
fn topology(frame: &IndexFrame, max_samples: usize) -> Topology {
    let mut seen = vec![false; frame.pixels.len()];
    let mut stack = Vec::new();
    let (mut components, mut isolated, mut holes, mut largest) = (0, 0, 0, 0);
    let (mut isolated_points, mut hole_points) = (Vec::new(), Vec::new());
    let w = frame.w as usize;
    for start in 0..seen.len() {
        if seen[start] {
            continue;
        }
        let opaque = frame.pixels[start] != 0;
        seen[start] = true;
        stack.push(start);
        let (mut size, mut edge) = (0, false);
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            edge |= x == 0 || y == 0 || x == frame.w as i32 - 1 || y == frame.h as i32 - 1;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(j) = frame.idx(x + dx, y + dy) {
                    if !seen[j] && (frame.pixels[j] != 0) == opaque {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let point = crate::PixelLocation {
            x: (start % w) as u16,
            y: (start / w) as u16,
        };
        if opaque {
            components += 1;
            largest = largest.max(size);
            if size == 1 {
                isolated += 1;
                if isolated_points.len() < max_samples {
                    isolated_points.push(point);
                }
            }
        } else if !edge {
            holes += 1;
            if hole_points.len() < max_samples {
                hole_points.push(point);
            }
        }
    }
    (
        components,
        isolated,
        holes,
        largest,
        isolated_points,
        hole_points,
    )
}
