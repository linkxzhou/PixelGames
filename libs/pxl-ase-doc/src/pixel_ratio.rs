//! Sprite pixel aspect ratio (`doc::PixelRatio` ≅ width/height pair).

/// Pixel aspect: `width/height`. Zero in either field means 1:1 (ase-file-specs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRatio {
    pub width: u8,
    pub height: u8,
}

impl Default for PixelRatio {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
        }
    }
}

impl PixelRatio {
    pub fn new(width: u8, height: u8) -> Self {
        Self { width, height }
    }

    /// Effective ratio after treating 0 as 1.
    pub fn effective(self) -> (u8, u8) {
        (
            if self.width == 0 { 1 } else { self.width },
            if self.height == 0 { 1 } else { self.height },
        )
    }
}
