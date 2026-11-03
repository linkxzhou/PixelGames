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


/// Set one palette entry. `frame=None` → document palette; `Some(f)` → frame override.
pub fn palette_set_entry(
    sprite: &mut Sprite,
    frame: Option<u16>,
    index: usize,
    color: Rgba,
) -> Result<()> {
    let pal = match frame {
        None => {
            if sprite.palette.len() <= index {
                sprite.palette.resize(index + 1, Rgba::TRANSPARENT);
            }
            &mut sprite.palette
        }
        Some(f) => {
            let p = sprite.ensure_frame_palette(f);
            if p.len() <= index {
                p.resize(index + 1, Rgba::TRANSPARENT);
            }
            sprite.frame_palettes.get_mut(&f).unwrap()
        }
    };
    if index >= 256 {
        return Err(AseErr("palette index must be < 256".into()));
    }
    pal[index] = color;
    let pal_len = pal.len();
    drop(pal);
    match frame {
        None => sprite.sync_palette_names_len(),
        Some(f) => {
            let names = sprite
                .frame_palette_names
                .entry(f)
                .or_insert_with(|| vec![None; pal_len]);
            if names.len() < pal_len {
                names.resize(pal_len, None);
            }
        }
    }
    Ok(())
}

pub fn palette_add_entry(
    sprite: &mut Sprite,
    frame: Option<u16>,
    color: Rgba,
) -> Result<usize> {
    let pal = match frame {
        None => &mut sprite.palette,
        Some(f) => {
            let _ = sprite.ensure_frame_palette(f);
            sprite.frame_palettes.get_mut(&f).unwrap()
        }
    };
    if pal.len() >= 256 {
        return Err(AseErr("palette already has 256 colors".into()));
    }
    pal.push(color);
    let new_index = pal.len() - 1;
    drop(pal);
    match frame {
        None => {
            sprite.palette_names.push(None);
            sprite.sync_palette_names_len();
        }
        Some(f) => {
            sprite
                .frame_palette_names
                .entry(f)
                .or_insert_with(Vec::new)
                .push(None);
        }
    }
    Ok(new_index)
}

pub fn palette_remove_entry(
    sprite: &mut Sprite,
    frame: Option<u16>,
    index: usize,
) -> Result<()> {
    match frame {
        None => {
            if index >= sprite.palette.len() {
                return Err(AseErr(format!("palette index {index} out of range")));
            }
            sprite.palette.remove(index);
            if index < sprite.palette_names.len() {
                sprite.palette_names.remove(index);
            }
            sprite.sync_palette_names_len();
        }
        Some(f) => {
            let Some(pal) = sprite.frame_palettes.get_mut(&f) else {
                return Err(AseErr(format!("no frame palette on frame {f}")));
            };
            if index >= pal.len() {
                return Err(AseErr(format!("frame palette index {index} out of range")));
            }
            pal.remove(index);
            if let Some(names) = sprite.frame_palette_names.get_mut(&f) {
                if index < names.len() {
                    names.remove(index);
                }
            }
            if pal.is_empty() {
                sprite.frame_palettes.remove(&f);
                sprite.frame_palette_names.remove(&f);
            }
        }
    }
    Ok(())
}

pub fn palette_clear_frame(sprite: &mut Sprite, frame: u16) -> Result<()> {
    if sprite.frame_palettes.remove(&frame).is_none() {
        return Err(AseErr(format!("no frame palette on frame {frame}")));
    }
    sprite.frame_palette_names.remove(&frame);
    Ok(())
}

#[cfg(test)]
mod frame_palette_tests {
    use super::*;
    use pxl_ase_doc::ColorMode;

    #[test]
    fn frame_palette_overrides_document() {
        let mut s = Sprite::new(1, 1, ColorMode::Indexed);
        s.palette = vec![Rgba::rgb(1, 2, 3), Rgba::rgb(4, 5, 6)];
        palette_set_entry(&mut s, Some(1), 0, Rgba::rgb(9, 9, 9)).unwrap();
        assert_eq!(s.palette_for_frame(0)[0], Rgba::rgb(1, 2, 3));
        assert_eq!(s.palette_for_frame(1)[0], Rgba::rgb(9, 9, 9));
    }
}
