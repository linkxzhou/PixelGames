//! Stroke expansion: joins, caps and outline polygons.
use alloc::vec;
use alloc::vec::Vec;

use super::{Pt, Poly};
use super::path::dist;
use crate::num::{sincos, PI};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

fn disc(c: Pt, r: f64) -> Vec<Pt> {
    let n = ((r * 4.0).ceil() as usize).clamp(8, 48);
    (0..n)
        .map(|i| {
            let (s, co) = sincos(2.0 * PI * i as f64 / n as f64);
            (c.0 + r * co, c.1 + r * s)
        })
        .collect()
}

/// Expand a stroke into convex pieces. Each piece is filled on its own and
/// the results are unioned, so overlapping pieces never cancel.
pub fn stroke(polys: &[Poly], width: f64, join: Join, cap: Cap, miter_limit: f64) -> Vec<Vec<Pt>> {
    let hw = width / 2.0;
    let mut out = Vec::new();
    if hw <= 0.0 {
        return out;
    }
    for poly in polys {
        let mut pts = poly.pts.clone();
        if poly.closed && pts.len() > 2 {
            pts.push(pts[0]);
        }
        if pts.len() == 1 {
            if cap != Cap::Butt {
                out.push(disc(pts[0], hw));
            }
            continue;
        }
        let segs: Vec<(Pt, Pt, Pt)> = pts
            .windows(2)
            .filter_map(|w| {
                let (a, b) = (w[0], w[1]);
                let l = dist(a, b);
                if l == 0.0 {
                    None
                } else {
                    Some((a, b, (-(b.1 - a.1) / l * hw, (b.0 - a.0) / l * hw)))
                }
            })
            .collect();
        for (i, (a, b, n)) in segs.iter().enumerate() {
            let (mut a, mut b) = (*a, *b);
            let l = dist(a, b);
            let dir = ((b.0 - a.0) / l, (b.1 - a.1) / l);
            if cap == Cap::Square && !poly.closed {
                if i == 0 {
                    a = (a.0 - dir.0 * hw, a.1 - dir.1 * hw);
                }
                if i + 1 == segs.len() {
                    b = (b.0 + dir.0 * hw, b.1 + dir.1 * hw);
                }
            }
            out.push(vec![
                (a.0 + n.0, a.1 + n.1),
                (b.0 + n.0, b.1 + n.1),
                (b.0 - n.0, b.1 - n.1),
                (a.0 - n.0, a.1 - n.1),
            ]);
        }
        // Joins between consecutive segments (and the closing one).
        let mut joints: Vec<(usize, usize)> = (1..segs.len()).map(|i| (i - 1, i)).collect();
        if poly.closed && segs.len() > 2 {
            joints.push((segs.len() - 1, 0));
        }
        for (i, j) in joints {
            let (_, p, n0) = segs[i];
            let (_, _, n1) = segs[j];
            match join {
                Join::Round => out.push(disc(p, hw)),
                Join::Bevel | Join::Miter => {
                    let cross = n0.0 * n1.1 - n0.1 * n1.0;
                    // Outer side: the normals on the convex side of the turn.
                    let s = if cross > 0.0 { -1.0 } else { 1.0 };
                    let o0 = (p.0 + s * n0.0, p.1 + s * n0.1);
                    let o1 = (p.0 + s * n1.0, p.1 + s * n1.1);
                    let mut piece = vec![p, o0, o1];
                    if join == Join::Miter {
                        let dot = (n0.0 * n1.0 + n0.1 * n1.1) / (hw * hw);
                        let cos_half = ((1.0 + dot) / 2.0).max(0.0).sqrt();
                        if cos_half > 0.0 && 1.0 / cos_half <= miter_limit {
                            let mx = (n0.0 + n1.0, n0.1 + n1.1);
                            let ml = (mx.0 * mx.0 + mx.1 * mx.1).sqrt();
                            if ml > 0.0 {
                                let len = hw / cos_half;
                                let tip = (p.0 + s * mx.0 / ml * len, p.1 + s * mx.1 / ml * len);
                                piece = vec![p, o0, tip, o1];
                            }
                        }
                    }
                    out.push(piece);
                }
            }
        }
        if cap == Cap::Round && !poly.closed {
            out.push(disc(pts[0], hw));
            out.push(disc(*pts.last().unwrap(), hw));
        }
    }
    out
}
