//! Ellipse, polygon and thick-line primitives for the shape rasterizer.
use pxl_core::{IndexFrame, PxlError, PxlResult};

use super::bresenham;

/// Ellipse inscribed in the box `[x, y, w, h]`. Pixel centres are tested in
/// doubled coordinates so the maths stays in integers: with the box centre at
/// `(2x + w, 2y + h)` and semi-axes `w`, `h` (all doubled), pixel `(px, py)`
/// centre `(2px + 1, 2py + 1)` is inside when
/// `((2px+1-cx)/w)^2 + ((2py+1-cy)/h)^2 <= 1`, evaluated as
/// `dx^2 * h^2 + dy^2 * w^2 <= w^2 * h^2`.
pub(super) fn draw_ellipse(stamp: &mut IndexFrame, e: [i32; 4], c: u8, fill: bool) -> PxlResult<()> {
    let [x, y, w, h] = e;
    if w <= 0 || h <= 0 {
        return Err(PxlError::schema(
            "ellipse width and height must be positive",
        ));
    }
    let (x, y, w, h) = (x as i64, y as i64, w as i64, h as i64);
    let inside = |px: i64, py: i64| -> bool {
        let dx = 2 * px + 1 - (2 * x + w);
        let dy = 2 * py + 1 - (2 * y + h);
        dx * dx * h * h + dy * dy * w * w <= w * w * h * h
    };
    let (cw, ch) = (stamp.w as i64, stamp.h as i64);
    for py in y.max(0)..(y + h).min(ch) {
        for px in x.max(0)..(x + w).min(cw) {
            if !inside(px, py) {
                continue;
            }
            // A ring keeps only pixels with a 4-neighbour outside the ellipse.
            let edge = !inside(px - 1, py)
                || !inside(px + 1, py)
                || !inside(px, py - 1)
                || !inside(px, py + 1);
            if fill || edge {
                stamp.set(px as i32, py as i32, c);
            }
        }
    }
    Ok(())
}

/// Even-odd polygon fill. A pixel is inside when a ray from its centre toward +x
/// crosses an odd number of edges. Crossings use the half-open rule
/// `min(y0,y1) <= cy < max(y0,y1)`, so a vertex shared by two edges is counted
/// once, and the x position is compared exactly with integer cross products.
pub(super) fn draw_polygon(stamp: &mut IndexFrame, pts: &[[i32; 2]], c: u8) -> PxlResult<()> {
    if pts.len() < 3 {
        return Err(PxlError::schema("polygon needs at least 3 points"));
    }
    if pts.len() > 256 {
        return Err(PxlError::schema("polygon has more than 256 points"));
    }
    let p: Vec<(i64, i64)> = pts
        .iter()
        .map(|[x, y]| (2 * *x as i64, 2 * *y as i64))
        .collect();
    let (min_y, max_y) = p.iter().fold((i64::MAX, i64::MIN), |(lo, hi), &(_, y)| {
        (lo.min(y), hi.max(y))
    });
    let (min_x, max_x) = p.iter().fold((i64::MAX, i64::MIN), |(lo, hi), &(x, _)| {
        (lo.min(x), hi.max(x))
    });
    let (cw, ch) = (stamp.w as i64, stamp.h as i64);
    // Pixel (px, py) covers [px, px+1) so its centre is (2px+1, 2py+1) in the
    // doubled space; clip the scan to the polygon bounds and the canvas.
    let y0 = (min_y.div_euclid(2) - 1).max(0);
    let y1 = (max_y.div_euclid(2) + 1).min(ch - 1);
    let x0 = (min_x.div_euclid(2) - 1).max(0);
    let x1 = (max_x.div_euclid(2) + 1).min(cw - 1);
    for py in y0..=y1 {
        let cy = 2 * py + 1;
        for px in x0..=x1 {
            let cx = 2 * px + 1;
            let mut inside = false;
            for i in 0..p.len() {
                let (ax, ay) = p[i];
                let (bx, by) = p[(i + 1) % p.len()];
                if (ay <= cy) == (by <= cy) {
                    continue;
                }
                // Is the crossing to the right of the pixel centre?
                //   x_cross = ax + (cy - ay) * (bx - ax) / (by - ay)  >=  cx
                // Compared as a cross product in i128: coordinates reach 2^32,
                // so a product of two of them overflows i64.
                let num = (cy - ay) as i128 * (bx - ax) as i128;
                let den = (by - ay) as i128;
                let lhs = (cx - ax) as i128 * den;
                let right = if den > 0 { num >= lhs } else { num <= lhs };
                if right {
                    inside = !inside;
                }
            }
            if inside {
                stamp.set(px as i32, py as i32, c);
            }
        }
    }
    Ok(())
}

/// A `width`-pixel line: the single-pixel Bresenham path with a square brush of
/// `width` pixels centred on each step. Width 1 equals a plain line.
pub(super) fn draw_thick_line(stamp: &mut IndexFrame, l: [i32; 4], width: u8, c: u8) -> PxlResult<()> {
    if width == 0 || width > 16 {
        return Err(PxlError::schema("line width must be in 1..=16"));
    }
    let mut path = IndexFrame::canvas(stamp.w, stamp.h)?;
    let [x0, y0, x1, y1] = l;
    bresenham(&mut path, x0, y0, x1, y1, 1)?;
    let lo = -((width as i32 - 1) / 2);
    let hi = width as i32 / 2;
    for y in 0..stamp.h as i32 {
        for x in 0..stamp.w as i32 {
            if path.get(x, y) == 0 {
                continue;
            }
            for dy in lo..=hi {
                for dx in lo..=hi {
                    stamp.set(x + dx, y + dy, c);
                }
            }
        }
    }
    // A brush centred off-canvas can still reach in; replay the ends unclipped.
    for (ex, ey) in [(x0, y0), (x1, y1)] {
        for dy in lo..=hi {
            for dx in lo..=hi {
                if let (Some(px), Some(py)) = (ex.checked_add(dx), ey.checked_add(dy)) {
                    stamp.set(px, py, c);
                }
            }
        }
    }
    Ok(())
}
