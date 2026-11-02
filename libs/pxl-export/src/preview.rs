use pxl_core::{IndexFrame, Palette, PxlResult, Rgba};

use crate::png::encode_rgba_png;
use crate::raster::{composite_checked, scaled_dims};

/// Preview composite. Transparent pixels become a flat dark backdrop so the
/// PNG is readable, and `--grid` draws a 1px lattice on the scaled cells.
///
/// This is a viewing aid, not an asset: use [`render_asset_png`] for output that
/// keeps transparency.
pub fn render_preview_png(
    frame: &IndexFrame,
    palette: &Palette,
    scale: u32,
    grid: bool,
) -> PxlResult<Vec<u8>> {
    let (w, h, rgba) = composite_checked(frame, palette, scale.max(1), grid)?;
    encode_rgba_png(w, h, &rgba)
}

/// Transparent asset PNG: every source pixel is copied `scale x scale` times
/// (nearest neighbour), transparent indices stay fully transparent, and no
/// backdrop or grid is drawn. `scale` changes the output size only, never the
/// content.
pub fn render_asset_png(frame: &IndexFrame, palette: &Palette, scale: u32) -> PxlResult<Vec<u8>> {
    let scale = scale.max(1);
    let (w, h) = scaled_dims(frame.w as u32, frame.h as u32, scale)?;
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..frame.h as u32 {
        for x in 0..frame.w as u32 {
            let col = palette
                .colors
                .get(frame.get(x as i32, y as i32) as usize)
                .copied()
                .unwrap_or(Rgba::TRANSPARENT);
            let px = if col.a == 0 {
                [0, 0, 0, 0]
            } else {
                [col.r, col.g, col.b, 255]
            };
            for dy in 0..scale {
                let row = (y * scale + dy) as usize * w as usize;
                for dx in 0..scale {
                    let i = (row + (x * scale + dx) as usize) * 4;
                    rgba[i..i + 4].copy_from_slice(&px);
                }
            }
        }
    }
    encode_rgba_png(w, h, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::decode_png_rgba;
    use crate::tests::support::palette;

    fn frame() -> IndexFrame {
        let mut f = IndexFrame::new(2, 2).unwrap();
        f.set(0, 0, 2);
        f.set(1, 1, 3);
        f
    }

    #[test]
    fn asset_png_keeps_transparency_and_has_no_grid() {
        let pal = palette();
        let (w, h, rgba) = decode_png_rgba(&render_asset_png(&frame(), &pal, 4).unwrap()).unwrap();
        assert_eq!((w, h), (8, 8));
        let px = |x: usize, y: usize| {
            let i = (y * w as usize + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_eq!(px(7, 0)[3], 0, "transparent source stays transparent");
        assert_eq!(px(0, 0), [59, 74, 42, 255]);
        assert_eq!(px(3, 3), px(0, 0), "block edge is not a grid line");
    }

    #[test]
    fn preview_png_paints_a_backdrop_where_the_asset_is_transparent() {
        let pal = palette();
        let (_, _, rgba) =
            decode_png_rgba(&render_preview_png(&frame(), &pal, 2, false).unwrap()).unwrap();
        assert_eq!(
            &rgba[(0 * 4 + 6) * 4..(0 * 4 + 6) * 4 + 4],
            &[28, 26, 24, 255]
        );
    }

    #[test]
    fn oversized_scale_is_an_error_not_a_panic() {
        let pal = palette();
        assert!(render_asset_png(&frame(), &pal, u32::MAX).is_err());
        assert!(render_preview_png(&frame(), &pal, u32::MAX, false).is_err());
    }
}
