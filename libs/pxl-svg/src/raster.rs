//! Supersampled scanline coverage and the majority-vote downsample.
//!
//! Each shape is filled on an `N x N` sample grid per output pixel (sample
//! centres at `(i + 0.5) / N`), with the nonzero or even-odd rule. Samples
//! hold palette indices and the id of the shape that painted them, never a
//! blended colour. One output pixel is then the most common index among its
//! samples:
//!
//! * more than half the samples clear: the pixel is clear;
//! * exactly half: kept only when the covered half lies right (then bottom)
//!   of the pixel centre, so a 1-px line on a pixel boundary lands in one
//!   column, and a shape from x = 2.5 to 6.5 stays four pixels wide;
//! * ties between colours go to the shape painted last (on top).

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

pub type Pt = (f64, f64);

/// Filled sample mask over a bounding box of the sample grid.
pub struct Mask {
    pub x0: usize,
    pub y0: usize,
    pub w: usize,
    pub h: usize,
    pub bits: Vec<bool>,
}

impl Mask {
    #[cfg(test)]
    pub fn get(&self, sx: usize, sy: usize) -> bool {
        sx >= self.x0
            && sy >= self.y0
            && sx < self.x0 + self.w
            && sy < self.y0 + self.h
            && self.bits[(sy - self.y0) * self.w + sx - self.x0]
    }

    pub fn count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

fn signed_area(p: &[Pt]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (x0, y0) = p[i];
        let (x1, y1) = p[(i + 1) % p.len()];
        a += x0 * y1 - x1 * y0;
    }
    a
}

/// Fill `polys` (pixel space, implicitly closed) into a mask on a grid of
/// `gw x gh` samples with `n` samples per pixel. `union` orients every
/// polygon the same way first, so overlapping stroke pieces add up instead of
/// cancelling.
pub fn fill(
    polys: &[Vec<Pt>],
    rule: FillRule,
    union: bool,
    n: usize,
    gw: usize,
    gh: usize,
) -> Mask {
    let nf = n as f64;
    let mut edges: Vec<(f64, f64, f64, f64, i32)> = Vec::new();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for poly in polys {
        if poly.len() < 3 {
            continue;
        }
        let flip = union && signed_area(poly) < 0.0;
        for i in 0..poly.len() {
            let (mut a, mut b) = (poly[i], poly[(i + 1) % poly.len()]);
            if flip {
                core::mem::swap(&mut a, &mut b);
            }
            let (ax, ay, bx, by) = (a.0 * nf, a.1 * nf, b.0 * nf, b.1 * nf);
            if !(ax.is_finite() && ay.is_finite() && bx.is_finite() && by.is_finite()) {
                continue;
            }
            minx = minx.min(ax.min(bx));
            maxx = maxx.max(ax.max(bx));
            miny = miny.min(ay.min(by));
            maxy = maxy.max(ay.max(by));
            if ay == by {
                continue;
            }
            let dir = if by > ay { 1 } else { -1 };
            let (x0, y0, x1, y1) = if ay < by {
                (ax, ay, bx, by)
            } else {
                (bx, by, ax, ay)
            };
            edges.push((x0, y0, x1, y1, dir));
        }
    }
    let empty = Mask {
        x0: 0,
        y0: 0,
        w: 0,
        h: 0,
        bits: Vec::new(),
    };
    if edges.is_empty() {
        return empty;
    }
    let sx0 = (minx.floor().max(0.0)) as usize;
    let sy0 = (miny.floor().max(0.0)) as usize;
    let sx1 = (maxx.ceil().min(gw as f64)).max(0.0) as usize;
    let sy1 = (maxy.ceil().min(gh as f64)).max(0.0) as usize;
    if sx1 <= sx0 || sy1 <= sy0 {
        return empty;
    }
    let (w, h) = (sx1 - sx0, sy1 - sy0);
    let mut bits = vec![false; w * h];
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for row in 0..h {
        let yc = (sy0 + row) as f64 + 0.5;
        xs.clear();
        for &(x0, y0, x1, y1, dir) in &edges {
            if yc >= y0 && yc < y1 {
                let x = x0 + (yc - y0) * (x1 - x0) / (y1 - y0);
                xs.push((x, dir));
            }
        }
        xs.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        let mut wind = 0i32;
        for k in 0..xs.len() {
            wind += xs[k].1;
            let inside = match rule {
                FillRule::NonZero => wind != 0,
                FillRule::EvenOdd => (k + 1) % 2 == 1,
            };
            if !inside || k + 1 >= xs.len() {
                continue;
            }
            // Samples whose centre lies in [xs[k], xs[k+1]).
            let from = (xs[k].0 - 0.5).ceil().max(sx0 as f64) as isize;
            let to = (xs[k + 1].0 - 0.5).ceil().min(sx1 as f64) as isize;
            for sx in from..to {
                bits[row * w + (sx as usize - sx0)] = true;
            }
        }
    }
    Mask {
        x0: sx0,
        y0: sy0,
        w,
        h,
        bits,
    }
}

/// The sample buffer of one frame.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub n: usize,
    pub color: Vec<u8>,
    pub shape: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, n: usize) -> Self {
        let len = w * n * h * n;
        Self {
            w,
            h,
            n,
            color: vec![0; len],
            shape: vec![0; len],
        }
    }

    pub fn gw(&self) -> usize {
        self.w * self.n
    }

    pub fn gh(&self) -> usize {
        self.h * self.n
    }

    /// Paint the mask with a colour per sample (`paint` gets sample coords).
    /// `clip` is a full-grid sample mask.
    pub fn paint(
        &mut self,
        mask: &Mask,
        clip: Option<&[bool]>,
        id: u32,
        paint: &mut dyn FnMut(usize, usize) -> u8,
    ) {
        let gw = self.gw();
        for y in 0..mask.h {
            for x in 0..mask.w {
                if !mask.bits[y * mask.w + x] {
                    continue;
                }
                let (sx, sy) = (mask.x0 + x, mask.y0 + y);
                if let Some(c) = clip {
                    if !c[sy * gw + sx] {
                        continue;
                    }
                }
                let c = paint(sx, sy);
                if c == 0 {
                    continue;
                }
                self.color[sy * gw + sx] = c;
                self.shape[sy * gw + sx] = id;
            }
        }
    }
}

/// Result of the downsample: one index and one winning shape per pixel.
pub struct Resolved {
    pub pixels: Vec<u8>,
    /// The shape painted on top in that pixel.
    pub owner: Vec<u32>,
}

pub fn resolve(c: &Canvas) -> Resolved {
    let (w, h, n) = (c.w, c.h, c.n);
    let gw = c.gw();
    let total = n * n;
    let mut pixels = vec![0u8; w * h];
    let mut owner = vec![0u32; w * h];
    let mut counts: Vec<(u8, usize, u32)> = Vec::with_capacity(total);
    for py in 0..h {
        for px in 0..w {
            counts.clear();
            let (mut opaque, mut cx, mut cy) = (0usize, 0i64, 0i64);
            for sy in 0..n {
                for sx in 0..n {
                    let i = (py * n + sy) * gw + px * n + sx;
                    let col = c.color[i];
                    if col == 0 {
                        continue;
                    }
                    opaque += 1;
                    // Offsets from the pixel centre, doubled to stay integral.
                    cx += 2 * sx as i64 + 1 - n as i64;
                    cy += 2 * sy as i64 + 1 - n as i64;
                    match counts.iter_mut().find(|e| e.0 == col) {
                        Some(e) => {
                            e.1 += 1;
                            e.2 = e.2.max(c.shape[i]);
                        }
                        None => counts.push((col, 1, c.shape[i])),
                    }
                }
            }
            if opaque * 2 < total || opaque == 0 {
                continue;
            }
            if opaque * 2 == total && !(cx > 0 || (cx == 0 && cy > 0)) {
                continue;
            }
            let best = counts
                .iter()
                .max_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)))
                .copied()
                .unwrap_or((0, 0, 0));
            pixels[py * w + px] = best.0;
            owner[py * w + px] = best.2;
        }
    }
    Resolved { pixels, owner }
}

impl Mask {
    /// The mask spread over the full `gw x gh` grid.
    pub fn full(&self, gw: usize, gh: usize) -> Vec<bool> {
        let mut out = vec![false; gw * gh];
        for y in 0..self.h {
            for x in 0..self.w {
                if self.bits[y * self.w + x] {
                    out[(self.y0 + y) * gw + self.x0 + x] = true;
                }
            }
        }
        out
    }
}

/// For every shape still visible somewhere: `(pixel, samples, colour)` of
/// each pixel it shows in. One pass over the canvas.
pub fn visibility(c: &Canvas) -> BTreeMap<u32, Vec<(usize, usize, u8)>> {
    let (w, h, n) = (c.w, c.h, c.n);
    let gw = c.gw();
    let mut out: BTreeMap<u32, Vec<(usize, usize, u8)>> = Default::default();
    let mut local: Vec<(u32, usize, u8)> = Vec::new();
    for py in 0..h {
        for px in 0..w {
            local.clear();
            for sy in 0..n {
                for sx in 0..n {
                    let i = (py * n + sy) * gw + px * n + sx;
                    if c.color[i] == 0 {
                        continue;
                    }
                    let id = c.shape[i];
                    match local.iter_mut().find(|e| e.0 == id) {
                        Some(e) => e.1 += 1,
                        None => local.push((id, 1, c.color[i])),
                    }
                }
            }
            for (id, k, col) in &local {
                out.entry(*id).or_default().push((py * w + px, *k, *col));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    fn render(polys: &[Vec<Pt>], n: usize) -> Vec<u8> {
        let mut c = Canvas::new(8, 8, n);
        let m = fill(polys, FillRule::NonZero, false, n, 8 * n, 8 * n);
        c.paint(&m, None, 1, &mut |_, _| 1);
        resolve(&c).pixels
    }

    #[test]
    fn an_integer_rect_is_exact_at_every_sample_count() {
        for n in [1, 2, 4, 8] {
            let px = render(&[rect(2.0, 1.0, 5.0, 4.0)], n);
            for y in 0..8 {
                for x in 0..8 {
                    let inside = (2..5).contains(&x) && (1..4).contains(&y);
                    assert_eq!(px[y * 8 + x] == 1, inside, "n={n} {x},{y}");
                }
            }
        }
    }

    #[test]
    fn half_pixel_edges_keep_the_width() {
        let px = render(&[rect(2.5, 0.0, 6.5, 1.0)], 4);
        let row: Vec<u8> = px[..8].to_vec();
        assert_eq!(row.iter().filter(|v| **v == 1).count(), 4, "{row:?}");
        // A one-pixel line on a pixel boundary lands in exactly one column.
        let px = render(&[rect(2.5, 0.0, 3.5, 8.0)], 4);
        for y in 0..8 {
            assert_eq!((0..8).filter(|x| px[y * 8 + x] == 1).count(), 1);
        }
    }

    #[test]
    fn even_odd_cuts_a_hole_and_nonzero_does_not() {
        let outer = rect(0.0, 0.0, 8.0, 8.0);
        let inner = rect(2.0, 2.0, 6.0, 6.0);
        let mut c = Canvas::new(8, 8, 2);
        let m = fill(
            &[outer.clone(), inner.clone()],
            FillRule::EvenOdd,
            false,
            2,
            16,
            16,
        );
        c.paint(&m, None, 1, &mut |_, _| 1);
        assert_eq!(resolve(&c).pixels[4 * 8 + 4], 0);
        let px = render(&[outer, inner], 2);
        assert_eq!(px[4 * 8 + 4], 1);
    }

    #[test]
    fn union_orientation_keeps_overlaps() {
        let a = rect(0.0, 0.0, 4.0, 4.0);
        let mut b = rect(2.0, 0.0, 6.0, 4.0);
        b.reverse();
        let m = fill(&[a.clone(), b.clone()], FillRule::NonZero, true, 1, 8, 8);
        assert!(m.get(3, 1));
        let m = fill(&[a, b], FillRule::NonZero, false, 1, 8, 8);
        assert!(!m.get(3, 1), "opposite windings cancel without union");
    }
}
