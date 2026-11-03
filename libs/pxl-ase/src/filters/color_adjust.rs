//! Headless filters (merged `pxl-ase-filters`).

use std::sync::Arc;

use pxl_ase_doc::{ColorMode, ImageBuffer, Sprite};
use pxl_core::Rgba;

use crate::render::flatten_frame_rgba;
use crate::{AseErr, Result};

pub fn replace_color_rgba(sprite: &mut Sprite, from: Rgba, to: Rgba) -> Result<usize> {
    replace_color_rgba_tol(sprite, from, to, 0)
}

/// Replace RGBA within Chebyshev distance `tolerance` on each channel.
pub fn replace_color_rgba_tol(
    sprite: &mut Sprite,
    from: Rgba,
    to: Rgba,
    tolerance: u8,
) -> Result<usize> {
    let tol = tolerance as i16;
    let near = |a: u8, b: u8| (a as i16 - b as i16).abs() <= tol;
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if near(chunk[0], from.r)
                        && near(chunk[1], from.g)
                        && near(chunk[2], from.b)
                        && near(chunk[3], from.a)
                    {
                        chunk[0] = to.r;
                        chunk[1] = to.g;
                        chunk[2] = to.b;
                        chunk[3] = to.a;
                        n += 1;
                    }
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(_) | ImageBuffer::Grayscale(_) => {}
        }
    }
    Ok(n)
}

pub fn replace_index(sprite: &mut Sprite, from: u8, to: u8) -> Result<usize> {
    if sprite.color_mode != ColorMode::Indexed {
        return Err(AseErr("replace_index needs indexed mode".into()));
    }
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Indexed(buf) = img {
            let mut px = buf.pixels.as_ref().clone();
            for p in &mut px {
                if *p == from {
                    *p = to;
                    n += 1;
                }
            }
            buf.pixels = Arc::new(px);
        }
    }
    Ok(n)
}

/// Invert RGB (keep alpha) on RGBA buffers; invert indices around mid for indexed.
pub fn invert(sprite: &mut Sprite) -> Result<usize> {
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    chunk[0] = 255 - chunk[0];
                    chunk[1] = 255 - chunk[1];
                    chunk[2] = 255 - chunk[2];
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Grayscale(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(2) {
                    chunk[0] = 255 - chunk[0];
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(buf) => {
                let max = sprite.palette.len().saturating_sub(1).max(1) as u8;
                let mut px = buf.pixels.as_ref().clone();
                for p in &mut px {
                    if *p != sprite.transparent_index {
                        *p = max.saturating_sub(*p);
                        n += 1;
                    }
                }
                buf.pixels = Arc::new(px);
            }
        }
    }
    Ok(n)
}

/// Brightness/contrast on RGBA (and grayscale) buffers.
/// `brightness` in -255..=255, `contrast` in -100..=100 (percent-ish).
pub fn brightness_contrast(sprite: &mut Sprite, brightness: i32, contrast: i32) -> Result<usize> {
    let b = brightness.clamp(-255, 255);
    let c = contrast.clamp(-100, 100);
    // contrast factor: map -100..100 → 0..2 around 1.0
    let factor = (100 + c) as f32 / 100.0;
    let adj = |v: u8| -> u8 {
        let mut x = v as f32;
        x = (x - 128.0) * factor + 128.0 + b as f32;
        x.round().clamp(0.0, 255.0) as u8
    };
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if chunk[3] == 0 {
                        continue;
                    }
                    chunk[0] = adj(chunk[0]);
                    chunk[1] = adj(chunk[1]);
                    chunk[2] = adj(chunk[2]);
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Grayscale(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(2) {
                    if chunk[1] == 0 {
                        continue;
                    }
                    chunk[0] = adj(chunk[0]);
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(_) => {}
        }
    }
    // For indexed-only docs, flatten→RGBA adjust→write back as RGBA layer.
    if n == 0 && sprite.color_mode == ColorMode::Indexed {
        let frames = sprite.frame_count();
        let mut flats = Vec::with_capacity(frames);
        for f in 0..frames {
            flats.push(flatten_frame_rgba(sprite, f as u16)?);
        }
        sprite.layers.clear();
        sprite.images.clear();
        let layer = sprite.add_image_layer("Adjusted");
        for (fi, mut rgba) in flats.into_iter().enumerate() {
            for chunk in rgba.chunks_exact_mut(4) {
                if chunk[3] == 0 {
                    continue;
                }
                chunk[0] = adj(chunk[0]);
                chunk[1] = adj(chunk[1]);
                chunk[2] = adj(chunk[2]);
                n += 1;
            }
            let img = pxl_ase_doc::RgbaImage::from_rgba(sprite.width, sprite.height, rgba);
            let id = sprite.insert_image(ImageBuffer::Rgba(img));
            sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
        }
        sprite.color_mode = ColorMode::Rgba;
    }
    Ok(n)
}


pub fn hue_saturation(sprite: &mut Sprite, hue_deg: i32, sat: i32, value: i32) -> Result<usize> {
    if sprite.color_mode == ColorMode::Indexed {
        // Rasterize to RGBA so we do not scramble palette ramps.
        let frames = sprite.frame_count();
        let mut flats = Vec::with_capacity(frames);
        for f in 0..frames {
            flats.push(flatten_frame_rgba(sprite, f as u16)?);
        }
        sprite.layers.clear();
        sprite.images.clear();
        let layer = sprite.add_image_layer("HueSat");
        for (fi, rgba) in flats.into_iter().enumerate() {
            let img = pxl_ase_doc::RgbaImage::from_rgba(sprite.width, sprite.height, rgba);
            let id = sprite.insert_image(ImageBuffer::Rgba(img));
            sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
        }
        sprite.color_mode = ColorMode::Rgba;
    }
    let hue = ((hue_deg % 360) + 360) % 360;
    let sat = sat.clamp(-100, 100) as f32 / 100.0;
    let val = value.clamp(-100, 100) as f32 / 100.0;
    let mut n = 0usize;
    for img in sprite.images.values_mut() {
        match img {
            ImageBuffer::Rgba(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(4) {
                    if chunk[3] == 0 {
                        continue;
                    }
                    let (h, s, v) = rgb_to_hsv(chunk[0], chunk[1], chunk[2]);
                    let mut nh = (h + hue as f32) % 360.0;
                    if nh < 0.0 {
                        nh += 360.0;
                    }
                    let ns = (s + sat).clamp(0.0, 1.0);
                    let nv = (v + val).clamp(0.0, 1.0);
                    let (r, g, b) = hsv_to_rgb(nh, ns, nv);
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Grayscale(buf) => {
                let mut px = buf.pixels.as_ref().clone();
                for chunk in px.chunks_exact_mut(2) {
                    if chunk[1] == 0 {
                        continue;
                    }
                    let v = (chunk[0] as f32 / 255.0 + val).clamp(0.0, 1.0);
                    chunk[0] = (v * 255.0).round() as u8;
                    n += 1;
                }
                buf.pixels = Arc::new(px);
            }
            ImageBuffer::Indexed(_) => {}
        }
    }
    Ok(n)
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let v = max;
    let s = if max <= 1e-6 { 0.0 } else { delta / max };
    let h = if delta <= 1e-6 {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if max == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (rp, gp, bp) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((rp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((gp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((bp + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}
