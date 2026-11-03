//! Integer vector contour reconstruction. Pixel-region boundaries are traced
//! into closed loops, simplified with RDP, optionally fitted with quadratic
//! curves, and rasterised at the requested integer scale.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, PxlResult};

use crate::analyze::{Material, SpriteGraph};
use crate::upscale::UNIT;

use super::trace::{Point, Region, RDP_EPSILON};

pub(crate) fn simplify_closed(points: &[Point], material: Material) -> Vec<Point> {
    let mut points = points.to_vec();
    dedup_points(&mut points);
    if points.len() <= 3 {
        return points;
    }

    let mut anchors = if material.keeps_corners() {
        (0..points.len())
            .filter(|&i| sharp_turn(&points, i))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if anchors.len() < 2 {
        let b = farthest(&points, 0);
        let a = farthest(&points, b);
        anchors = if a == b {
            alloc::vec![0, b]
        } else {
            alloc::vec![a, b]
        };
    }
    anchors.sort_unstable();
    anchors.dedup();
    if anchors.len() < 2 || anchors[0] == anchors[1] {
        return points;
    }

    let mut simplified = Vec::new();
    for (a, b) in anchor_pairs(&anchors, points.len()) {
        let chain = chain_between(&points, a, b);
        let chain = if material.keeps_corners() {
            chain
        } else {
            idealize_staircases(&chain)
        };
        let part = rdp_open(&chain);
        let take = part.len().saturating_sub(1);
        simplified.extend(part.into_iter().take(take));
    }
    dedup_points(&mut simplified);
    if simplified.len() < 3 {
        return points;
    }
    if material.keeps_corners() {
        simplified
    } else {
        smooth_curves(&simplified)
    }
}

pub(crate) fn anchor_pairs(anchors: &[usize], n: usize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..anchors.len() {
        pairs.push((anchors[i], anchors[(i + 1) % anchors.len()]));
    }
    if pairs.len() == 1 && pairs[0].0 == pairs[0].1 {
        pairs[0].1 = (pairs[0].1 + 1) % n;
    }
    pairs
}

pub(crate) fn chain_between(points: &[Point], a: usize, b: usize) -> Vec<Point> {
    let mut chain = vec![points[a]];
    let mut i = a;
    while i != b {
        i = (i + 1) % points.len();
        chain.push(points[i]);
    }
    chain
}

/// Collapse uniform axis-aligned staircases into their ideal straight slope.
/// Runs with lengths n/n+1 are retained as Bresenham-style sampling error;
/// longer monotone runs are left for the curve fitter.
pub(crate) fn idealize_staircases(points: &[Point]) -> Vec<Point> {
    if points.len() < 5 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    let mut i = 0;
    while i + 1 < points.len() {
        let mut horizontal = Vec::new();
        let mut vertical = Vec::new();
        let (mut x_sign, mut y_sign) = (0i32, 0i32);
        let mut previous_axis = u8::MAX;
        let mut j = i;
        while j + 1 < points.len() {
            let dx = points[j + 1].x - points[j].x;
            let dy = points[j + 1].y - points[j].y;
            let axis = if dy == 0 && dx != 0 {
                0
            } else if dx == 0 && dy != 0 {
                1
            } else {
                break;
            };
            if axis == previous_axis {
                break;
            }
            if axis == 0 {
                let sign = dx.signum();
                if x_sign != 0 && sign != x_sign {
                    break;
                }
                x_sign = sign;
                horizontal.push(dx.abs());
            } else {
                let sign = dy.signum();
                if y_sign != 0 && sign != y_sign {
                    break;
                }
                y_sign = sign;
                vertical.push(dy.abs());
            }
            previous_axis = axis;
            j += 1;
        }
        let developing_curve = |lengths: &[i32]| {
            if lengths.len() < 3 {
                return false;
            }
            let spread = lengths.iter().max().unwrap() - lengths.iter().min().unwrap();
            spread > 1
                && (lengths.windows(2).all(|pair| pair[0] <= pair[1])
                    || lengths.windows(2).all(|pair| pair[0] >= pair[1]))
        };
        if j - i >= 4
            && x_sign != 0
            && y_sign != 0
            && !developing_curve(&horizontal)
            && !developing_curve(&vertical)
            && fits_line(&points[i..=j], UNIT)
        {
            push_unique(&mut out, points[j]);
            i = j;
        } else {
            push_unique(&mut out, points[i + 1]);
            i += 1;
        }
    }
    out
}

pub(crate) fn fits_line(points: &[Point], tolerance: i32) -> bool {
    if points.len() < 3 {
        return true;
    }
    let a = points[0];
    let b = points[points.len() - 1];
    let dx = (b.x - a.x) as i128;
    let dy = (b.y - a.y) as i128;
    let len2 = dx * dx + dy * dy;
    if len2 == 0 {
        return false;
    }
    points[1..points.len() - 1].iter().all(|point| {
        let px = (point.x - a.x) as i128;
        let py = (point.y - a.y) as i128;
        let cross = dx * py - dy * px;
        cross * cross <= (tolerance * tolerance) as i128 * len2
    })
}

pub(crate) fn farthest(points: &[Point], from: usize) -> usize {
    let mut best = from;
    let mut distance = -1i64;
    for (i, point) in points.iter().enumerate() {
        let dx = (point.x - points[from].x) as i64;
        let dy = (point.y - points[from].y) as i64;
        let d = dx * dx + dy * dy;
        if d > distance {
            distance = d;
            best = i;
        }
    }
    best
}

pub(crate) fn rdp_open(points: &[Point]) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = alloc::vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let start = points[a];
        let end = points[b];
        let dx = (end.x - start.x) as i128;
        let dy = (end.y - start.y) as i128;
        let len2 = dx * dx + dy * dy;
        let limit = if len2 == 0 {
            (RDP_EPSILON * RDP_EPSILON) as i128
        } else {
            (RDP_EPSILON * RDP_EPSILON) as i128 * len2
        };
        let mut best = None;
        for (i, point) in points.iter().enumerate().take(b).skip(a + 1) {
            let px = (point.x - start.x) as i128;
            let py = (point.y - start.y) as i128;
            let score = if len2 == 0 {
                px * px + py * py
            } else {
                let cross = dx * py - dy * px;
                cross * cross
            };
            if score > limit && best.map_or(true, |(best_score, _)| score > best_score) {
                best = Some((score, i));
            }
        }
        if let Some((_, i)) = best {
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    points
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, p)| *p)
        .collect()
}

pub(crate) fn sharp_turn(points: &[Point], i: usize) -> bool {
    let n = points.len();
    let a = points[(i + n - 1) % n];
    let b = points[i];
    let c = points[(i + 1) % n];
    let (ux, uy) = ((b.x - a.x) as i128, (b.y - a.y) as i128);
    let (vx, vy) = ((c.x - b.x) as i128, (c.y - b.y) as i128);
    let lu = ux * ux + uy * uy;
    let lv = vx * vx + vy * vy;
    if lu == 0 || lv == 0 {
        return false;
    }
    let dot = ux * vx + uy * vy;
    dot <= 0 || 4 * dot * dot <= lu * lv
}

/// Replace runs of at least three gentle same-direction turns with quadratic
/// segments. Sharp corners remain polygon vertices.
pub(crate) fn smooth_curves(points: &[Point]) -> Vec<Point> {
    let n = points.len();
    if n < 4 {
        return points.to_vec();
    }
    let mut signs = vec![0i8; n];
    for i in 0..n {
        if sharp_turn(points, i) {
            continue;
        }
        let a = points[(i + n - 1) % n];
        let b = points[i];
        let c = points[(i + 1) % n];
        let cross =
            (b.x - a.x) as i64 * (c.y - b.y) as i64 - (b.y - a.y) as i64 * (c.x - b.x) as i64;
        signs[i] = cross.signum() as i8;
    }

    let mut smooth = vec![false; n];
    let boundary = (0..n).find(|&i| signs[i] == 0 || signs[(i + n - 1) % n] != signs[i]);
    if let Some(start) = boundary {
        let mut pos = 0;
        while pos < n {
            let i = (start + pos) % n;
            let sign = signs[i];
            let mut len = 1;
            while pos + len < n && signs[(start + pos + len) % n] == sign {
                len += 1;
            }
            if sign != 0 && len >= 3 {
                for j in 0..len {
                    smooth[(start + pos + j) % n] = true;
                }
            }
            pos += len;
        }
    } else if signs[0] != 0 {
        smooth.fill(true);
    }
    if !smooth.iter().any(|v| *v) {
        return points.to_vec();
    }

    let mut out = Vec::new();
    for i in 0..n {
        if !smooth[i] {
            push_unique(&mut out, points[i]);
            continue;
        }
        let previous = points[(i + n - 1) % n];
        let next = points[(i + 1) % n];
        let start = previous.midpoint(points[i]);
        let end = points[i].midpoint(next);
        push_unique(&mut out, start);
        let span = (points[i].x - start.x)
            .abs()
            .max((points[i].y - start.y).abs())
            .max((end.x - points[i].x).abs())
            .max((end.y - points[i].y).abs());
        let steps = (4 * (span + UNIT - 1) / UNIT).clamp(4, 32);
        for step in 1..=steps {
            push_unique(&mut out, quadratic(start, points[i], end, step, steps));
        }
    }
    dedup_points(&mut out);
    if out.len() >= 3 {
        out
    } else {
        points.to_vec()
    }
}

pub(crate) fn quadratic(a: Point, control: Point, b: Point, step: i32, steps: i32) -> Point {
    if step == steps {
        return b;
    }
    let unit = UNIT as i64;
    let t = step as i64 * unit / steps as i64;
    let u = unit - t;
    let denominator = unit * unit;
    let weights = [u * u, 2 * u * t, t * t];
    let x = weights[0] * a.x as i64 + weights[1] * control.x as i64 + weights[2] * b.x as i64;
    let y = weights[0] * a.y as i64 + weights[1] * control.y as i64 + weights[2] * b.y as i64;
    Point::new(round_div(x, denominator), round_div(y, denominator))
}

pub(crate) fn round_div(value: i64, denominator: i64) -> i32 {
    if value >= 0 {
        ((value + denominator / 2) / denominator) as i32
    } else {
        -((-value + denominator / 2) / denominator) as i32
    }
}

pub(crate) fn push_unique(points: &mut Vec<Point>, point: Point) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

pub(crate) fn dedup_points(points: &mut Vec<Point>) {
    points.dedup();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
}

/// Preserve the source topology: an enclosed clear component that does not
/// overlap any source-clear pixel was introduced by vectorisation, so restore
/// those pixels from the nearest-neighbour baseline.
pub(crate) fn fill_new_holes(out: &mut IndexFrame, baseline: &IndexFrame) {
    let (w, h) = (out.w as i32, out.h as i32);
    let mut seen = vec![false; out.pixels.len()];
    let mut stack = Vec::new();
    for start in 0..out.pixels.len() {
        if out.pixels[start] != 0 || seen[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut touches_edge = false;
        let mut existed = false;
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            component.push(i);
            let (x, y) = (i as i32 % w, i as i32 / w);
            touches_edge |= x == 0 || y == 0 || x == w - 1 || y == h - 1;
            existed |= baseline.pixels[i] == 0;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if out.pixels[j] == 0 && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if !touches_edge && !existed {
            for i in component {
                out.pixels[i] = baseline.pixels[i];
            }
        }
    }
}

pub(crate) fn paint_native_region(out: &mut IndexFrame, frame: &IndexFrame, region: &Region, k: u16) {
    let kk = k as i32;
    for &i in &region.cells {
        let x = i as i32 % frame.w as i32;
        let y = i as i32 / frame.w as i32;
        for dy in 0..kk {
            for dx in 0..kk {
                out.set(x * kk + dx, y * kk + dy, region.color);
            }
        }
    }
}

pub(crate) fn paint_region(out: &mut IndexFrame, loops: &[Vec<Point>], color: u8, k: u16) {
    let mut min = Point::new(i32::MAX, i32::MAX);
    let mut max = Point::new(i32::MIN, i32::MIN);
    for point in loops.iter().flatten() {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let kk = k as i32;
    let x0 = (min.x * kk / UNIT - 1).max(0);
    let y0 = (min.y * kk / UNIT - 1).max(0);
    let x1 = ((max.x * kk + UNIT - 1) / UNIT + 1).min(out.w as i32 - 1);
    let y1 = ((max.y * kk + UNIT - 1) / UNIT + 1).min(out.h as i32 - 1);
    for y in y0..=y1 {
        let sy = (2 * y + 1) * UNIT / (2 * kk);
        for x in x0..=x1 {
            let sx = (2 * x + 1) * UNIT / (2 * kk);
            if inside(loops, sx, sy) {
                out.set(x, y, color);
            }
        }
    }
}

pub(crate) fn inside(loops: &[Vec<Point>], x: i32, y: i32) -> bool {
    let mut inside = false;
    for points in loops {
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            if on_segment(a, b, x, y) {
                return true;
            }
            if (a.y > y) == (b.y > y) {
                continue;
            }
            let dy = (b.y - a.y) as i64;
            let lhs = (x - a.x) as i64 * dy;
            let rhs = (y - a.y) as i64 * (b.x - a.x) as i64;
            let crossing = if dy > 0 { lhs < rhs } else { lhs > rhs };
            if crossing {
                inside = !inside;
            }
        }
    }
    inside
}

pub(crate) fn on_segment(a: Point, b: Point, x: i32, y: i32) -> bool {
    let cross = (x - a.x) as i64 * (b.y - a.y) as i64 - (y - a.y) as i64 * (b.x - a.x) as i64;
    if cross != 0 {
        return false;
    }
    x >= a.x.min(b.x) && x <= a.x.max(b.x) && y >= a.y.min(b.y) && y <= a.y.max(b.y)
}

