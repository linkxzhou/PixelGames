//! Numbers and trigonometry for SVG geometry.
//!
//! Geometry uses `f64`, but only the IEEE operations that are exactly rounded
//! on every target (+ - * / sqrt, floor/ceil/round, string parsing). Sine and
//! arctangent are evaluated here with fixed polynomials instead of the
//! platform libm, so the same SVG gives the same pixels on macOS, Linux,
//! Windows and wasm.

pub const PI: f64 = core::f64::consts::PI;
const HALF_PI: f64 = core::f64::consts::FRAC_PI_2;
const QUARTER_PI: f64 = core::f64::consts::FRAC_PI_4;
/// tan(pi/8).
const TAN_EIGHTH: f64 = 0.414_213_562_373_095_1;

/// `(sin x, cos x)`.
pub fn sincos(x: f64) -> (f64, f64) {
    if !x.is_finite() {
        return (0.0, 1.0);
    }
    let q = (x / HALF_PI).round();
    let r = x - q * HALF_PI;
    let r2 = r * r;
    let s = r
        * (1.0
            + r2 * (-1.0 / 6.0
                + r2 * (1.0 / 120.0
                    + r2 * (-1.0 / 5040.0 + r2 * (1.0 / 362_880.0 + r2 * (-1.0 / 39_916_800.0))))));
    let c = 1.0
        + r2 * (-0.5
            + r2 * (1.0 / 24.0
                + r2 * (-1.0 / 720.0 + r2 * (1.0 / 40_320.0 + r2 * (-1.0 / 3_628_800.0)))));
    match (q as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

pub fn deg(d: f64) -> f64 {
    d * PI / 180.0
}

fn atan_small(t: f64) -> f64 {
    // |t| <= tan(pi/8): the series converges fast enough for 1e-12.
    let t2 = t * t;
    let mut term = t;
    let mut sum = t;
    let mut k = 1.0;
    for _ in 0..14 {
        term *= -t2;
        k += 2.0;
        sum += term / k;
    }
    sum
}

pub fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return 0.0;
    }
    if x < 0.0 {
        return -atan(-x);
    }
    if x > 1.0 {
        return HALF_PI - atan(1.0 / x);
    }
    if x > TAN_EIGHTH {
        return QUARTER_PI + atan_small((x - 1.0) / (x + 1.0));
    }
    atan_small(x)
}

pub fn atan2(y: f64, x: f64) -> f64 {
    if x > 0.0 {
        atan(y / x)
    } else if x < 0.0 {
        if y >= 0.0 {
            atan(y / x) + PI
        } else {
            atan(y / x) - PI
        }
    } else if y > 0.0 {
        HALF_PI
    } else if y < 0.0 {
        -HALF_PI
    } else {
        0.0
    }
}

/// Scanner for SVG number lists and path data: `10-5` is two numbers,
/// `1.5.5` is `1.5` and `.5`; commas and whitespace separate.
pub struct Nums<'a> {
    s: &'a [u8],
    pub i: usize,
}

impl<'a> Nums<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            i: 0,
        }
    }

    pub fn skip_sep(&mut self) {
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',')
        {
            self.i += 1;
        }
    }

    pub fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    pub fn number(&mut self) -> Option<f64> {
        self.skip_sep();
        let start = self.i;
        let mut j = self.i;
        if j < self.s.len() && (self.s[j] == b'+' || self.s[j] == b'-') {
            j += 1;
        }
        let mut digits = 0;
        while j < self.s.len() && self.s[j].is_ascii_digit() {
            j += 1;
            digits += 1;
        }
        if j < self.s.len() && self.s[j] == b'.' {
            j += 1;
            while j < self.s.len() && self.s[j].is_ascii_digit() {
                j += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        if j < self.s.len() && (self.s[j] == b'e' || self.s[j] == b'E') {
            let mut k = j + 1;
            if k < self.s.len() && (self.s[k] == b'+' || self.s[k] == b'-') {
                k += 1;
            }
            let ds = k;
            while k < self.s.len() && self.s[k].is_ascii_digit() {
                k += 1;
            }
            if k > ds {
                j = k;
            }
        }
        let text = core::str::from_utf8(&self.s[start..j]).ok()?;
        let v: f64 = text.parse().ok()?;
        self.i = j;
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }

    /// Arc flags are a single `0` or `1`, possibly with no separator.
    pub fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        match self.peek()? {
            b'0' => {
                self.i += 1;
                Some(false)
            }
            b'1' => {
                self.i += 1;
                Some(true)
            }
            _ => None,
        }
    }
}

/// A plain number, optionally with a `px` unit.
pub fn number(s: &str) -> Option<f64> {
    let t = s.trim();
    let t = t.strip_suffix("px").unwrap_or(t).trim();
    let mut n = Nums::new(t);
    let v = n.number()?;
    n.skip_sep();
    if n.peek().is_some() {
        return None;
    }
    Some(v)
}

pub fn number_list(s: &str) -> alloc::vec::Vec<f64> {
    let mut n = Nums::new(s);
    let mut out = Vec::new();
    while let Some(v) = n.number() {
        out.push(v);
    }
    out
}

/// A gradient stop offset: a number or a percentage, clamped to 0..=1.
pub fn offset(s: &str) -> Option<f64> {
    let t = s.trim();
    let v = if let Some(p) = t.strip_suffix('%') {
        number(p)? / 100.0
    } else {
        number(t)?
    };
    Some(v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn trig_matches_known_values() {
        for (d, s, c) in [
            (0.0, 0.0, 1.0),
            (30.0, 0.5, 0.866_025_403_784_438_6),
            (90.0, 1.0, 0.0),
            (180.0, 0.0, -1.0),
            (
                -45.0,
                -core::f64::consts::FRAC_1_SQRT_2,
                core::f64::consts::FRAC_1_SQRT_2,
            ),
        ] {
            let (ss, cc) = sincos(deg(d));
            assert!(close(ss, s) && close(cc, c), "{d}: {ss} {cc}");
        }
        assert!(close(atan2(1.0, 1.0), QUARTER_PI));
        assert!(close(atan2(1.0, -1.0), 3.0 * QUARTER_PI));
        assert!(close(atan2(-2.0, 0.0), -HALF_PI));
        assert!(close(atan(0.3), 0.291_456_794_477_867_1));
        assert!(close(atan(5.0), 1.373_400_766_945_016));
    }

    #[test]
    fn numbers_split_like_svg() {
        assert_eq!(
            number_list("10-5 1.5.5,2e1 -.5e-1"),
            vec![10.0, -5.0, 1.5, 0.5, 20.0, -0.05]
        );
        assert_eq!(number("12px"), Some(12.0));
        assert_eq!(number("12%"), None);
        assert_eq!(offset("50%"), Some(0.5));
        let mut n = Nums::new("10 1 05");
        assert_eq!(n.number(), Some(10.0));
        assert_eq!(n.flag(), Some(true));
        assert_eq!(n.flag(), Some(false));
        assert_eq!(n.number(), Some(5.0));
    }
}
