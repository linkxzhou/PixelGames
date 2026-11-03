//! GD-8 palette sort.

use pxl_ase_doc::Sprite;
use pxl_core::Rgba;

use crate::{AseErr, Result};

fn rgb_to_hsl(c: Rgba) -> (f32, f32, f32) {
    let r = c.r as f32 / 255.0;
    let g = c.g as f32 / 255.0;
    let b = c.b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
}

/// Sort sprite.palette by hue | sat | lum. Index 0 (often transparent) stays put when `keep_zero`.
pub fn palette_sort(sprite: &mut Sprite, by: &str, keep_zero: bool) -> Result<()> {
    if sprite.palette.is_empty() {
        return Ok(());
    }
    let key = match by {
        "hue" | "h" => 0,
        "sat" | "saturation" | "s" => 1,
        "lum" | "luminance" | "lightness" | "l" => 2,
        other => {
            return Err(AseErr(format!(
                "unknown palette sort '{other}' (hue|sat|lum)"
            )));
        }
    };
    let start = if keep_zero && !sprite.palette.is_empty() { 1 } else { 0 };
    let mut rest: Vec<Rgba> = sprite.palette[start..].to_vec();
    rest.sort_by(|a, b| {
        let (ha, sa, la) = rgb_to_hsl(*a);
        let (hb, sb, lb) = rgb_to_hsl(*b);
        let ka = [ha, sa, la][key];
        let kb = [hb, sb, lb][key];
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    if start == 1 {
        sprite.palette.splice(1.., rest);
    } else {
        sprite.palette = rest;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::ColorMode;

    #[test]
    fn sort_by_lum_keeps_zero() {
        let mut s = Sprite::new(1, 1, ColorMode::Indexed);
        s.palette = vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(200, 200, 200),
            Rgba::rgb(10, 10, 10),
            Rgba::rgb(100, 100, 100),
        ];
        palette_sort(&mut s, "lum", true).unwrap();
        assert_eq!(s.palette[0].a, 0);
        assert!(s.palette[1].r < s.palette[2].r && s.palette[2].r < s.palette[3].r);
    }
}
