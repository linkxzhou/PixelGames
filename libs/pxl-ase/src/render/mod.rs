//! Layer flatten / composite (merged `pxl-ase-render`).

use pxl_ase_doc::{CelPixels, ColorMode, LayerKind, Sprite};
use pxl_core::Rgba;

use crate::{AseErr, Result};

/// Composite visible image layers bottom→top for one frame into RGBA8.
pub fn flatten_frame_rgba(sprite: &Sprite, frame: u16) -> Result<Vec<u8>> {
    let w = sprite.width as usize;
    let h = sprite.height as usize;
    let mut out = vec![0u8; w * h * 4];
    for layer in &sprite.layers {
        if layer.kind != LayerKind::Image || !layer.flags.visible {
            continue;
        }
        if layer.opacity == 0 {
            continue;
        }
        let Ok(cel) = sprite.cel_pixels(layer.id, frame) else {
            continue;
        };
        blit_cel(&mut out, w, h, &cel, sprite, layer.opacity);
    }
    Ok(out)
}

fn blit_cel(
    out: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    cel: &CelPixels,
    sprite: &Sprite,
    layer_opacity: u8,
) {
    match cel {
        CelPixels::Rgba {
            image,
            x,
            y,
            opacity,
        } => {
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = *x as i32 + px;
                    let dy = *y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                        continue;
                    }
                    let si = (py as usize * image.width as usize + px as usize) * 4;
                    let src = Rgba {
                        r: image.pixels[si],
                        g: image.pixels[si + 1],
                        b: image.pixels[si + 2],
                        a: image.pixels[si + 3],
                    };
                    let a = scale_alpha(src.a, *opacity, layer_opacity);
                    if a == 0 {
                        continue;
                    }
                    let di = (dy as usize * canvas_w + dx as usize) * 4;
                    blend_over(out, di, src.r, src.g, src.b, a);
                }
            }
        }
        CelPixels::Indexed {
            image,
            x,
            y,
            opacity,
        } => {
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = *x as i32 + px;
                    let dy = *y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                        continue;
                    }
                    let idx = image.pixels[py as usize * image.width as usize + px as usize];
                    if idx == sprite.transparent_index {
                        continue;
                    }
                    let color = sprite
                        .palette
                        .get(idx as usize)
                        .copied()
                        .unwrap_or(Rgba::TRANSPARENT);
                    let a = scale_alpha(color.a, *opacity, layer_opacity);
                    if a == 0 {
                        continue;
                    }
                    let di = (dy as usize * canvas_w + dx as usize) * 4;
                    blend_over(out, di, color.r, color.g, color.b, a);
                }
            }
        }
        CelPixels::Gray {
            image,
            x,
            y,
            opacity,
        } => {
            let iw = image.width as i32;
            let ih = image.height as i32;
            for py in 0..ih {
                for px in 0..iw {
                    let dx = *x as i32 + px;
                    let dy = *y as i32 + py;
                    if dx < 0 || dy < 0 || dx >= canvas_w as i32 || dy >= canvas_h as i32 {
                        continue;
                    }
                    let si = (py as usize * image.width as usize + px as usize) * 2;
                    let v = image.pixels[si];
                    let a = scale_alpha(image.pixels[si + 1], *opacity, layer_opacity);
                    if a == 0 {
                        continue;
                    }
                    let di = (dy as usize * canvas_w + dx as usize) * 4;
                    blend_over(out, di, v, v, v, a);
                }
            }
        }
    }
    let _ = sprite.color_mode; // silence if unused in future branches
}

fn scale_alpha(a: u8, cel: u8, layer: u8) -> u8 {
    let v = (a as u32) * (cel as u32) * (layer as u32) / (255 * 255);
    v.min(255) as u8
}

fn blend_over(out: &mut [u8], di: usize, r: u8, g: u8, b: u8, a: u8) {
    if a == 255 {
        out[di] = r;
        out[di + 1] = g;
        out[di + 2] = b;
        out[di + 3] = 255;
        return;
    }
    let da = out[di + 3] as u32;
    let sa = a as u32;
    let out_a = sa + da * (255 - sa) / 255;
    if out_a == 0 {
        out[di] = 0;
        out[di + 1] = 0;
        out[di + 2] = 0;
        out[di + 3] = 0;
        return;
    }
    let inv = 255 - sa;
    out[di] = ((r as u32 * sa + out[di] as u32 * da * inv / 255) / out_a) as u8;
    out[di + 1] = ((g as u32 * sa + out[di + 1] as u32 * da * inv / 255) / out_a) as u8;
    out[di + 2] = ((b as u32 * sa + out[di + 2] as u32 * da * inv / 255) / out_a) as u8;
    out[di + 3] = out_a as u8;
}

pub fn flatten_all_frames(sprite: &Sprite) -> Result<Vec<Vec<u8>>> {
    let mut frames = Vec::new();
    for f in 0..sprite.frame_count() {
        frames.push(flatten_frame_rgba(sprite, f as u16)?);
    }
    if frames.is_empty() {
        return Err(AseErr("sprite has no frames".into()));
    }
    let _ = ColorMode::Rgba;
    Ok(frames)
}
