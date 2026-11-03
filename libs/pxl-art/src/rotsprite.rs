//! RotSprite-style rotation (Xenowhirl): magnify 8x with a pixel-guessing
//! upscaler, rotate by sampling the magnified image, and come back to the
//! native grid by sampling once per output pixel. Like nearest rotation it
//! never invents a colour, but the 8x guess means diagonals stay one pixel
//! wide and single pixels are not dropped or doubled at odd angles.
//!
//! Angles are whole degrees, positive is clockwise on screen (y grows down).
//! The canvas keeps its size; art rotated past the edge is clipped.

use alloc::vec;
use alloc::vec::Vec;

use pxl_core::trig::{cos_q16, sin_q16};
use pxl_core::{IndexFrame, PxlResult};

/// EPX on a raw buffer. Outside the image reads as transparent, and a
/// transparent pixel is never filled, so a shape can lose corners but not grow.
fn scale2x(w: usize, h: usize, src: &[u8]) -> Vec<u8> {
    let (ow, oh) = (w * 2, h * 2);
    let mut out = vec![0u8; ow * oh];
    let at = |x: isize, y: isize| -> u8 {
        if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
            0
        } else {
            src[y as usize * w + x as usize]
        }
    };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let p = at(x, y);
            let (a, b, c, d) = (at(x, y - 1), at(x + 1, y), at(x - 1, y), at(x, y + 1));
            let mut q = [p; 4];
            if p != 0 {
                if c == a && c != d && a != b {
                    q[0] = a;
                }
                if a == b && a != c && b != d {
                    q[1] = b;
                }
                if d == c && d != b && c != a {
                    q[2] = c;
                }
                if b == d && b != a && d != c {
                    q[3] = d;
                }
            }
            let (ox, oy) = (x as usize * 2, y as usize * 2);
            out[oy * ow + ox] = q[0];
            out[oy * ow + ox + 1] = q[1];
            out[(oy + 1) * ow + ox] = q[2];
            out[(oy + 1) * ow + ox + 1] = q[3];
        }
    }
    out
}

/// Rotate `frame` by `deg` around `pivot` (pixel coordinates; the canvas
/// centre when `None`).
pub fn rotsprite(frame: &IndexFrame, deg: i32, pivot: Option<(i32, i32)>) -> PxlResult<IndexFrame> {
    let deg = deg.rem_euclid(360);
    if deg == 0 {
        return Ok(frame.clone());
    }
    let (w, h) = (frame.w as usize, frame.h as usize);
    let mut big = frame.pixels.clone();
    let (mut bw, mut bh) = (w, h);
    for _ in 0..3 {
        big = scale2x(bw, bh, &big);
        bw *= 2;
        bh *= 2;
    }
    // Everything below is in 1/8 pixel units.
    let (px, py) = match pivot {
        Some((x, y)) => (x as i64 * 8, y as i64 * 8),
        None => (w as i64 * 4, h as i64 * 4),
    };
    let (s, c) = (sin_q16(deg) as i64, cos_q16(deg) as i64);
    let mut out = frame.blank_like();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let dx = x * 8 + 4 - px;
            let dy = y * 8 + 4 - py;
            // Inverse of the clockwise rotation, rounded to the nearest 1/8 px.
            let sx = (c * dx + s * dy + 32768).div_euclid(65536) + px;
            let sy = (-s * dx + c * dy + 32768).div_euclid(65536) + py;
            if sx < 0 || sy < 0 || sx >= bw as i64 || sy >= bh as i64 {
                continue;
            }
            out.pixels[y as usize * w + x as usize] = big[sy as usize * bw + sx as usize];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        pxl_core::decode_matrix(&rows).unwrap()
    }

    fn rot90(f: &IndexFrame) -> IndexFrame {
        let mut out = f.blank_like();
        let n = f.w as i32;
        for y in 0..n {
            for x in 0..n {
                out.set(n - 1 - y, x, f.get(x, y));
            }
        }
        out
    }

    #[test]
    fn right_angles_are_exact() {
        let f = frame(&["1...", "12..", "1.3.", "1111"]);
        assert_eq!(rotsprite(&f, 90, None).unwrap(), rot90(&f));
        assert_eq!(rotsprite(&f, 360, None).unwrap(), f);
        assert_eq!(
            rotsprite(&f, 180, None).unwrap(),
            rot90(&rot90(&f)),
            "two quarter turns"
        );
    }

    #[test]
    fn odd_angles_keep_colours_and_roughly_the_area() {
        let mut f = IndexFrame::new(16, 16).unwrap();
        for y in 4..12 {
            for x in 3..13 {
                f.set(x, y, if y == 4 { 2 } else { 1 });
            }
        }
        let out = rotsprite(&f, 30, None).unwrap();
        for p in &out.pixels {
            assert!(*p <= 2);
        }
        let (a, b) = (f.count_opaque() as i32, out.count_opaque() as i32);
        assert!((a - b).abs() * 10 < a, "{a} vs {b}");
        assert!(out.pixels.iter().any(|p| *p == 2), "the stripe survives");
        assert_eq!(rotsprite(&f, 30, None).unwrap(), out);
    }
}
