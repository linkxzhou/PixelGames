//! Headless filters (merged `pxl-ase-filters`).

mod color_adjust;
mod convolve;
mod curve;
mod median;
mod opts;
mod outline_fill;

pub use color_adjust::{
    brightness_contrast, hue_saturation, invert, replace_color_rgba, replace_color_rgba_tol,
    replace_index,
};
pub use convolve::{convolve_sprite, convolve_sprite_opts, Kernel};
pub use curve::{apply_curve_lut, apply_curve_lut_opts, contrast_curve_lut, lut_from_points};
pub use median::{median_sprite, median_sprite_opts, MAX_MEDIAN_RADIUS};
pub use opts::{BoundaryMode, ChannelMask, FilterOpts};
pub use outline_fill::{flood_fill_frame, outline_all_frames, outline_flatten};

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, RgbaImage, Sprite};
    use pxl_core::Rgba;

    use crate::render::flatten_frame_rgba;

    fn rgba_sprite() -> Sprite {
        let mut s = Sprite::new(2, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(2, 1, vec![10, 20, 30, 255, 200, 210, 220, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        s
    }

    #[test]
    fn invert_and_replace_tol() {
        let mut s = rgba_sprite();
        invert(&mut s).unwrap();
        replace_color_rgba_tol(
            &mut s,
            Rgba {
                r: 245,
                g: 235,
                b: 225,
                a: 255,
            },
            Rgba::rgb(1, 2, 3),
            5,
        )
        .unwrap();
        brightness_contrast(&mut s, 10, 0).unwrap();
    }

    #[test]
    fn hue_sat_shifts_red_toward_green() {
        let mut s = Sprite::new(1, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(1, 1, vec![255, 0, 0, 255]);
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        hue_saturation(&mut s, 120, 0, 0).unwrap();
        let flat = flatten_frame_rgba(&s, 0).unwrap();
        // ~green
        assert!(flat[1] > flat[0] && flat[1] > flat[2], "{flat:?}");
    }

    #[test]
    fn flood_fill_replaces_connected() {
        let mut s = Sprite::new(3, 1, ColorMode::Rgba);
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = RgbaImage::from_rgba(
            3,
            1,
            vec![10, 10, 10, 255, 10, 10, 10, 255, 200, 200, 200, 255],
        );
        let id = s.insert_image(ImageBuffer::Rgba(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        let n = flood_fill_frame(&mut s, 0, 0, 0, Rgba::rgb(1, 2, 3), 0).unwrap();
        assert_eq!(n, 2);
        let flat = flatten_frame_rgba(&s, 0).unwrap();
        assert_eq!(&flat[0..4], &[1, 2, 3, 255]);
        assert_eq!(&flat[4..8], &[1, 2, 3, 255]);
        assert_eq!(&flat[8..12], &[200, 200, 200, 255]);
    }
}
