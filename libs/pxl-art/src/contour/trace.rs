//! Integer vector contour reconstruction. Pixel-region boundaries are traced
//! into closed loops, simplified with RDP, optionally fitted with quadratic
//! curves, and rasterised at the requested integer scale.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, PxlResult};

use crate::analyze::{Material, SpriteGraph};
use crate::upscale::UNIT;

use super::geom::{dedup_points, fill_new_holes, paint_native_region, paint_region, simplify_closed};


pub(crate) const RDP_EPSILON: i32 = UNIT / 2;
const EAST: u8 = 0;
const SOUTH: u8 = 1;
const WEST: u8 = 2;
const NORTH: u8 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Point {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl Point {
    pub(crate) fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub(crate) fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) / 2, (self.y + other.y) / 2)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    from: Point,
    to: Point,
    direction: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct Region {
    pub(crate) color: u8,
    pub(crate) material: Material,
    pub(crate) cells: Vec<usize>,
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

