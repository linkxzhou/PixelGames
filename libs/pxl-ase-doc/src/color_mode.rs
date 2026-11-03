#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ColorMode {
    Rgba,
    Grayscale,
    Indexed,
}

impl ColorMode {
    pub fn bits_per_pixel(self) -> u16 {
        match self {
            ColorMode::Rgba => 32,
            ColorMode::Grayscale => 16,
            ColorMode::Indexed => 8,
        }
    }

    pub fn from_bpp(bpp: u16) -> Option<Self> {
        match bpp {
            32 => Some(ColorMode::Rgba),
            16 => Some(ColorMode::Grayscale),
            8 => Some(ColorMode::Indexed),
            _ => None,
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            ColorMode::Rgba => 4,
            ColorMode::Grayscale => 2,
            ColorMode::Indexed => 1,
        }
    }
}
