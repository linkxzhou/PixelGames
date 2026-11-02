//! Integer grid doubling on index frames. Nothing here blends colours: every
//! output pixel is one of the source pixel's own colour or a 4-neighbour's.

use crate::{IndexFrame, PxlError, PxlResult};

fn doubled(frame: &IndexFrame, k: u16) -> PxlResult<IndexFrame> {
    let w = frame.w.checked_mul(k);
    let h = frame.h.checked_mul(k);
    match (w, h) {
        (Some(w), Some(h)) => IndexFrame::new(w, h),
        _ => Err(PxlError::schema(alloc::format!(
            "{}x{} scaled {k}x overflows the frame size",
            frame.w,
            frame.h
        ))),
    }
}

/// Nearest `k x k` copy of every pixel. The honest baseline: no new detail.
pub fn scale_copy(frame: &IndexFrame, k: u16) -> PxlResult<IndexFrame> {
    if k == 0 {
        return Err(PxlError::schema("scale must be at least 1"));
    }
    let mut out = doubled(frame, k)?;
    let k = k as i32;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let c = frame.get(x, y);
            if c == 0 {
                continue;
            }
            for dy in 0..k {
                for dx in 0..k {
                    out.set(x * k + dx, y * k + dy, c);
                }
            }
        }
    }
    Ok(out)
}

/// Plain nearest 2x.
pub fn double_copy(frame: &IndexFrame) -> PxlResult<IndexFrame> {
    scale_copy(frame, 2)
}

/// EPX / Scale2x. A sub-pixel takes a neighbour's colour when its two adjacent
/// neighbours agree and differ from the other pair.
///
/// Outside the frame counts as transparent, with two departures from the
/// textbook rule that keep sprites intact: transparent pixels are never filled
/// (a shape can only lose corners, never grow), and a corner whose two
/// neighbours are both off-canvas is left alone, so art that fills a canvas
/// corner is not clipped there.
pub fn epx2x(frame: &IndexFrame) -> PxlResult<IndexFrame> {
    let mut out = doubled(frame, 2)?;
    for y in 0..frame.h as i32 {
        for x in 0..frame.w as i32 {
            let p = frame.get(x, y);
            if p == 0 {
                continue;
            }
            let n = |dx: i32, dy: i32| (frame.get(x + dx, y + dy), frame.in_bounds(x + dx, y + dy));
            let (a, b, c, d) = (n(0, -1), n(1, 0), n(-1, 0), n(0, 1));
            // n1/n2 are the two neighbours touching the corner, o1/o2 the
            // opposite ones.
            let rule = |n1: (u8, bool), n2: (u8, bool), o1: (u8, bool), o2: (u8, bool)| -> u8 {
                if (n1.1 || n2.1) && n1.0 == n2.0 && n1.0 != o1.0 && n2.0 != o2.0 {
                    n1.0
                } else {
                    p
                }
            };
            let tl = rule(c, a, d, b);
            let tr = rule(a, b, c, d);
            let bl = rule(d, c, b, a);
            let br = rule(b, d, a, c);
            out.set(x * 2, y * 2, tl);
            out.set(x * 2 + 1, y * 2, tr);
            out.set(x * 2, y * 2 + 1, bl);
            out.set(x * 2 + 1, y * 2 + 1, br);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_matrix, encode_matrix};
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    fn frame(rows: &[&str]) -> IndexFrame {
        let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        decode_matrix(&rows).unwrap()
    }

    #[test]
    fn copy_matches_every_block() {
        let f = frame(&["12", "03"]);
        let out = scale_copy(&f, 3).unwrap();
        assert_eq!((out.w, out.h), (6, 6));
        for y in 0..6 {
            for x in 0..6 {
                assert_eq!(out.get(x, y), f.get(x / 3, y / 3));
            }
        }
    }

    #[test]
    fn epx_rounds_a_detached_block_and_keeps_the_edge() {
        let out = epx2x(&frame(&["0000", "0110", "0110", "0000"])).unwrap();
        assert_eq!(out.get(2, 2), 0);
        assert_eq!(out.get(3, 3), 1);
        let edge = epx2x(&frame(&["11", "10"])).unwrap();
        assert_eq!(
            encode_matrix(&edge).unwrap(),
            ["1111", "1111", "11..", "11.."]
        );
    }

    #[test]
    fn epx_never_fills_transparency() {
        let out = epx2x(&frame(&["010", "101", "010"])).unwrap();
        assert_eq!(out.get(2, 2), 0);
        assert_eq!(out.get(3, 3), 0);
    }

    #[test]
    fn oversize_is_an_error() {
        let f = IndexFrame::new(200, 1).unwrap();
        assert!(double_copy(&f).is_err());
        assert!(scale_copy(&f, 0).is_err());
    }
}
