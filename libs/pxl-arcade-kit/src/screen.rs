//! Integer-scale viewport maths.
//!
//! The logical resolution never changes; the window picks the largest whole
//! multiple that fits and centres the result, so a pixel is always a square
//! block of screen pixels.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub logical_w: u32,
    pub logical_h: u32,
    pub scale: u32,
    /// Screen-space offset of the logical origin, in physical pixels.
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Viewport {
    pub fn fit(logical_w: u32, logical_h: u32, window_w: u32, window_h: u32) -> Viewport {
        let scale = (window_w / logical_w.max(1))
            .min(window_h / logical_h.max(1))
            .max(1);
        let width = logical_w * scale;
        let height = logical_h * scale;
        Viewport {
            logical_w,
            logical_h,
            scale,
            offset_x: ((window_w as i32) - width as i32) / 2,
            offset_y: ((window_h as i32) - height as i32) / 2,
        }
    }

    /// World rect the camera must cover so the image is centred with a whole
    /// pixel per screen pixel.
    pub fn camera_rect(&self, window_w: u32, window_h: u32) -> (f32, f32, f32, f32) {
        let w = window_w as f32 / self.scale as f32;
        let h = window_h as f32 / self.scale as f32;
        let x = -(w - self.logical_w as f32) / 2.0;
        let y = -(h - self.logical_h as f32) / 2.0;
        (x, y, w, h)
    }

    pub fn to_screen(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.offset_x as f32 + x * self.scale as f32,
            self.offset_y as f32 + y * self.scale as f32,
        )
    }

    pub fn to_logical(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.offset_x as f32) / self.scale as f32,
            (y - self.offset_y as f32) / self.scale as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_scale_and_centring() {
        let view = Viewport::fit(224, 288, 800, 600);
        assert_eq!(view.scale, 2);
        assert_eq!(view.offset_x, (800 - 448) / 2);
        assert_eq!(view.offset_y, (600 - 576) / 2);
        let (x, y, w, h) = view.camera_rect(800, 600);
        assert_eq!((x, y), (-88.0, -6.0));
        assert_eq!((w, h), (400.0, 300.0));
    }

    #[test]
    fn tiny_window_never_scales_below_one() {
        let view = Viewport::fit(320, 240, 200, 120);
        assert_eq!(view.scale, 1);
        assert_eq!(view.offset_x, -60);
    }
}
