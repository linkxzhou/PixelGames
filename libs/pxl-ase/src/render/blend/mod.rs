//! Clean-room RGBA blend modes (ase-file-specs / compositing-1).

mod hsl;
mod sep;

use pxl_ase_doc::BlendMode;

use hsl::{blend_color, blend_hue, blend_luminosity, blend_saturation};
use sep::{
    color_burn_u8, color_dodge_u8, divide_u8, exclusion_u8, hard_light_u8, mul_u8, overlay_u8,
    screen_u8, soft_light_u8,
};

/// Blend `src` over backdrop at `dst[di..]`. `sa` already includes cel/layer opacity.
pub fn blend_pixel(
    mode: BlendMode,
    dst: &mut [u8],
    di: usize,
    sr: u8,
    sg: u8,
    sb: u8,
    sa: u8,
) {
    if sa == 0 {
        return;
    }
    let (br, bg, bb, ba) = (dst[di], dst[di + 1], dst[di + 2], dst[di + 3]);
    let (cr, cg, cb) = channel_blend(mode, br, bg, bb, sr, sg, sb);
    composite_normal(dst, di, cr, cg, cb, sa, br, bg, bb, ba);
}

fn channel_blend(
    mode: BlendMode,
    br: u8,
    bg: u8,
    bb: u8,
    sr: u8,
    sg: u8,
    sb: u8,
) -> (u8, u8, u8) {
    match mode {
        BlendMode::Normal => (sr, sg, sb),
        BlendMode::Multiply => (mul_u8(br, sr), mul_u8(bg, sg), mul_u8(bb, sb)),
        BlendMode::Screen => (screen_u8(br, sr), screen_u8(bg, sg), screen_u8(bb, sb)),
        BlendMode::Overlay => (overlay_u8(br, sr), overlay_u8(bg, sg), overlay_u8(bb, sb)),
        BlendMode::Darken => (br.min(sr), bg.min(sg), bb.min(sb)),
        BlendMode::Lighten => (br.max(sr), bg.max(sg), bb.max(sb)),
        BlendMode::ColorDodge => (
            color_dodge_u8(br, sr),
            color_dodge_u8(bg, sg),
            color_dodge_u8(bb, sb),
        ),
        BlendMode::ColorBurn => (
            color_burn_u8(br, sr),
            color_burn_u8(bg, sg),
            color_burn_u8(bb, sb),
        ),
        BlendMode::HardLight => (
            hard_light_u8(br, sr),
            hard_light_u8(bg, sg),
            hard_light_u8(bb, sb),
        ),
        BlendMode::SoftLight => (
            soft_light_u8(br, sr),
            soft_light_u8(bg, sg),
            soft_light_u8(bb, sb),
        ),
        BlendMode::Difference => (
            (br as i16 - sr as i16).unsigned_abs() as u8,
            (bg as i16 - sg as i16).unsigned_abs() as u8,
            (bb as i16 - sb as i16).unsigned_abs() as u8,
        ),
        BlendMode::Exclusion => (
            exclusion_u8(br, sr),
            exclusion_u8(bg, sg),
            exclusion_u8(bb, sb),
        ),
        BlendMode::Hue => blend_hue(br, bg, bb, sr, sg, sb),
        BlendMode::Saturation => blend_saturation(br, bg, bb, sr, sg, sb),
        BlendMode::Color => blend_color(br, bg, bb, sr, sg, sb),
        BlendMode::Luminosity => blend_luminosity(br, bg, bb, sr, sg, sb),
        BlendMode::Addition => (
            br.saturating_add(sr),
            bg.saturating_add(sg),
            bb.saturating_add(sb),
        ),
        BlendMode::Subtract => (
            br.saturating_sub(sr),
            bg.saturating_sub(sg),
            bb.saturating_sub(sb),
        ),
        BlendMode::Divide => (divide_u8(br, sr), divide_u8(bg, sg), divide_u8(bb, sb)),
    }
}

fn composite_normal(
    dst: &mut [u8],
    di: usize,
    sr: u8,
    sg: u8,
    sb: u8,
    sa: u8,
    br: u8,
    bg: u8,
    bb: u8,
    ba: u8,
) {
    if sa == 255 {
        dst[di] = sr;
        dst[di + 1] = sg;
        dst[di + 2] = sb;
        dst[di + 3] = 255;
        return;
    }
    let sa = sa as u32;
    let ba = ba as u32;
    let out_a = sa + ba * (255 - sa) / 255;
    if out_a == 0 {
        dst[di] = 0;
        dst[di + 1] = 0;
        dst[di + 2] = 0;
        dst[di + 3] = 0;
        return;
    }
    let inv = 255 - sa;
    dst[di] = ((sr as u32 * sa + br as u32 * ba * inv / 255) / out_a) as u8;
    dst[di + 1] = ((sg as u32 * sa + bg as u32 * ba * inv / 255) / out_a) as u8;
    dst[di + 2] = ((sb as u32 * sa + bb as u32 * ba * inv / 255) / out_a) as u8;
    dst[di + 3] = out_a as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(mode: BlendMode, back: [u8; 4], src: [u8; 4]) -> [u8; 4] {
        let mut dst = back;
        blend_pixel(mode, &mut dst, 0, src[0], src[1], src[2], src[3]);
        dst
    }

    #[test]
    fn separable_smoke() {
        assert!(apply(BlendMode::Multiply, [200, 200, 200, 255], [128, 128, 128, 255])[0] < 200);
        assert!(apply(BlendMode::Screen, [100, 100, 100, 255], [100, 100, 100, 255])[0] > 100);
        assert_eq!(apply(BlendMode::Darken, [200, 0, 0, 255], [50, 0, 0, 255])[0], 50);
        assert_eq!(apply(BlendMode::Lighten, [50, 0, 0, 255], [200, 0, 0, 255])[0], 200);
        assert_eq!(apply(BlendMode::Difference, [200, 0, 0, 255], [50, 0, 0, 255])[0], 150);
        assert_eq!(apply(BlendMode::Addition, [200, 0, 0, 255], [100, 0, 0, 255])[0], 255);
        assert_eq!(apply(BlendMode::Subtract, [50, 0, 0, 255], [80, 0, 0, 255])[0], 0);
        let _ = apply(BlendMode::ColorDodge, [100, 0, 0, 255], [200, 0, 0, 255]);
        let _ = apply(BlendMode::ColorBurn, [200, 0, 0, 255], [100, 0, 0, 255]);
        let _ = apply(BlendMode::HardLight, [128, 128, 128, 255], [200, 200, 200, 255]);
        let _ = apply(BlendMode::SoftLight, [128, 128, 128, 255], [64, 64, 64, 255]);
        let _ = apply(BlendMode::Exclusion, [200, 100, 50, 255], [50, 100, 200, 255]);
        let _ = apply(BlendMode::Divide, [100, 0, 0, 255], [200, 0, 0, 255]);
        let _ = apply(BlendMode::Overlay, [128, 128, 128, 255], [128, 128, 128, 255]);
    }

    #[test]
    fn hsl_modes_run() {
        let back = [180, 40, 40, 255];
        let src = [40, 180, 40, 255];
        for mode in [
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ] {
            let out = apply(mode, back, src);
            assert_eq!(out[3], 255, "{mode:?}");
        }
    }
}
