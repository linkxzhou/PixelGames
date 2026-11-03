//! GE-2 median filter with radius cap.

use std::sync::Arc;

use pxl_ase_doc::{ImageBuffer, Sprite};

use super::opts::FilterOpts;
use crate::{AseErr, Result};

/// Hard cap — keeps cost O(n * r² log) bounded for CLI.
pub const MAX_MEDIAN_RADIUS: u32 = 3;

fn median_rgba(w: u16, h: u16, src: &[u8], radius: u32, opts: FilterOpts) -> Vec<u8> {
    let (w, h) = (w as i32, h as i32);
    let r = radius as i32;
    let mut out = src.to_vec();
    let mut channel = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
    for y in 0..h {
        for x in 0..w {
            let di = ((y * w + x) * 4) as usize;
            let mut filtered = src[di..di + 4].to_vec();
            for c in 0..4 {
                let want = [opts.channels.r, opts.channels.g, opts.channels.b, opts.channels.a][c];
                if !want {
                    continue;
                }
                channel.clear();
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (sx, sy) = opts.boundary.sample(x + dx, y + dy, w, h);
                        let si = ((sy * w + sx) * 4) as usize;
                        channel.push(src[si + c]);
                    }
                }
                channel.sort_unstable();
                filtered[c] = channel[channel.len() / 2];
            }
            opts.channels.write_into(&mut out[di..di + 4], &filtered);
        }
    }
    out
}

pub fn median_sprite(sprite: &mut Sprite, radius: u32) -> Result<usize> {
    median_sprite_opts(sprite, radius, FilterOpts::default())
}

pub fn median_sprite_opts(sprite: &mut Sprite, radius: u32, opts: FilterOpts) -> Result<usize> {
    if radius == 0 {
        return Ok(0);
    }
    if radius > MAX_MEDIAN_RADIUS {
        return Err(AseErr(format!(
            "median radius {radius} exceeds cap {MAX_MEDIAN_RADIUS}"
        )));
    }
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Rgba(buf) = img {
            let px = median_rgba(buf.width, buf.height, buf.pixels.as_ref(), radius, opts);
            buf.pixels = Arc::new(px);
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, RgbaImage};

    #[test]
    fn radius_cap_enforced() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        assert!(median_sprite(&mut s, 9).is_err());
    }

    #[test]
    fn median_runs() {
        let mut s = Sprite::new(3, 3, ColorMode::Rgba);
        let layer = s.add_image_layer("a");
        let mut px = vec![0u8; 3 * 3 * 4];
        for i in 0..9 {
            let o = i * 4;
            px[o] = (i * 20) as u8;
            px[o + 3] = 255;
        }
        let img = RgbaImage::from_rgba(3, 3, px);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        median_sprite(&mut s, 1).unwrap();
    }
}
