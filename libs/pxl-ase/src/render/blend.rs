//! Clean-room RGBA blend modes (ase-file-specs / compositing-1 channel formulas).
//!
//! Channel blends produce RGB; result is alpha-composited with source-over.

use pxl_ase_doc::BlendMode;

/// Blend `src` over backdrop at `dst[di..]` using `mode`. `sa` already includes cel/layer opacity.
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
    let (cr, cg, cb) = match mode {
        BlendMode::Normal => (sr, sg, sb),
        BlendMode::Multiply => (mul_u8(br, sr), mul_u8(bg, sg), mul_u8(bb, sb)),
        BlendMode::Screen => (screen_u8(br, sr), screen_u8(bg, sg), screen_u8(bb, sb)),
        BlendMode::Overlay => (overlay_u8(br, sr), overlay_u8(bg, sg), overlay_u8(bb, sb)),
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
        _ => (sr, sg, sb),
    };
    composite_normal(dst, di, cr, cg, cb, sa, br, bg, bb, ba);
}

fn mul_u8(b: u8, s: u8) -> u8 {
    let t = b as u32 * s as u32;
    ((t + (t >> 8)) >> 8) as u8
}

fn screen_u8(b: u8, s: u8) -> u8 {
    let m = mul_u8(b, s);
    (b as u16 + s as u16 - m as u16).min(255) as u8
}

fn overlay_u8(b: u8, s: u8) -> u8 {
    // overlay(b,s) ≡ hard_light(s,b)
    if s < 128 {
        mul_u8(b, s.saturating_mul(2))
    } else {
        screen_u8(b, ((s as u16) * 2 - 255) as u8)
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
    fn multiply_darkens() {
        let out = apply(BlendMode::Multiply, [200, 200, 200, 255], [128, 128, 128, 255]);
        assert!(out[0] < 200, "{out:?}");
    }

    #[test]
    fn screen_lightens() {
        let out = apply(BlendMode::Screen, [100, 100, 100, 255], [100, 100, 100, 255]);
        assert!(out[0] > 100, "{out:?}");
    }

    #[test]
    fn overlay_mid_near() {
        let out = apply(BlendMode::Overlay, [128, 128, 128, 255], [128, 128, 128, 255]);
        assert!((out[0] as i16 - 128).abs() < 40, "{out:?}");
    }

    #[test]
    fn addition_clamps() {
        let out = apply(BlendMode::Addition, [200, 0, 0, 255], [100, 0, 0, 255]);
        assert_eq!(out[0], 255);
    }

    #[test]
    fn subtract_floors() {
        let out = apply(BlendMode::Subtract, [50, 0, 0, 255], [80, 0, 0, 255]);
        assert_eq!(out[0], 0);
    }
}
