use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::blend::BlendMode;
use crate::image::{GrayImage, IndexedImage, RgbaImage};
use crate::types::{FrameIndex, ImageId, LayerId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Image,
    Group,
    Tilemap,
}

impl LayerKind {
    pub fn from_u16(v: u16) -> Self {
        match v {
            1 => LayerKind::Group,
            2 => LayerKind::Tilemap,
            _ => LayerKind::Image,
        }
    }

    pub fn to_u16(self) -> u16 {
        match self {
            LayerKind::Image => 0,
            LayerKind::Group => 1,
            LayerKind::Tilemap => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LayerFlags {
    pub visible: bool,
    pub editable: bool,
    pub background: bool,
}

impl LayerFlags {
    pub fn from_bits(bits: u16) -> Self {
        Self {
            visible: bits & 1 != 0,
            editable: bits & 2 != 0,
            background: bits & 8 != 0,
        }
    }

    pub fn to_bits(self) -> u16 {
        let mut b = 0u16;
        if self.visible {
            b |= 1;
        }
        if self.editable {
            b |= 2;
        }
        if self.background {
            b |= 8;
        }
        b
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cel {
    Image {
        image: ImageId,
        x: i16,
        y: i16,
        opacity: u8,
        z_index: i16,
    },
    Linked {
        frame: FrameIndex,
    },
}

impl Cel {
    pub fn image(image: ImageId, x: i16, y: i16, opacity: u8) -> Self {
        Cel::Image {
            image,
            x,
            y,
            opacity,
            z_index: 0,
        }
    }

    pub fn linked(frame: FrameIndex) -> Self {
        Cel::Linked { frame }
    }
}

#[derive(Clone, Debug)]
pub enum CelPixels {
    Rgba {
        image: RgbaImage,
        x: i16,
        y: i16,
        opacity: u8,
    },
    Indexed {
        image: IndexedImage,
        x: i16,
        y: i16,
        opacity: u8,
    },
    Gray {
        image: GrayImage,
        x: i16,
        y: i16,
        opacity: u8,
    },
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    pub flags: LayerFlags,
    pub child_level: u16,
    pub blend: BlendMode,
    pub opacity: u8,
    pub cels: BTreeMap<FrameIndex, Cel>,
    pub tileset_index: Option<u32>,
}

impl Layer {
    pub fn image(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            kind: LayerKind::Image,
            flags: LayerFlags {
                visible: true,
                editable: true,
                background: false,
            },
            child_level: 0,
            blend: BlendMode::Normal,
            opacity: 255,
            cels: BTreeMap::new(),
            tileset_index: None,
        }
    }
}
