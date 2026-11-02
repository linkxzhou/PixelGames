//! Integer vector contour reconstruction. Pixel-region boundaries are traced
//! into closed loops, simplified with RDP, optionally fitted with quadratic
//! curves, and rasterised at the requested integer scale.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, PxlResult};

use crate::analyze::{Material, SpriteGraph};
use crate::upscale::UNIT;

const RDP_EPSILON: i32 = UNIT / 2;
const EAST: u8 = 0;
const SOUTH: u8 = 1;
const WEST: u8 = 2;
const NORTH: u8 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) / 2, (self.y + other.y) / 2)
    }
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    from: Point,
    to: Point,
    direction: u8,
}

#[derive(Clone, Debug)]
struct Region {
    color: u8,
    material: Material,
    cells: Vec<usize>,
}

/// Rebuild every colour/material region as vector geometry and composite the
/// regions in scan order, with outline-material regions last.
pub fn reconstruct(frame: &IndexFrame, k: u16, graph: &SpriteGraph) -> PxlResult<IndexFrame> {
    if k <= 1 {
        return Ok(frame.clone());
    }
    let mut out = IndexFrame::canvas(frame.w * k, frame.h * k)?;
    let (regions, labels) = regions(frame, graph);
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by_key(|i| regions[*i].material == Material::Outline);

    for id in order {
        let region = &regions[id];
        let loops = trace_loops(frame, &labels, id as u32)
            .into_iter()
            .map(|points| simplify_closed(&points, region.material))
            .filter(|points| points.len() >= 3)
            .collect::<Vec<_>>();
        if loops.is_empty() {
            paint_native_region(&mut out, frame, region, k);
            continue;
        }
        paint_region(&mut out, &loops, region.color, k);
    }

    let baseline = pxl_core::scale_copy(frame, k)?;
    fill_new_holes(&mut out, &baseline);
    let kk = k as i32;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            if graph.editable(x, y) {
                continue;
            }
            for dy in 0..kk {
                for dx in 0..kk {
                    let (px, py) = (x * kk + dx, y * kk + dy);
                    out.set(px, py, baseline.get(px, py));
                }
            }
        }
    }
    Ok(out)
}

fn regions(frame: &IndexFrame, graph: &SpriteGraph) -> (Vec<Region>, Vec<u32>) {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let n = frame.pixels.len();
    let mut labels = vec![u32::MAX; n];
    let mut regions = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        let color = frame.pixels[start];
        if color == 0 || labels[start] != u32::MAX {
            continue;
        }
        let material = graph.materials[start];
        let id = regions.len() as u32;
        let mut cells = Vec::new();
        labels[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            cells.push(i);
            let (x, y) = (i as i32 % w, i as i32 / w);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = (ny * w + nx) as usize;
                if labels[j] == u32::MAX
                    && frame.pixels[j] == color
                    && graph.materials[j] == material
                {
                    labels[j] = id;
                    stack.push(j);
                }
            }
        }
        cells.sort_unstable();
        regions.push(Region {
            color,
            material,
            cells,
        });
    }
    (regions, labels)
}

/// Trace directed cell edges. Interior is kept on the right; at a diagonal
/// saddle the right-most unused turn is selected, so loops cannot merge.
fn trace_loops(frame: &IndexFrame, labels: &[u32], id: u32) -> Vec<Vec<Point>> {
    let (w, h) = (frame.w as i32, frame.h as i32);
    let member =
        |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && labels[(y * w + x) as usize] == id;
    let mut edges = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !member(x, y) {
                continue;
            }
            let (x0, y0) = (x * UNIT, y * UNIT);
            let (x1, y1) = (x0 + UNIT, y0 + UNIT);
            if !member(x, y - 1) {
                edges.push(Edge {
                    from: Point::new(x0, y0),
                    to: Point::new(x1, y0),
                    direction: EAST,
                });
            }
            if !member(x + 1, y) {
                edges.push(Edge {
                    from: Point::new(x1, y0),
                    to: Point::new(x1, y1),
                    direction: SOUTH,
                });
            }
            if !member(x, y + 1) {
                edges.push(Edge {
                    from: Point::new(x1, y1),
                    to: Point::new(x0, y1),
                    direction: WEST,
                });
            }
            if !member(x - 1, y) {
                edges.push(Edge {
                    from: Point::new(x0, y1),
                    to: Point::new(x0, y0),
                    direction: NORTH,
                });
            }
        }
    }

    let mut outgoing = BTreeMap::<Point, Vec<usize>>::new();
    for (i, edge) in edges.iter().enumerate() {
        outgoing.entry(edge.from).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let start = edges[first].from;
        let mut points = vec![start];
        let mut current = first;
        let mut closed = false;
        for _ in 0..=edges.len() {
            if used[current] {
                break;
            }
            used[current] = true;
            let edge = edges[current];
            points.push(edge.to);
            if edge.to == start {
                closed = true;
                break;
            }
            let Some(candidates) = outgoing.get(&edge.to) else {
                break;
            };
            let mut next = None;
            for &candidate in candidates {
                if used[candidate] {
                    continue;
                }
                let turn = (edges[candidate].direction + 4 - edge.direction) % 4;
                let rank = match turn {
                    1 => 0, // right
                    0 => 1, // straight
                    3 => 2, // left
                    _ => 3, // reverse
                };
                if next.map_or(true, |(best_rank, _)| rank < best_rank) {
                    next = Some((rank, candidate));
                }
            }
            let Some((_, candidate)) = next else {
                break;
            };
            current = candidate;
        }
        if closed {
            points.pop();
            dedup_points(&mut points);
            if points.len() >= 3 {
                loops.push(points);
            }
        }
    }
    loops
}

fn simplify_closed(points: &[Point], material: Material) -> Vec<Point> {
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

fn anchor_pairs(anchors: &[usize], n: usize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..anchors.len() {
        pairs.push((anchors[i], anchors[(i + 1) % anchors.len()]));
    }
    if pairs.len() == 1 && pairs[0].0 == pairs[0].1 {
        pairs[0].1 = (pairs[0].1 + 1) % n;
    }
    pairs
}

fn chain_between(points: &[Point], a: usize, b: usize) -> Vec<Point> {
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
fn idealize_staircases(points: &[Point]) -> Vec<Point> {
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

fn fits_line(points: &[Point], tolerance: i32) -> bool {
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

fn farthest(points: &[Point], from: usize) -> usize {
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

fn rdp_open(points: &[Point]) -> Vec<Point> {
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

fn sharp_turn(points: &[Point], i: usize) -> bool {
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
fn smooth_curves(points: &[Point]) -> Vec<Point> {
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

fn quadratic(a: Point, control: Point, b: Point, step: i32, steps: i32) -> Point {
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

fn round_div(value: i64, denominator: i64) -> i32 {
    if value >= 0 {
        ((value + denominator / 2) / denominator) as i32
    } else {
        -((-value + denominator / 2) / denominator) as i32
    }
}

fn push_unique(points: &mut Vec<Point>, point: Point) {
    if points.last().copied() != Some(point) {
        points.push(point);
    }
}

fn dedup_points(points: &mut Vec<Point>) {
    points.dedup();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
}

/// Preserve the source topology: an enclosed clear component that does not
/// overlap any source-clear pixel was introduced by vectorisation, so restore
/// those pixels from the nearest-neighbour baseline.
fn fill_new_holes(out: &mut IndexFrame, baseline: &IndexFrame) {
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

fn paint_native_region(out: &mut IndexFrame, frame: &IndexFrame, region: &Region, k: u16) {
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

fn paint_region(out: &mut IndexFrame, loops: &[Vec<Point>], color: u8, k: u16) {
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

fn inside(loops: &[Vec<Point>], x: i32, y: i32) -> bool {
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

fn on_segment(a: Point, b: Point, x: i32, y: i32) -> bool {
    let cross = (x - a.x) as i64 * (b.y - a.y) as i64 - (y - a.y) as i64 * (b.x - a.x) as i64;
    if cross != 0 {
        return false;
    }
    x >= a.x.min(b.x) && x <= a.x.max(b.x) && y >= a.y.min(b.y) && y <= a.y.max(b.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use pxl_core::{Palette, Rgba};

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn graph(frame: &IndexFrame) -> SpriteGraph {
        let mut palette = Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(20, 20, 24),
                Rgba::rgb(240, 240, 232)
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: Default::default(),
        };
        palette.special.outline = 1;
        SpriteGraph::build(frame, &palette, None)
    }

    #[test]
    fn rectangles_keep_their_exact_geometry() {
        let f = frame(&["000000", "022220", "022220", "022220", "000000"]);
        let mut g = graph(&f);
        g.protect.fill(false);
        for k in [2, 3, 4] {
            let out = reconstruct(&f, k, &g).unwrap();
            assert_eq!(out, pxl_core::scale_copy(&f, k).unwrap(), "k={k}");
        }
    }

    #[test]
    fn holes_are_independent_loops() {
        let f = frame(&["022220", "020020", "020020", "022220", "000000"]);
        let mut g = graph(&f);
        g.protect.fill(false);
        let out = reconstruct(&f, 3, &g).unwrap();
        assert_eq!(out.get(4, 7), 2, "left ring");
        assert_eq!(out.get(7, 7), 0, "hole centre");
        assert_eq!(out.get(7, 10), 2, "bottom ring");
    }

    #[test]
    fn only_new_enclosed_holes_are_restored() {
        let solid = frame(&["222", "222", "222"]);
        let mut new_hole = solid.clone();
        new_hole.set(1, 1, 0);
        fill_new_holes(&mut new_hole, &solid);
        assert_eq!(new_hole, solid);

        let existing = frame(&["222", "202", "222"]);
        let mut preserved = existing.clone();
        fill_new_holes(&mut preserved, &existing);
        assert_eq!(preserved.get(1, 1), 0);
    }

    #[test]
    fn rdp_simplifies_a_pixel_staircase() {
        let points = [(0, 0), (1, 0), (1, 1), (2, 1), (2, 2), (3, 2)]
            .map(|(x, y)| Point::new(x * UNIT, y * UNIT));
        let simplified = rdp_open(&points);
        assert!(simplified.len() < points.len(), "{simplified:?}");
        assert_eq!(simplified.first(), points.first());
        assert_eq!(simplified.last(), points.last());
    }

    #[test]
    fn gentle_curve_runs_are_fitted_with_quadratics() {
        let points = [
            (0, 2),
            (2, 0),
            (4, 0),
            (6, 2),
            (6, 4),
            (4, 6),
            (2, 6),
            (0, 4),
        ]
        .map(|(x, y)| Point::new(x * UNIT, y * UNIT));
        let curved = smooth_curves(&points);
        assert!(curved.len() > points.len());
        assert!(curved.windows(2).all(|edge| edge[0] != edge[1]));
    }

    #[test]
    fn reconstruction_is_deterministic_and_uses_source_colours() {
        let f = frame(&["0011100", "0122210", "1222221", "0122210", "0011100"]);
        let g = graph(&f);
        let a = reconstruct(&f, 3, &g).unwrap();
        let b = reconstruct(&f, 3, &g).unwrap();
        assert_eq!(a, b);
        assert!(a.pixels.iter().all(|p| *p <= 2));
    }

    fn sloped_strip(tread: usize) -> IndexFrame {
        let mut rows = Vec::new();
        for step in 0..4usize {
            for _ in 0..tread {
                rows.push(
                    (0..8)
                        .map(|x| if x < 3 - step { '1' } else { '2' })
                        .collect::<String>(),
                );
            }
        }
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn colour_profile(frame: &IndexFrame, color: u8) -> Vec<(i32, i32)> {
        let mut runs: Vec<(i32, i32)> = Vec::new();
        for y in 0..frame.h as i32 {
            let count = (0..frame.w as i32)
                .filter(|&x| frame.get(x, y) == color)
                .count() as i32;
            match runs.last_mut() {
                Some((position, length)) if *position == count => *length += 1,
                _ => runs.push((count, 1)),
            }
        }
        runs
    }

    #[test]
    fn straight_slopes_have_uniform_output_steps() {
        for tread in [2usize, 3] {
            let f = sloped_strip(tread);
            let mut g = graph(&f);
            g.protect.fill(false);
            for k in [2, 3] {
                let out = reconstruct(&f, k, &g).unwrap();
                let runs = colour_profile(&out, 1);
                assert!(runs.len() >= 4, "tread={tread} k={k}: {runs:?}");
                let deltas: Vec<i32> = runs.windows(2).map(|pair| pair[1].0 - pair[0].0).collect();
                assert!(deltas.iter().all(|delta| *delta != 0));
                let delta_spread = deltas.iter().max().unwrap() - deltas.iter().min().unwrap();
                assert!(delta_spread <= 1, "tread={tread} k={k}: {runs:?}");
                let interior = &runs[1..runs.len() - 1];
                let min = interior.iter().map(|run| run.1).min().unwrap();
                let max = interior.iter().map(|run| run.1).max().unwrap();
                assert!(max - min <= 1, "tread={tread} k={k}: {runs:?}");
            }
        }
    }

    fn midpoint_disc(radius: i32) -> IndexFrame {
        let size = (2 * radius + 1) as usize;
        let mut mask = vec![false; size * size];
        let (mut x, mut y) = (radius, 0);
        let mut decision = 1 - radius;
        while y <= x {
            for (px, py) in [
                (x, y),
                (y, x),
                (y, -x),
                (x, -y),
                (-x, -y),
                (-y, -x),
                (-y, x),
                (-x, y),
            ] {
                let gx = radius + px;
                let gy = radius + py;
                if gx >= 0 && gy >= 0 && gx < size as i32 && gy < size as i32 {
                    mask[gy as usize * size + gx as usize] = true;
                }
            }
            y += 1;
            if decision <= 0 {
                decision += 2 * y + 1;
            } else {
                x -= 1;
                decision += 2 * (y - x) + 1;
            }
        }
        let rows = (0..size)
            .map(|y| {
                let first = (0..size).find(|&x| mask[y * size + x]);
                first.map_or_else(
                    || "0".repeat(size),
                    |first| {
                        let last = (first..size).rev().find(|&x| mask[y * size + x]).unwrap();
                        (0..size)
                            .map(|x| if x >= first && x <= last { '2' } else { '0' })
                            .collect::<String>()
                    },
                )
            })
            .collect::<Vec<_>>();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    #[test]
    fn circle_boundaries_stay_within_one_native_pixel() {
        let radius = 6i32;
        let k = 3i32;
        let f = midpoint_disc(radius);
        let mut g = graph(&f);
        g.protect.fill(false);
        let out = reconstruct(&f, k as u16, &g).unwrap();
        let centre = (2 * radius + 1) * k;
        let radius_q = centre;
        let mut largest_error = 0i32;
        for y in 0..out.h as i32 {
            let dy = 2 * y + 1 - centre;
            if dy.abs() + 2 * k >= radius_q {
                continue;
            }
            let expected = |x: i32| {
                let dx = 2 * x + 1 - centre;
                dx * dx + dy * dy <= radius_q * radius_q
            };
            let expected_left = (0..out.w as i32).find(|&x| expected(x));
            let actual_left = (0..out.w as i32).find(|&x| out.get(x, y) != 0);
            let (Some(expected_left), Some(actual_left)) = (expected_left, actual_left) else {
                panic!("missing row coverage at y={y}");
            };
            let expected_right = (expected_left..out.w as i32)
                .rev()
                .find(|&x| expected(x))
                .unwrap();
            let actual_right = (actual_left..out.w as i32)
                .rev()
                .find(|&x| out.get(x, y) != 0)
                .unwrap();
            largest_error = largest_error
                .max((actual_left - expected_left).abs())
                .max((actual_right - expected_right).abs());
        }
        assert!(
            largest_error <= k,
            "boundary error {largest_error} output pixels"
        );
    }

    #[test]
    fn random_frames_are_deterministic_and_never_invent_colours() {
        let mut seed = 0x6d2b79f5u32;
        for case in 0..500u32 {
            let rows = (0..16)
                .map(|_| {
                    (0..16)
                        .map(|_| {
                            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                            match seed >> 29 {
                                0 | 1 => '0',
                                2 => '1',
                                3 => '2',
                                _ => '3',
                            }
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            let f = pxl_core::decode_matrix(&rows).unwrap();
            let mut g = graph(&f);
            if case % 3 == 0 {
                g.protect.fill(false);
            }
            let k = 2 + (case % 3) as u16;
            let a = reconstruct(&f, k, &g).unwrap();
            let b = reconstruct(&f, k, &g).unwrap();
            assert_eq!(a, b, "case {case}");
            assert_eq!((a.w, a.h), (f.w * k, f.h * k));
            assert!(a.pixels.iter().all(|p| *p <= 3), "case {case}");
        }
    }
}
