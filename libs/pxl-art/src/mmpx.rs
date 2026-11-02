//! MMPX: style-preserving 2x magnification (McGuire & Gagiu, JCGT 10(2), 2021;
//! rule set from the MIT-licensed reference implementation).
//!
//! Each source pixel `E` becomes four sub-pixels `J K / L M`. The rules look at
//! the 3x3 neighbourhood plus the four pixels two steps away (`P` up, `Q`
//! left, `R` right, `S` down) and only ever copy an existing colour, so the
//! output stays indexed. Compared with EPX it keeps 1-px lines, text and
//! checkerboards intact and resolves 2:1 slopes instead of leaving steps.
//! Luminance breaks ties between foreground and background; transparent is
//! treated as the brightest value, like the reference does with alpha 0.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::{IndexFrame, Oklab, Palette, PxlResult};

fn luma_table(palette: &Palette) -> Vec<i64> {
    let mut out = vec![i64::MAX / 4; 256];
    for (i, c) in palette.colors.iter().enumerate().take(256) {
        if c.a != 0 {
            out[i] = Oklab::from_rgba(*c).l as i64;
        }
    }
    out
}

/// One MMPX 2x pass. Outside the frame reads as transparent.
pub fn mmpx2x(frame: &IndexFrame, palette: &Palette) -> PxlResult<IndexFrame> {
    let mut out = IndexFrame::canvas(frame.w * 2, frame.h * 2)?;
    let luma = luma_table(palette);
    let l = |c: u8| luma[c as usize];
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let s = |dx: i32, dy: i32| frame.get(x + dx, y + dy);
            let (a, b, c) = (s(-1, -1), s(0, -1), s(1, -1));
            let (d, e, f) = (s(-1, 0), s(0, 0), s(1, 0));
            let (g, h, i) = (s(-1, 1), s(0, 1), s(1, 1));
            let (mut j, mut k, mut ll, mut m) = (e, e, e, e);
            let flat = [a, b, c, d, f, g, h, i].iter().all(|v| *v == e);
            if !flat {
                let (p, q, r, ss) = (s(0, -2), s(-2, 0), s(2, 0), s(0, 2));
                let (bl, dl, el, fl, hl) = (l(b), l(d), l(e), l(f), l(h));
                let eq3 = |v: u8, x0: u8, x1: u8, x2: u8| v == x0 && v == x1 && v == x2;
                let eq4 = |v: u8, x0: u8, x1: u8, x2: u8, x3: u8| eq3(v, x0, x1, x2) && v == x3;
                let any3 = |v: u8, x0: u8, x1: u8, x2: u8| v == x0 || v == x1 || v == x2;
                let none2 = |v: u8, x0: u8, x1: u8| v != x0 && v != x1;
                let none4 = |v: u8, x0: u8, x1: u8, x2: u8, x3: u8| {
                    v != x0 && v != x1 && v != x2 && v != x3
                };

                // 1:1 slopes.
                if d == b
                    && d != h
                    && d != f
                    && (el >= dl || e == a)
                    && any3(e, a, c, g)
                    && (el < dl || a != d || e != p || e != q)
                {
                    j = d;
                }
                if b == f
                    && b != d
                    && b != h
                    && (el >= bl || e == c)
                    && any3(e, a, c, i)
                    && (el < bl || c != b || e != p || e != r)
                {
                    k = b;
                }
                if h == d
                    && h != f
                    && h != b
                    && (el >= hl || e == g)
                    && any3(e, a, g, i)
                    && (el < hl || g != h || e != ss || e != q)
                {
                    ll = h;
                }
                if f == h
                    && f != b
                    && f != d
                    && (el >= fl || e == i)
                    && any3(e, c, g, i)
                    && (el < fl || i != h || e != r || e != ss)
                {
                    m = f;
                }

                // Intersections.
                if e != f && eq4(e, c, i, d, q) && f == b && f == h && f != s(3, 0) {
                    k = f;
                    m = f;
                }
                if e != d && eq4(e, a, g, f, r) && d == b && d == h && d != s(-3, 0) {
                    j = d;
                    ll = d;
                }
                if e != h && eq4(e, g, i, b, p) && h == d && h == f && h != s(0, 3) {
                    ll = h;
                    m = h;
                }
                if e != b && eq4(e, a, c, h, ss) && b == d && b == f && b != s(0, -3) {
                    j = b;
                    k = b;
                }

                // Triangle tips.
                if bl < el && eq4(e, g, h, i, ss) && none4(e, a, d, c, f) {
                    j = b;
                    k = b;
                }
                if hl < el && eq4(e, a, b, c, p) && none4(e, d, g, i, f) {
                    ll = h;
                    m = h;
                }
                if fl < el && eq4(e, a, d, g, q) && none4(e, b, c, i, h) {
                    k = f;
                    m = f;
                }
                if dl < el && eq4(e, c, f, i, r) && none4(e, b, a, g, h) {
                    j = d;
                    ll = d;
                }

                // 2:1 slopes.
                if h != b {
                    if h != a && h != e && h != c {
                        if eq3(h, g, f, r) && none2(h, d, s(2, -1)) {
                            ll = m;
                        }
                        if eq3(h, i, d, q) && none2(h, f, s(-2, -1)) {
                            m = ll;
                        }
                    }
                    if b != i && b != g && b != e {
                        if eq3(b, a, f, r) && none2(b, d, s(2, 1)) {
                            j = k;
                        }
                        if eq3(b, c, d, q) && none2(b, f, s(-2, 1)) {
                            k = j;
                        }
                    }
                }
                if f != d {
                    if d != i && d != e && d != c {
                        if eq3(d, a, h, ss) && none2(d, b, s(1, 2)) {
                            j = ll;
                        }
                        if eq3(d, g, b, p) && none2(d, h, s(1, -2)) {
                            ll = j;
                        }
                    }
                    if f != e && f != a && f != g {
                        if eq3(f, c, h, ss) && none2(f, b, s(-1, 2)) {
                            k = m;
                        }
                        if eq3(f, i, b, p) && none2(f, h, s(-1, -2)) {
                            m = k;
                        }
                    }
                }
            }
            out.set(x * 2, y * 2, j);
            out.set(x * 2 + 1, y * 2, k);
            out.set(x * 2, y * 2 + 1, ll);
            out.set(x * 2 + 1, y * 2 + 1, m);
        }
    }
    Ok(out)
}

/// MMPX at 2x or 4x (two passes).
pub fn mmpx(frame: &IndexFrame, k: u16, palette: &Palette) -> PxlResult<IndexFrame> {
    let once = mmpx2x(frame, palette)?;
    if k == 4 {
        mmpx2x(&once, palette)
    } else {
        Ok(once)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use pxl_core::Rgba;

    fn pal() -> Palette {
        Palette {
            id: "t".into(),
            colors: alloc::vec![
                Rgba::TRANSPARENT,
                Rgba::rgb(20, 20, 30),
                Rgba::rgb(220, 80, 60)
            ],
            ramps: Default::default(),
            cycles: Default::default(),
            special: pxl_core::Special::default(),
        }
    }

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    #[test]
    fn flat_areas_and_single_lines_are_copied() {
        let f = frame(&["....", ".11.", "....", "2222"]);
        let out = mmpx2x(&f, &pal()).unwrap();
        // A one-pixel line keeps its full length at 2x.
        for x in 0..8 {
            assert_eq!(out.get(x, 6), 2);
            assert_eq!(out.get(x, 7), 2);
        }
        assert_eq!(out.get(2, 2), 1);
        assert_eq!(out.get(5, 3), 1);
    }

    #[test]
    fn a_diagonal_staircase_is_smoothed_without_new_colours() {
        let f = frame(&["1...", "11..", "111.", "1111"]);
        let out = mmpx2x(&f, &pal()).unwrap();
        assert_ne!(out, pxl_core::scale_copy(&f, 2).unwrap());
        for p in &out.pixels {
            assert!(*p <= 2);
        }
        assert_eq!(mmpx2x(&f, &pal()).unwrap(), out, "deterministic");
    }

    #[test]
    fn four_x_is_two_passes() {
        let f = frame(&["12", "21"]);
        let out = mmpx(&f, 4, &pal()).unwrap();
        assert_eq!((out.w, out.h), (8, 8));
    }
}
