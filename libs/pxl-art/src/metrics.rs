//! Twelve integer measurements of how a scaled frame reads as pixel art.
//!
//! They are Info-level signals that guard against regressions; they do not
//! decide whether art is good. Everything is integer arithmetic, so a metric
//! table is identical on every platform and can be diffed between runs.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Oklab, Palette, Rgba};

use crate::analyze::{Axis, SpriteGraph};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metrics {
    /// M1: aligned 2x2 blocks that hold one colour, in basis points. `None`
    /// when a side is odd.
    pub copy2x_bps: Option<u32>,
    /// M2: pixels that differ from a plain nearest-neighbour copy of the
    /// native frame, as basis points of the opaque pixels.
    pub new_info_bps: u32,
    /// M3: staircase windows that are neither monotone nor within one pixel.
    pub jaggy_irregular: u32,
    /// M4: one-pixel elbows (exactly two same-colour neighbours at a right
    /// angle with no corner pixel).
    pub lshape_count: u32,
    /// M5: pixels with no same-colour 4-neighbour, outside protected features.
    pub orphan_count: u32,
    /// M6: protected small-feature pixels whose scaled block is still one
    /// solid square, in basis points (10000 when there are none).
    pub small_feature_kept_bps: u32,
    /// M7: OKLab lightness gap between the outline colour and the preview
    /// background, Q16 (0.15 is about 9830).
    pub outline_bg_dl: i32,
    /// M8: 8-connected silhouette-edge components over 8-connected body
    /// components, in basis points. 10000 means one unbroken edge per body.
    pub outline_continuity_bps: u32,
    /// M9: opaque pixels whose colour sits on a ramp of three or more steps,
    /// in basis points.
    pub ramp_coverage_bps: u32,
    /// M10: OKLab lightness P95 - P5 over opaque pixels, Q16.
    pub value_range: i32,
    /// M11: mirror mismatches for a symmetric sprite; `None` when the native
    /// sprite is not symmetric.
    pub symmetry_err: Option<u32>,
    /// M12: pairs of adjacent one-pixel lines of equal span and different
    /// colour (parallel banding).
    pub banding_count: u32,
}

impl Metrics {
    /// Stable `(id, value)` rows for printing and diffing. Missing optional
    /// measurements are skipped.
    pub fn entries(&self) -> Vec<(&'static str, i64)> {
        let mut rows = Vec::with_capacity(12);
        if let Some(v) = self.copy2x_bps {
            rows.push(("copy2x_bps", v as i64));
        }
        rows.push(("new_info_bps", self.new_info_bps as i64));
        rows.push(("jaggy_irregular", self.jaggy_irregular as i64));
        rows.push(("lshape_count", self.lshape_count as i64));
        rows.push(("orphan_count", self.orphan_count as i64));
        rows.push(("small_feature_kept_bps", self.small_feature_kept_bps as i64));
        rows.push(("outline_bg_dl", self.outline_bg_dl as i64));
        rows.push(("outline_continuity_bps", self.outline_continuity_bps as i64));
        rows.push(("ramp_coverage_bps", self.ramp_coverage_bps as i64));
        rows.push(("value_range", self.value_range as i64));
        if let Some(v) = self.symmetry_err {
            rows.push(("symmetry_err", v as i64));
        }
        rows.push(("banding_count", self.banding_count as i64));
        rows
    }
}

/// Measure `frame`, the result of scaling `native` by `k`. `graph` must have
/// been built from a frame with the same size as `native`.
pub fn measure(
    frame: &IndexFrame,
    native: &IndexFrame,
    k: u16,
    graph: &SpriteGraph,
    palette: &Palette,
    bg: Rgba,
) -> Metrics {
    Metrics {
        copy2x_bps: copy2x_bps(frame),
        new_info_bps: new_info_bps(frame, native, k),
        jaggy_irregular: jaggy_irregular(frame),
        lshape_count: lshape_count(frame),
        orphan_count: orphan_count(frame, k, graph),
        small_feature_kept_bps: small_feature_kept_bps(frame, native, k, graph),
        outline_bg_dl: outline_bg_dl(palette, bg),
        outline_continuity_bps: outline_continuity_bps(frame),
        ramp_coverage_bps: ramp_coverage_bps(frame, palette),
        value_range: value_range(frame, palette),
        symmetry_err: symmetry_err(frame, graph, palette),
        banding_count: banding_count(frame, palette.special.outline),
    }
}

fn bps(part: usize, whole: usize) -> u32 {
    if whole == 0 {
        0
    } else {
        (part * 10_000 / whole) as u32
    }
}

pub fn copy2x_bps(frame: &IndexFrame) -> Option<u32> {
    if frame.w % 2 != 0 || frame.h % 2 != 0 {
        return None;
    }
    let (mut same, mut total) = (0usize, 0usize);
    for by in 0..frame.h as i32 / 2 {
        for bx in 0..frame.w as i32 / 2 {
            let (x, y) = (bx * 2, by * 2);
            total += 1;
            let c = frame.get(x, y);
            if frame.get(x + 1, y) == c && frame.get(x, y + 1) == c && frame.get(x + 1, y + 1) == c
            {
                same += 1;
            }
        }
    }
    Some(bps(same, total))
}

pub fn new_info_bps(frame: &IndexFrame, native: &IndexFrame, k: u16) -> u32 {
    let Ok(baseline) = pxl_core::scale_copy(native, k) else {
        return 0;
    };
    if baseline.w != frame.w || baseline.h != frame.h {
        return 0;
    }
    bps(frame.diff_count(&baseline), frame.count_opaque().max(1))
}

pub(crate) const PROFILE_LEFT: usize = 0;
pub(crate) const PROFILE_RIGHT: usize = 1;
pub(crate) const PROFILE_TOP: usize = 2;
pub(crate) const PROFILE_BOTTOM: usize = 3;

pub(crate) fn silhouette_profiles(frame: &IndexFrame) -> [Vec<Option<i32>>; 4] {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut profiles = [
        vec![None; h as usize],
        vec![None; h as usize],
        vec![None; w as usize],
        vec![None; w as usize],
    ];
    for y in 0..h {
        for x in 0..w {
            if frame.get(x, y) == 0 {
                continue;
            }
            let (xi, yi) = (x as usize, y as usize);
            if profiles[PROFILE_LEFT][yi].is_none() {
                profiles[PROFILE_LEFT][yi] = Some(x);
            }
            profiles[PROFILE_RIGHT][yi] = Some(x);
            if profiles[PROFILE_TOP][xi].is_none() {
                profiles[PROFILE_TOP][xi] = Some(y);
            }
            profiles[PROFILE_BOTTOM][xi] = Some(y);
        }
    }
    profiles
}

pub fn jaggy_irregular(frame: &IndexFrame) -> u32 {
    silhouette_profiles(frame)
        .iter()
        .map(|profile| irregular(profile))
        .sum()
}

/// Run-length encode a silhouette edge profile and score each staircase.
pub(crate) fn irregular(profile: &[Option<i32>]) -> u32 {
    let mut total = 0;
    let mut runs: Vec<(i32, i32)> = Vec::new();
    for entry in profile {
        match entry {
            Some(v) => match runs.last_mut() {
                Some(last) if last.0 == *v => last.1 += 1,
                _ => runs.push((*v, 1)),
            },
            None => {
                total += irregular_runs(&runs);
                runs.clear();
            }
        }
    }
    total + irregular_runs(&runs)
}

/// A staircase is a stretch of runs whose position moves one pixel each time
/// in one direction. Its first and last runs are cut short by the sprite's own
/// extremes, so only the interior lengths are judged. Three lengths in a row
/// are fine when they are monotone (a curve's 1-1-2-3 rhythm) or within one
/// pixel of each other (a slope's 1-2-1-2); anything else, such as 2-1-3, is
/// irregular.
pub(crate) fn irregular_runs(runs: &[(i32, i32)]) -> u32 {
    let mut total = 0;
    let mut i = 0;
    while i + 1 < runs.len() {
        let dir = runs[i + 1].0 - runs[i].0;
        if dir.abs() != 1 {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j + 1 < runs.len() && runs[j + 1].0 - runs[j].0 == dir {
            j += 1;
        }
        let interior: Vec<i32> = runs[i + 1..j].iter().map(|r| r.1).collect();
        for w in interior.windows(3) {
            let (a, b, c) = (w[0], w[1], w[2]);
            let monotone = (a <= b && b <= c) || (a >= b && b >= c);
            let spread = a.max(b).max(c) - a.min(b).min(c);
            if !monotone && spread > 1 {
                total += 1;
            }
        }
        i = j;
    }
    total
}

pub fn lshape_count(frame: &IndexFrame) -> u32 {
    let mut count = 0;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            if c == 0 {
                continue;
            }
            let same = |dx: i32, dy: i32| frame.get(x + dx, y + dy) == c;
            let n = [same(-1, 0), same(1, 0), same(0, -1), same(0, 1)];
            if n.iter().filter(|s| **s).count() != 2 {
                continue;
            }
            let horizontal = if n[0] { -1 } else { 1 };
            let vertical = if n[2] { -1 } else { 1 };
            let bends = (n[0] || n[1]) && (n[2] || n[3]);
            if bends && !same(horizontal, vertical) {
                count += 1;
            }
        }
    }
    count
}

pub fn orphan_count(frame: &IndexFrame, k: u16, graph: &SpriteGraph) -> u32 {
    let k = k.max(1) as i32;
    let mut count = 0;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            if c == 0 || !graph.editable(x / k, y / k) {
                continue;
            }
            let alone = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .all(|(dx, dy)| frame.get(x + dx, y + dy) != c);
            if alone {
                count += 1;
            }
        }
    }
    count
}

pub fn small_feature_kept_bps(
    frame: &IndexFrame,
    native: &IndexFrame,
    k: u16,
    graph: &SpriteGraph,
) -> u32 {
    let kk = k.max(1) as i32;
    let (mut kept, mut total) = (0usize, 0usize);
    for y in 0..native.h as i32 {
        for x in 0..native.w as i32 {
            let i = (y as usize) * (native.w as usize) + (x as usize);
            let c = native.get(x, y);
            if c == 0 || !graph.small.get(i).copied().unwrap_or(false) {
                continue;
            }
            total += 1;
            let solid =
                (0..kk).all(|dy| (0..kk).all(|dx| frame.get(x * kk + dx, y * kk + dy) == c));
            if solid {
                kept += 1;
            }
        }
    }
    if total == 0 {
        10_000
    } else {
        bps(kept, total)
    }
}

pub fn outline_bg_dl(palette: &Palette, bg: Rgba) -> i32 {
    let outline = palette
        .colors
        .get(palette.special.outline as usize)
        .copied()
        .unwrap_or(Rgba::rgb(0, 0, 0));
    (Oklab::from_rgba(outline).l - Oklab::from_rgba(bg).l).abs()
}

pub fn outline_continuity_bps(frame: &IndexFrame) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut edge = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            if frame.get(x, y) == 0 {
                continue;
            }
            let open = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dy)| frame.get(x + dx, y + dy) == 0);
            edge[(y * w + x) as usize] = open;
        }
    }
    let body: Vec<bool> = frame.pixels.iter().map(|p| *p != 0).collect();
    let edges = components8(&edge, w, h);
    let bodies = components8(&body, w, h);
    bps(edges, bodies)
}

fn components8(mask: &[bool], w: i32, h: i32) -> usize {
    let mut seen = vec![false; mask.len()];
    let mut stack: Vec<(i32, i32)> = Vec::new();
    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            let start = (y * w + x) as usize;
            if !mask[start] || seen[start] {
                continue;
            }
            count += 1;
            seen[start] = true;
            stack.push((x, y));
            while let Some((cx, cy)) = stack.pop() {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (cx + dx, cy + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        let i = (ny * w + nx) as usize;
                        if mask[i] && !seen[i] {
                            seen[i] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
        }
    }
    count
}

pub fn ramp_coverage_bps(frame: &IndexFrame, palette: &Palette) -> u32 {
    let mut on_ramp = [false; 256];
    for ramp in palette.ramps.values() {
        if ramp.len() >= 3 {
            for &c in ramp {
                on_ramp[c as usize] = true;
            }
        }
    }
    let opaque = frame.count_opaque();
    let covered = frame
        .pixels
        .iter()
        .filter(|p| **p != 0 && on_ramp[**p as usize])
        .count();
    bps(covered, opaque)
}

pub fn value_range(frame: &IndexFrame, palette: &Palette) -> i32 {
    let mut light = [0i32; 256];
    for (i, c) in palette.colors.iter().enumerate().take(256) {
        light[i] = Oklab::from_rgba(*c).l;
    }
    let mut values: Vec<i32> = frame
        .pixels
        .iter()
        .filter(|p| **p != 0)
        .map(|p| light[*p as usize])
        .collect();
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let n = values.len();
    values[(n * 95 / 100).min(n - 1)] - values[n * 5 / 100]
}

pub fn symmetry_err(frame: &IndexFrame, graph: &SpriteGraph, palette: &Palette) -> Option<u32> {
    let axis = graph.symmetry?;
    let (w, h) = (frame.w as i32, frame.h as i32);
    let mut canonical = [0u16; 256];
    for (i, value) in canonical.iter_mut().enumerate() {
        *value = i as u16;
    }
    // A ramp is one logical material colour. Lighting within that ramp must
    // not turn an intentionally symmetric silhouette into an M11 error.
    for (ramp_id, ramp) in palette.ramps.values().enumerate() {
        for &color in ramp {
            canonical[color as usize] = 256 + ramp_id as u16;
        }
    }
    let mut wrong = 0;
    for y in 0..h {
        for x in 0..w {
            let mirror = match axis {
                Axis::Vertical => frame.get(w - 1 - x, y),
                Axis::Horizontal => frame.get(x, h - 1 - y),
            };
            let before = match axis {
                Axis::Vertical => x < w - 1 - x,
                Axis::Horizontal => y < h - 1 - y,
            };
            if before && canonical[frame.get(x, y) as usize] != canonical[mirror as usize] {
                wrong += 1;
            }
        }
    }
    Some(wrong)
}

pub fn banding_count(frame: &IndexFrame, outline: u8) -> u32 {
    let (w, h) = (frame.w as i32, frame.h as i32);
    bands(w, h, &|a, b| frame.get(a, b), outline) + bands(h, w, &|a, b| frame.get(b, a), outline)
}

/// `get(a, b)` reads along a line (`a`) and across lines (`b`). A line is a
/// run of four or more equal pixels with a different colour on both sides.
fn bands(along: i32, across: i32, get: &dyn Fn(i32, i32) -> u8, outline: u8) -> u32 {
    let mut count = 0;
    let mut prev: Vec<(i32, i32, u8)> = Vec::new();
    for b in 0..across {
        let mut cur: Vec<(i32, i32, u8)> = Vec::new();
        let mut a = 0;
        while a < along {
            let c = get(a, b);
            let mut end = a + 1;
            while end < along && get(end, b) == c {
                end += 1;
            }
            if c != 0 && c != outline && end - a >= 4 {
                let thin = (a..end).all(|x| get(x, b - 1) != c && get(x, b + 1) != c);
                if thin {
                    cur.push((a, end - a, c));
                }
            }
            a = end;
        }
        for (start, len, color) in &cur {
            if prev
                .iter()
                .any(|(s, l, c)| s == start && l == len && c != color)
            {
                count += 1;
            }
        }
        prev = cur;
    }
    count
}


#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
