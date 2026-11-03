use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::Rgba;

use crate::color_mode::ColorMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Arc<Vec<u8>>, // RGBA8 row-major
}

impl RgbaImage {
    pub fn new(width: u16, height: u16) -> Self {
        let n = (width as usize) * (height as usize) * 4;
        Self {
            width,
            height,
            pixels: Arc::new(vec![0; n]),
        }
    }

    pub fn from_rgba(width: u16, height: u16, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels: Arc::new(pixels),
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Rgba {
        let i = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let p = &self.pixels;
        Rgba {
            r: p[i],
            g: p[i + 1],
            b: p[i + 2],
            a: p[i + 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Arc<Vec<u8>>,
}

impl IndexedImage {
    pub fn filled(width: u16, height: u16, value: u8) -> Self {
        let n = (width as usize) * (height as usize);
        Self {
            width,
            height,
            pixels: Arc::new(vec![value; n]),
        }
    }

    pub fn from_indices(width: u16, height: u16, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels: Arc::new(pixels),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrayImage {
    pub width: u16,
    pub height: u16,
    /// VA pairs
    pub pixels: Arc<Vec<u8>>,
}

impl GrayImage {
    pub fn new(width: u16, height: u16) -> Self {
        let n = (width as usize) * (height as usize) * 2;
        Self {
            width,
            height,
            pixels: Arc::new(vec![0; n]),
        }
    }

    pub fn from_va(width: u16, height: u16, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels: Arc::new(pixels),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageBuffer {
    Rgba(RgbaImage),
    Grayscale(GrayImage),
    Indexed(IndexedImage),
}

impl ImageBuffer {
    pub fn color_mode(&self) -> ColorMode {
        match self {
            ImageBuffer::Rgba(_) => ColorMode::Rgba,
            ImageBuffer::Grayscale(_) => ColorMode::Grayscale,
            ImageBuffer::Indexed(_) => ColorMode::Indexed,
        }
    }

    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            ImageBuffer::Rgba(i) => (i.width, i.height),
            ImageBuffer::Grayscale(i) => (i.width, i.height),
            ImageBuffer::Indexed(i) => (i.width, i.height),
        }
    }
}
