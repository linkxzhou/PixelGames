use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::blend::BlendMode;
use crate::image::{GrayImage, IndexedImage, RgbaImage};
use crate::types::{FrameIndex, ImageId, LayerId};
use crate::cel_extra::CelExtra;
use crate::user_data::UserData;

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
    /// Bit 4 — lock movement (ase-file-specs).
    pub lock_move: bool,
    pub background: bool,
    /// Bit 16 — prefer linked cels.
    pub prefer_linked: bool,
    /// Bit 32 — group collapsed in UI (stored for roundtrip).
    pub collapsed: bool,
    /// Bit 64 — reference layer.
    pub reference: bool,
}

impl LayerFlags {
    pub fn from_bits(bits: u16) -> Self {
        Self {
            visible: bits & 1 != 0,
            editable: bits & 2 != 0,
            lock_move: bits & 4 != 0,
            background: bits & 8 != 0,
            prefer_linked: bits & 16 != 0,
            collapsed: bits & 32 != 0,
            reference: bits & 64 != 0,
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
        if self.lock_move {
            b |= 4;
        }
        if self.background {
            b |= 8;
        }
        if self.prefer_linked {
            b |= 16;
        }
        if self.collapsed {
            b |= 32;
        }
        if self.reference {
            b |= 64;
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
        user_data: UserData,
        extra: Option<CelExtra>,
    },
    Linked {
        frame: FrameIndex,
        z_index: i16,
        user_data: UserData,
        extra: Option<CelExtra>,
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
            user_data: UserData::default(),
            extra: None,
        }
    }

    pub fn linked(frame: FrameIndex) -> Self {
        Cel::Linked {
            frame,
            z_index: 0,
            user_data: UserData::default(),
            extra: None,
        }
    }

    pub fn z_index(&self) -> i16 {
        match self {
            Cel::Image { z_index, .. } | Cel::Linked { z_index, .. } => *z_index,
        }
    }

    pub fn user_data(&self) -> &UserData {
        match self {
            Cel::Image { user_data, .. } | Cel::Linked { user_data, .. } => user_data,
        }
    }

    pub fn user_data_mut(&mut self) -> &mut UserData {
        match self {
            Cel::Image { user_data, .. } | Cel::Linked { user_data, .. } => user_data,
        }
    }

    pub fn extra(&self) -> Option<&CelExtra> {
        match self {
            Cel::Image { extra, .. } | Cel::Linked { extra, .. } => extra.as_ref(),
        }
    }

    pub fn set_extra(&mut self, extra: CelExtra) {
        match self {
            Cel::Image { extra: slot, .. } | Cel::Linked { extra: slot, .. } => {
                *slot = Some(extra);
            }
        }
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
    pub uuid: Option<[u8; 16]>,
    pub user_data: UserData,
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
                lock_move: false,
                background: false,
                prefer_linked: false,
                collapsed: false,
                reference: false,
            },
            child_level: 0,
            blend: BlendMode::Normal,
            opacity: 255,
            cels: BTreeMap::new(),
            tileset_index: None,
            uuid: None,
            user_data: UserData::default(),
        }
    }
}
