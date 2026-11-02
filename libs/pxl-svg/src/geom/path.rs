//! Path data parsing, curve flattening and basic shape builders.
use alloc::string::String;
use alloc::vec::Vec;

use crate::num::{atan2, deg, number_list, sincos, Nums, PI};
use super::{Affine, Pt};

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

pub(crate) fn dist(a: Pt, b: Pt) -> f64 {
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
