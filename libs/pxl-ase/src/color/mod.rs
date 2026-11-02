//! Color-mode conversion, remap, quantize, palette files (merged `pxl-ase-color`).

use std::path::Path;
use std::sync::Arc;

use pxl_ase_doc::{ColorMode, ImageBuffer, IndexedImage, RgbaImage, Sprite};
use pxl_art::quantize::{map_to_palette, map_to_palette_ordered, median_cut, DitherMatrix};
use pxl_core::Rgba;

use crate::render::flatten_frame_rgba;
use crate::{sprite_palette, AseErr, Result};

/// Dithering algorithm names aligned with Aseprite CLI docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DitherAlgorithm {
    #[default]
    None,
    Ordered,
    /// Alias of ordered for CLI `--dithering-algorithm old` (same Bayer path).
    Old,
}

impl DitherAlgorithm {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" | "off" | "nearest" => Ok(Self::None),
            "ordered" | "bayer" => Ok(Self::Ordered),
            "old" => Ok(Self::Old),
            other => Err(AseErr(format!(
                "unknown dithering-algorithm '{other}' (none|ordered|old)"
            ))),
        }
    }

    pub fn strength(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Ordered | Self::Old => 100,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConvertColorOpts {
    pub algorithm: DitherAlgorithm,
    pub matrix: DitherMatrix,
    pub max_colors: usize,
}

impl Default for ConvertColorOpts {
    fn default() -> Self {
        Self {
            algorithm: DitherAlgorithm::None,
            matrix: DitherMatrix::Bayer8,
            max_colors: 32,
        }
    }
}

pub fn quantize_rgba(rgba_stream: &[u8], max_colors: usize) -> Vec<Rgba> {
    median_cut(rgba_stream, max_colors.max(1), 4)
}

/// Load a GIMP `.gpl` or Adobe `.act` palette file (clean-room).
pub fn load_palette_file(path: &Path) -> Result<Vec<Rgba>> {
    let bytes = std::fs::read(path).map_err(|e| AseErr(format!("palette read: {e}")))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "gpl" => parse_gpl(&String::from_utf8_lossy(&bytes)),
        "act" => parse_act(&bytes),
        _ => {
            // Try GPL text first, then ACT binary.
            if bytes.starts_with(b"GIMP Palette") || bytes.starts_with(b"JASC-PAL") {
                parse_gpl(&String::from_utf8_lossy(&bytes))
            } else if bytes.len() == 768 || bytes.len() == 772 {
                parse_act(&bytes)
            } else {
                Err(AseErr(format!(
                    "unsupported palette '{}'; use .gpl or .act",
                    path.display()
                )))
            }
        }
    }
}

pub fn parse_gpl(text: &str) -> Result<Vec<Rgba>> {
    let mut colors = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("GIMP Palette")
            || line.starts_with("Name:")
            || line.starts_with("Columns:")
            || line.starts_with("JASC-PAL")
        {
            continue;
        }
        // Optional leading index for JASC: "0 255 0 0"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let (r, g, b) = if parts.len() >= 4 && parts[0].parse::<u16>().is_ok() && parts[1].parse::<u16>().is_ok() {
            // Could be "R G B name" or "idx R G B"
            if parts[0].parse::<u16>().unwrap_or(999) <= 255
                && parts.get(3).and_then(|s| s.parse::<u16>().ok()).is_some()
                && parts[1].parse::<u16>().unwrap_or(999) <= 255
            {
                // idx R G B
                (
                    parts[1].parse::<u8>().unwrap_or(0),
                    parts[2].parse::<u8>().unwrap_or(0),
                    parts[3].parse::<u8>().unwrap_or(0),
                )
            } else {
                (
                    parts[0].parse::<u8>().unwrap_or(0),
                    parts[1].parse::<u8>().unwrap_or(0),
                    parts[2].parse::<u8>().unwrap_or(0),
                )
            }
        } else {
            (
                parts[0].parse::<u8>().unwrap_or(0),
                parts[1].parse::<u8>().unwrap_or(0),
                parts[2].parse::<u8>().unwrap_or(0),
            )
        };
        colors.push(Rgba::rgb(r, g, b));
        if colors.len() >= 256 {
            break;
        }
    }
    if colors.is_empty() {
        return Err(AseErr("GPL palette has no colors".into()));
    }
    // Ensure index 0 can be transparent for indexed workflows when first is opaque.
    Ok(colors)
}

pub fn parse_act(bytes: &[u8]) -> Result<Vec<Rgba>> {
    if bytes.len() < 768 {
        return Err(AseErr("ACT palette too short (need 768 bytes)".into()));
    }
    let mut n = 256usize;
    if bytes.len() >= 772 {
        n = u16::from_be_bytes([bytes[768], bytes[769]]) as usize;
        if n == 0 || n > 256 {
            n = 256;
        }
    }
    let mut colors = Vec::with_capacity(n);
    for i in 0..n {
        let o = i * 3;
        colors.push(Rgba::rgb(bytes[o], bytes[o + 1], bytes[o + 2]));
    }
    Ok(colors)
}

/// Replace sprite palette (does not remap pixels). Pads/truncates to ≤256.
pub fn apply_palette(sprite: &mut Sprite, colors: Vec<Rgba>) -> Result<()> {
    if colors.is_empty() {
        return Err(AseErr("palette is empty".into()));
    }
    sprite.palette = colors.into_iter().take(256).collect();
    Ok(())
}

pub fn convert_sprite_color_mode(sprite: &mut Sprite, target: ColorMode) -> Result<()> {
    convert_sprite_color_mode_opts(sprite, target, ConvertColorOpts::default())
}

pub fn convert_sprite_color_mode_opts(
    sprite: &mut Sprite,
    target: ColorMode,
    opts: ConvertColorOpts,
) -> Result<()> {
    if sprite.color_mode == target && opts.algorithm == DitherAlgorithm::None {
        return Ok(());
    }
    let frames = sprite.frame_count();
    let mut flat = Vec::with_capacity(frames);
    for f in 0..frames {
        flat.push(flatten_frame_rgba(sprite, f as u16)?);
    }
    match target {
        ColorMode::Rgba => {
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let img = RgbaImage::from_rgba(sprite.width, sprite.height, rgba);
                let id = sprite.insert_image(ImageBuffer::Rgba(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Rgba;
        }
        ColorMode::Indexed => {
            let colors = if sprite.palette.len() > 1 {
                sprite.palette.clone()
            } else {
                let mut samples = Vec::new();
                for rgba in &flat {
                    samples.extend_from_slice(rgba);
                }
                quantize_rgba(&samples, opts.max_colors)
            };
            sprite.palette = colors;
            let pal = sprite_palette(sprite);
            let dither = opts.algorithm.strength();
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let indices = if dither == 0 {
                    map_to_palette(
                        sprite.width as usize,
                        sprite.height as usize,
                        &rgba,
                        &pal,
                        0,
                    )
                } else {
                    map_to_palette_ordered(
                        sprite.width as usize,
                        sprite.height as usize,
                        &rgba,
                        &pal,
                        dither,
                        opts.matrix,
                    )
                };
                let img = IndexedImage::from_indices(sprite.width, sprite.height, indices);
                let id = sprite.insert_image(ImageBuffer::Indexed(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Indexed;
        }
        ColorMode::Grayscale => {
            sprite.layers.clear();
            sprite.images.clear();
            let layer = sprite.add_image_layer("Flattened");
            for (fi, rgba) in flat.into_iter().enumerate() {
                let mut va = Vec::with_capacity(rgba.len() / 2);
                for chunk in rgba.chunks_exact(4) {
                    let y = ((chunk[0] as u16 * 77 + chunk[1] as u16 * 150 + chunk[2] as u16 * 29)
                        / 256) as u8;
                    va.push(y);
                    va.push(chunk[3]);
                }
                let img = pxl_ase_doc::GrayImage::from_va(sprite.width, sprite.height, va);
                let id = sprite.insert_image(ImageBuffer::Grayscale(img));
                sprite.set_cel(layer, fi as u16, pxl_ase_doc::Cel::image(id, 0, 0, 255));
            }
            sprite.color_mode = ColorMode::Grayscale;
        }
    }
    Ok(())
}

pub fn remap_palette_indices(sprite: &mut Sprite, map: &[u8]) -> Result<()> {
    if sprite.color_mode != ColorMode::Indexed {
        return Err(AseErr("remap requires indexed color mode".into()));
    }
    for img in sprite.images.values_mut() {
        if let ImageBuffer::Indexed(ix) = img {
            let mut px = ix.pixels.as_ref().clone();
            for p in &mut px {
                *p = map.get(*p as usize).copied().unwrap_or(0);
            }
            ix.pixels = Arc::new(px);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, Sprite};

    fn mini() -> Sprite {
        let mut s = Sprite::new(2, 2, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(
            2,
            2,
            vec![
                200, 10, 10, 255, 10, 10, 200, 255, 10, 200, 10, 255, 200, 200, 10, 255,
            ],
        );
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s
    }

    #[test]
    fn gpl_and_act_roundtrip_load() {
        let gpl = "GIMP Palette\nName: t\n#\n255 0 0 Red\n0 255 0 Green\n0 0 255 Blue\n";
        let colors = parse_gpl(gpl).unwrap();
        assert_eq!(colors.len(), 3);
        assert_eq!(colors[0], Rgba::rgb(255, 0, 0));
        let mut act = Vec::new();
        for c in &colors {
            act.extend_from_slice(&[c.r, c.g, c.b]);
        }
        while act.len() < 768 {
            act.extend_from_slice(&[0, 0, 0]);
        }
        act.extend_from_slice(&[0, 3, 0, 0]); // count=3
        let act_colors = parse_act(&act).unwrap();
        assert_eq!(&act_colors[..3], &colors[..]);
    }

    #[test]
    fn indexed_convert_with_ordered_dither() {
        let mut s = mini();
        convert_sprite_color_mode_opts(
            &mut s,
            ColorMode::Indexed,
            ConvertColorOpts {
                algorithm: DitherAlgorithm::Ordered,
                matrix: DitherMatrix::Bayer8,
                max_colors: 4,
            },
        )
        .unwrap();
        assert_eq!(s.color_mode, ColorMode::Indexed);
        assert!(!s.palette.is_empty());
    }

    #[test]
    fn remap_indices() {
        let mut s = Sprite::new(2, 1, ColorMode::Indexed);
        s.palette = vec![Rgba::TRANSPARENT, Rgba::rgb(1, 2, 3), Rgba::rgb(4, 5, 6)];
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = IndexedImage::from_indices(2, 1, vec![1, 2]);
        let id = s.insert_image(ImageBuffer::Indexed(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        remap_palette_indices(&mut s, &[0, 2, 1]).unwrap();
        if let ImageBuffer::Indexed(ix) = s.images.values().next().unwrap() {
            assert_eq!(ix.pixels.as_ref(), &[2, 1]);
        } else {
            panic!("expected indexed");
        }
    }
}
