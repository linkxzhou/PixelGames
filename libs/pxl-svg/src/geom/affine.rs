//! Affine transforms and SVG transform attribute parsing.
use alloc::string::String;

use crate::num::{deg, number_list, sincos};

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

