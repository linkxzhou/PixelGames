//! GE-3 color curve (minimal LUT remap).

use std::sync::Arc;

use pxl_ase_doc::{ImageBuffer, Sprite};

use crate::{AseErr, Result};

/// Build a 256-entry LUT from control points `[[x,y],...]` with x,y in 0..=255.
pub fn lut_from_points(points: &[(u8, u8)]) -> Result<[u8; 256]> {
    if points.len() < 2 {
        return Err(AseErr("color curve needs ≥2 control points".into()));
    }
    let mut pts = points.to_vec();
    pts.sort_by_key(|(x, _)| *x);
    let mut lut = [0u8; 256];
    for i in 0..256u16 {
        let x = i as u8;
        // find segment
        let mut y = pts[0].1 as i32;
        for w in pts.windows(2) {
            let (x0, y0) = (w[0].0 as i32, w[0].1 as i32);
            let (x1, y1) = (w[1].0 as i32, w[1].1 as i32);
            if i as i32 >= x0 && i as i32 <= x1 {
                if x1 == x0 {
                    y = y1;
                } else {
                    y = y0 + (y1 - y0) * (i as i32 - x0) / (x1 - x0);
                }
                break;
            }
            if i as i32 > x1 {
                y = y1;
            }
        }
        lut[i as usize] = y.clamp(0, 255) as u8;
        let _ = x;
    }
    Ok(lut)
}

pub fn apply_curve_lut(sprite: &mut Sprite, lut: &[u8; 256]) -> Result<usize> {
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Rgba(buf) = img {
            let mut px = buf.pixels.as_ref().clone();
            for chunk in px.chunks_exact_mut(4) {
                chunk[0] = lut[chunk[0] as usize];
                chunk[1] = lut[chunk[1] as usize];
                chunk[2] = lut[chunk[2] as usize];
            }
            buf.pixels = Arc::new(px);
            n += 1;
        }
    }
    Ok(n)
}

/// Convenience: identity→gentle contrast S-curve via mid lift.
pub fn contrast_curve_lut() -> [u8; 256] {
    lut_from_points(&[(0, 0), (64, 48), (192, 208), (255, 255)]).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_endpoints() {
        let lut = lut_from_points(&[(0, 0), (255, 255)]).unwrap();
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
        assert_eq!(lut[128], 128);
    }
}
