//! GE-1 convolution matrix filter (identity / sharpen / blur + JSON custom).

use std::sync::Arc;

use pxl_ase_doc::{ImageBuffer, Sprite};
use serde_json::Value;

use super::opts::FilterOpts;
use crate::{AseErr, Result};

#[derive(Clone, Debug)]
pub struct Kernel {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
    pub divisor: f32,
    pub bias: f32,
}

impl Kernel {
    pub fn identity() -> Self {
        Self {
            width: 1,
            height: 1,
            values: vec![1.0],
            divisor: 1.0,
            bias: 0.0,
        }
    }

    pub fn sharpen() -> Self {
        Self {
            width: 3,
            height: 3,
            values: vec![0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0],
            divisor: 1.0,
            bias: 0.0,
        }
    }

    pub fn blur() -> Self {
        Self {
            width: 3,
            height: 3,
            values: vec![1.0; 9],
            divisor: 9.0,
            bias: 0.0,
        }
    }

    pub fn from_json(v: &Value) -> Result<Self> {
        let width = v.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let height = v.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        if width == 0 || height == 0 || width % 2 == 0 || height % 2 == 0 {
            return Err(AseErr(
                "convolution kernel width/height must be odd and >0".into(),
            ));
        }
        let values: Vec<f32> = v
            .get("values")
            .and_then(|x| x.as_array())
            .ok_or_else(|| AseErr("kernel.values array required".into()))?
            .iter()
            .map(|n| n.as_f64().unwrap_or(0.0) as f32)
            .collect();
        if values.len() != width * height {
            return Err(AseErr(format!(
                "kernel.values len {} != {}x{}",
                values.len(),
                width,
                height
            )));
        }
        let divisor = v
            .get("divisor")
            .and_then(|x| x.as_f64())
            .unwrap_or(1.0) as f32;
        if divisor.abs() < 1e-6 {
            return Err(AseErr("kernel divisor must be non-zero".into()));
        }
        let bias = v.get("bias").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        Ok(Self {
            width,
            height,
            values,
            divisor,
            bias,
        })
    }

    pub fn named(name: &str) -> Result<Self> {
        match name {
            "identity" => Ok(Self::identity()),
            "sharpen" => Ok(Self::sharpen()),
            "blur" | "box-blur" => Ok(Self::blur()),
            other => Err(AseErr(format!(
                "unknown kernel '{other}' (identity|sharpen|blur) or pass --kernel-json"
            ))),
        }
    }
}

fn convolve_rgba(w: u16, h: u16, src: &[u8], k: &Kernel, opts: FilterOpts) -> Vec<u8> {
    let (w, h) = (w as i32, h as i32);
    let kw = k.width as i32;
    let kh = k.height as i32;
    let ox = kw / 2;
    let oy = kh / 2;
    let mut out = src.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for ky in 0..kh {
                for kx in 0..kw {
                    let (sx, sy) = opts.boundary.sample(x + kx - ox, y + ky - oy, w, h);
                    let si = ((sy * w + sx) * 4) as usize;
                    let kv = k.values[(ky * kw + kx) as usize];
                    for c in 0..4 {
                        acc[c] += src[si + c] as f32 * kv;
                    }
                }
            }
            let di = ((y * w + x) * 4) as usize;
            let mut filtered = [0u8; 4];
            for c in 0..4 {
                let v = (acc[c] / k.divisor + k.bias).round().clamp(0.0, 255.0);
                filtered[c] = v as u8;
            }
            // Default keeps source alpha unless channels.a.
            if !opts.channels.a {
                filtered[3] = src[di + 3];
            }
            opts.channels.write_into(&mut out[di..di + 4], &filtered);
        }
    }
    out
}

/// Apply convolution to every RGBA image buffer. Indexed/gray skipped.
pub fn convolve_sprite(sprite: &mut Sprite, kernel: &Kernel) -> Result<usize> {
    convolve_sprite_opts(sprite, kernel, FilterOpts::default())
}

pub fn convolve_sprite_opts(
    sprite: &mut Sprite,
    kernel: &Kernel,
    opts: FilterOpts,
) -> Result<usize> {
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Rgba(buf) = img {
            let px = convolve_rgba(buf.width, buf.height, buf.pixels.as_ref(), kernel, opts);
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
    fn identity_kernel_preserves_pixels() {
        let mut s = Sprite::new(2, 1, ColorMode::Rgba);
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        convolve_sprite(&mut s, &Kernel::identity()).unwrap();
        if let ImageBuffer::Rgba(b) = s.images.values().next().unwrap() {
            assert_eq!(&b.pixels[..4], &[10, 20, 30, 255]);
        }
    }

    #[test]
    fn rejects_even_json_kernel() {
        let v = serde_json::json!({"width":2,"height":2,"values":[1,1,1,1]});
        assert!(Kernel::from_json(&v).is_err());
    }
}
