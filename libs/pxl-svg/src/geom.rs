//! Geometry: affine transforms, path data, basic shapes, curve flattening and
//! stroke expansion. Curves are transformed to pixel space before they are
//! flattened (an affine map keeps a Bézier a Bézier), so the tolerance is in
//! output pixels whatever the viewBox.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::num::{atan2, deg, number_list, sincos, Nums, PI};

pub type Pt = (f64, f64);

/// `[a c e; b d f]`: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    /// `self` after `other`: apply `other` first.
    pub fn then(&self, other: &Affine) -> Affine {
        // self * other
        Affine {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    pub fn apply(&self, p: Pt) -> Pt {
        (
            self.a * p.0 + self.c * p.1 + self.e,
            self.b * p.0 + self.d * p.1 + self.f,
        )
    }

    pub fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn invert(&self) -> Option<Affine> {
        let det = self.det();
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    /// Average linear scale, for stroke widths.
    pub fn mean_scale(&self) -> f64 {
        self.det().abs().sqrt()
    }
}

/// Parse a `transform` attribute. Unknown functions are an error string.
pub fn parse_transform(s: &str) -> Result<Affine, String> {
    let mut m = Affine::IDENTITY;
    let mut rest = s.trim();
    while !rest.is_empty() {
        let open = rest
            .find('(')
            .ok_or_else(|| format!("bad transform '{s}'"))?;
        let close = rest
            .find(')')
            .ok_or_else(|| format!("bad transform '{s}'"))?;
        if close < open {
            return Err(format!("bad transform '{s}'"));
        }
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let v = number_list(&rest[open + 1..close]);
        let get = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
        let t = match name {
            "matrix" if v.len() == 6 => Affine {
                a: v[0],
                b: v[1],
                c: v[2],
                d: v[3],
                e: v[4],
                f: v[5],
            },
            "translate" if !v.is_empty() => Affine::translate(v[0], get(1, 0.0)),
            "scale" if !v.is_empty() => Affine::scale(v[0], get(1, v[0])),
            "rotate" if !v.is_empty() => {
                let (sn, cs) = sincos(deg(v[0]));
                let r = Affine {
                    a: cs,
                    b: sn,
                    c: -sn,
                    d: cs,
                    e: 0.0,
                    f: 0.0,
                };
                if v.len() >= 3 {
                    Affine::translate(v[1], v[2])
                        .then(&r)
                        .then(&Affine::translate(-v[1], -v[2]))
                } else {
                    r
                }
            }
            "skewX" if v.len() == 1 => {
                let (sn, cs) = sincos(deg(v[0]));
                Affine {
                    c: sn / cs,
                    ..Affine::IDENTITY
                }
            }
            "skewY" if v.len() == 1 => {
                let (sn, cs) = sincos(deg(v[0]));
                Affine {
                    b: sn / cs,
                    ..Affine::IDENTITY
                }
            }
            _ => return Err(format!("unsupported transform '{name}'")),
        };
        m = m.then(&t);
        rest = rest[close + 1..].trim();
    }
    Ok(m)
}

/// One subpath in pixel space after flattening.
#[derive(Clone, Debug, Default)]
pub struct Poly {
    pub pts: Vec<Pt>,
    pub closed: bool,
}

/// Builds flattened subpaths. Points arrive in user space; `m` maps them.
pub struct PathBuilder {
    m: Affine,
    pub polys: Vec<Poly>,
    cur: Poly,
    /// Point a command after `Z` continues from.
    pending: Option<Pt>,
}

fn dist(a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    (dx * dx + dy * dy).sqrt()
}

impl PathBuilder {
    pub fn new(m: Affine) -> Self {
        Self {
            m,
            polys: Vec::new(),
            cur: Poly::default(),
            pending: None,
        }
    }

    fn flush(&mut self) {
        if self.cur.pts.len() >= 2 || (self.cur.pts.len() == 1 && !self.cur.closed) {
            self.polys.push(core::mem::take(&mut self.cur));
        } else {
            self.cur = Poly::default();
        }
    }

    pub fn move_to(&mut self, p: Pt) {
        self.flush();
        self.pending = None;
        self.cur.pts.push(self.m.apply(p));
    }

    /// Continue from the pending point after a `Z` when the subpath is empty.
    fn prepare(&mut self) {
        if self.cur.pts.is_empty() {
            if let Some(p) = self.pending.take() {
                self.cur.pts.push(p);
            }
        }
    }

    pub fn line_to(&mut self, p: Pt) {
        self.prepare();
        let q = self.m.apply(p);
        if self.cur.pts.last() != Some(&q) {
            self.cur.pts.push(q);
        }
    }

    pub fn cubic_to(&mut self, p1: Pt, p2: Pt, p3: Pt) {
        self.prepare();
        let p0 = *self.cur.pts.last().unwrap_or(&self.m.apply(p1));
        let (q1, q2, q3) = (self.m.apply(p1), self.m.apply(p2), self.m.apply(p3));
        let len = dist(p0, q1) + dist(q1, q2) + dist(q2, q3);
        let n = ((len * 1.5).ceil() as usize).clamp(2, 64);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            let q = (
                b0 * p0.0 + b1 * q1.0 + b2 * q2.0 + b3 * q3.0,
                b0 * p0.1 + b1 * q1.1 + b2 * q2.1 + b3 * q3.1,
            );
            if self.cur.pts.last() != Some(&q) {
                self.cur.pts.push(q);
            }
        }
    }

    pub fn close(&mut self) {
        self.cur.closed = true;
        // A command after Z continues from the subpath's first point.
        self.pending = self.cur.pts.first().copied();
        self.flush();
    }

    pub fn finish(mut self) -> Vec<Poly> {
        self.flush();
        self.polys
    }
}

/// Arc from `p0` to `p` (SVG endpoint form) as cubic segments, user space.
#[allow(clippy::too_many_arguments)]
fn arc_to(
    b: &mut PathBuilder,
    p0: Pt,
    rx: f64,
    ry: f64,
    phi_deg: f64,
    large: bool,
    sweep: bool,
    p: Pt,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || p0 == p {
        b.line_to(p);
        return;
    }
    let (sp, cp) = sincos(deg(phi_deg));
    let dx2 = (p0.0 - p.0) / 2.0;
    let dy2 = (p0.1 - p.1) / 2.0;
    let x1 = cp * dx2 + sp * dy2;
    let y1 = -sp * dx2 + cp * dy2;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den == 0.0 {
        0.0
    } else {
        (num / den).max(0.0).sqrt()
    };
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let cx = cp * cx1 - sp * cy1 + (p0.0 + p.0) / 2.0;
    let cy = sp * cx1 + cp * cy1 + (p0.1 + p.1) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| atan2(ux * vy - uy * vx, ux * vx + uy * vy);
    let theta1 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dtheta = ang(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }
    let segs = ((dtheta.abs() / (PI / 2.0)).ceil() as usize).max(1);
    let delta = dtheta / segs as f64;
    let k = 4.0 / 3.0 * {
        let (s, c) = sincos(delta / 4.0);
        s / c
    };
    let point = |t: f64| {
        let (st, ct) = sincos(t);
        (
            cx + rx * ct * cp - ry * st * sp,
            cy + rx * ct * sp + ry * st * cp,
        )
    };
    let deriv = |t: f64| {
        let (st, ct) = sincos(t);
        (-rx * st * cp - ry * ct * sp, -rx * st * sp + ry * ct * cp)
    };
    let mut t = theta1;
    for i in 0..segs {
        let t2 = t + delta;
        let (a, d1) = (point(t), deriv(t));
        let (e, d2) = (if i + 1 == segs { p } else { point(t2) }, deriv(t2));
        b.cubic_to(
            (a.0 + k * d1.0, a.1 + k * d1.1),
            (e.0 - k * d2.0, e.1 - k * d2.1),
            e,
        );
        t = t2;
    }
}

/// Parse path data into `b`. Returns an error message on malformed data;
/// what was parsed before the error is kept, as SVG renderers do.
pub fn path(d: &str, b: &mut PathBuilder) -> Result<(), String> {
    let mut n = Nums::new(d);
    let mut cur: Pt = (0.0, 0.0);
    let mut start: Pt = (0.0, 0.0);
    let mut last_ctrl: Option<(u8, Pt)> = None;
    let mut cmd = 0u8;
    let mut seen_moveto = false;
    loop {
        n.skip_sep();
        let Some(c) = n.peek() else {
            return Ok(());
        };
        if c.is_ascii_alphabetic() {
            if !seen_moveto && !matches!(c, b'M' | b'm') {
                return Err("path data must start with a moveto".to_string());
            }
            cmd = c;
            n.i += 1;
            seen_moveto = true;
            if cmd == b'z' || cmd == b'Z' {
                b.close();
                cur = start;
                last_ctrl = None;
                continue;
            }
        } else if cmd == 0 {
            return Err(format!(
                "path data must start with a command near byte {}",
                n.i
            ));
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let mut num = || {
            n.number()
                .ok_or_else(|| "path data ends inside a command".to_string())
        };
        let upper = cmd.to_ascii_uppercase();
        match upper {
            b'M' => {
                let p = (base.0 + num()?, base.1 + num()?);
                b.move_to(p);
                cur = p;
                start = p;
                // Further pairs are implicit line-tos.
                cmd = if rel { b'l' } else { b'L' };
                last_ctrl = None;
            }
            b'L' => {
                let p = (base.0 + num()?, base.1 + num()?);
                b.line_to(p);
                cur = p;
                last_ctrl = None;
            }
            b'H' => {
                let x = num()?;
                cur = (if rel { cur.0 + x } else { x }, cur.1);
                b.line_to(cur);
                last_ctrl = None;
            }
            b'V' => {
                let y = num()?;
                cur = (cur.0, if rel { cur.1 + y } else { y });
                b.line_to(cur);
                last_ctrl = None;
            }
            b'C' => {
                let p1 = (base.0 + num()?, base.1 + num()?);
                let p2 = (base.0 + num()?, base.1 + num()?);
                let p = (base.0 + num()?, base.1 + num()?);
                b.cubic_to(p1, p2, p);
                last_ctrl = Some((b'C', p2));
                cur = p;
            }
            b'S' => {
                let p2 = (base.0 + num()?, base.1 + num()?);
                let p = (base.0 + num()?, base.1 + num()?);
                let p1 = match last_ctrl {
                    Some((b'C', c)) => (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1),
                    _ => cur,
                };
                b.cubic_to(p1, p2, p);
                last_ctrl = Some((b'C', p2));
                cur = p;
            }
            b'Q' | b'T' => {
                let q = if upper == b'Q' {
                    (base.0 + num()?, base.1 + num()?)
                } else {
                    match last_ctrl {
                        Some((b'Q', c)) => (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1),
                        _ => cur,
                    }
                };
                let p = (base.0 + num()?, base.1 + num()?);
                let p1 = (
                    cur.0 + 2.0 / 3.0 * (q.0 - cur.0),
                    cur.1 + 2.0 / 3.0 * (q.1 - cur.1),
                );
                let p2 = (p.0 + 2.0 / 3.0 * (q.0 - p.0), p.1 + 2.0 / 3.0 * (q.1 - p.1));
                b.cubic_to(p1, p2, p);
                last_ctrl = Some((b'Q', q));
                cur = p;
            }
            b'A' => {
                let rx = num()?;
                let ry = num()?;
                let phi = num()?;
                drop(num);
                let large = n.flag().ok_or("bad arc flag")?;
                let sweep = n.flag().ok_or("bad arc flag")?;
                let x = n.number().ok_or("path data ends inside an arc")?;
                let y = n.number().ok_or("path data ends inside an arc")?;
                let p = (base.0 + x, base.1 + y);
                arc_to(b, cur, rx, ry, phi, large, sweep, p);
                cur = p;
                last_ctrl = None;
            }
            other => return Err(format!("unknown path command '{}'", other as char)),
        }
    }
}

/// Bézier circle constant.
const KAPPA: f64 = 0.552_284_749_830_793_4;

pub fn ellipse(b: &mut PathBuilder, cx: f64, cy: f64, rx: f64, ry: f64) {
    if rx <= 0.0 || ry <= 0.0 {
        return;
    }
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    b.move_to((cx + rx, cy));
    b.cubic_to((cx + rx, cy + ky), (cx + kx, cy + ry), (cx, cy + ry));
    b.cubic_to((cx - kx, cy + ry), (cx - rx, cy + ky), (cx - rx, cy));
    b.cubic_to((cx - rx, cy - ky), (cx - kx, cy - ry), (cx, cy - ry));
    b.cubic_to((cx + kx, cy - ry), (cx + rx, cy - ky), (cx + rx, cy));
    b.close();
}

pub fn rect(b: &mut PathBuilder, x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let rx = rx.clamp(0.0, w / 2.0);
    let ry = ry.clamp(0.0, h / 2.0);
    if rx == 0.0 || ry == 0.0 {
        b.move_to((x, y));
        b.line_to((x + w, y));
        b.line_to((x + w, y + h));
        b.line_to((x, y + h));
        b.close();
        return;
    }
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    b.move_to((x + rx, y));
    b.line_to((x + w - rx, y));
    b.cubic_to((x + w - rx + kx, y), (x + w, y + ry - ky), (x + w, y + ry));
    b.line_to((x + w, y + h - ry));
    b.cubic_to(
        (x + w, y + h - ry + ky),
        (x + w - rx + kx, y + h),
        (x + w - rx, y + h),
    );
    b.line_to((x + rx, y + h));
    b.cubic_to((x + rx - kx, y + h), (x, y + h - ry + ky), (x, y + h - ry));
    b.line_to((x, y + ry));
    b.cubic_to((x, y + ry - ky), (x + rx - kx, y), (x + rx, y));
    b.close();
}

pub fn points(b: &mut PathBuilder, list: &str, close: bool) {
    let v = number_list(list);
    for (i, p) in v.chunks_exact(2).enumerate() {
        if i == 0 {
            b.move_to((p[0], p[1]));
        } else {
            b.line_to((p[0], p[1]));
        }
    }
    if close && v.len() >= 6 {
        b.close();
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn transforms_compose_in_svg_order() {
        let m = parse_transform("translate(10,0) scale(2)").unwrap();
        assert_eq!(m.apply((1.0, 1.0)), (12.0, 2.0));
        let r = parse_transform("rotate(90)").unwrap();
        let p = r.apply((1.0, 0.0));
        assert!(close(p.0, 0.0) && close(p.1, 1.0));
        let r = parse_transform("rotate(180 5 5)").unwrap();
        let p = r.apply((0.0, 0.0));
        assert!(close(p.0, 10.0) && close(p.1, 10.0));
        let inv = m.invert().unwrap();
        let q = inv.apply(m.apply((3.0, -4.0)));
        assert!(close(q.0, 3.0) && close(q.1, -4.0));
        assert!(parse_transform("perspective(2)").is_err());
    }

    #[test]
    fn path_commands_end_where_expected() {
        let mut b = PathBuilder::new(Affine::IDENTITY);
        path(
            "M1 1 h4 v4 H1 z m10 0 l2 2 c1 0 2 1 2 2 s1 1 2 0 q1 1 2 0 t2 0 a2 2 0 0 1 4 0",
            &mut b,
        )
        .unwrap();
        let polys = b.finish();
        assert_eq!(polys.len(), 2);
        assert!(polys[0].closed);
        assert_eq!(
            polys[0].pts,
            vec![(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0)]
        );
        let end = *polys[1].pts.last().unwrap();
        assert!(close(end.0, 25.0) && close(end.1, 5.0), "{end:?}");
        let mut b = PathBuilder::new(Affine::IDENTITY);
        assert!(path("L 1 2", &mut b).is_err());
    }

    #[test]
    fn a_half_arc_passes_through_its_apex() {
        let mut b = PathBuilder::new(Affine::IDENTITY);
        path("M0 0 A 5 5 0 0 1 10 0", &mut b).unwrap();
        let polys = b.finish();
        let min_y = polys[0].pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!((min_y + 5.0).abs() < 0.05, "{min_y}");
    }

    #[test]
    fn stroke_pieces_cover_segments_and_joins() {
        let poly = Poly {
            pts: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            closed: false,
        };
        let p = stroke(&[poly.clone()], 2.0, Join::Miter, Cap::Butt, 4.0);
        assert_eq!(p.len(), 3);
        let r = stroke(&[poly], 2.0, Join::Round, Cap::Round, 4.0);
        assert_eq!(r.len(), 5);
    }
}
