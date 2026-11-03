use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use pxl_core::Rgba;

use crate::color_mode::ColorMode;
use crate::error::{AseError, AseResult};
use crate::image::ImageBuffer;
use crate::layer::{Cel, CelPixels, Layer, LayerKind};
use crate::types::{FrameIndex, ImageId, LayerId};
use crate::user_data::UserData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagDirection {
    Forward = 0,
    Reverse = 1,
    PingPong = 2,
    PingPongReverse = 3,
}

impl TagDirection {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TagDirection::Reverse,
            2 => TagDirection::PingPong,
            3 => TagDirection::PingPongReverse,
            _ => TagDirection::Forward,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub from: FrameIndex,
    pub to: FrameIndex,
    pub direction: TagDirection,
    pub repeat: u16,
    pub user_data: UserData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceKey {
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub center: Option<(i32, i32, u32, u32)>,
    pub pivot: Option<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slice {
    pub name: String,
    pub keys: Vec<SliceKey>,
    pub user_data: UserData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Grid {
    pub x: i16,
    pub y: i16,
    pub w: u16,
    pub h: u16,
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub width: u16,
    pub height: u16,
    pub color_mode: ColorMode,
    pub transparent_index: u8,
    pub frame_durations_ms: Vec<u16>,
    pub layers: Vec<Layer>,
    pub images: BTreeMap<ImageId, ImageBuffer>,
    pub palette: Vec<Rgba>,
    pub tags: Vec<Tag>,
    pub slices: Vec<Slice>,
    pub grid: Grid,
    pub user_data: UserData,
    /// Notes collected while reading (skipped chunks / dropped fields). Cleared by callers if undesired.
    pub read_loss_notes: Vec<String>,
    next_image_id: u32,
}

impl Sprite {
    pub fn new(width: u16, height: u16, color_mode: ColorMode) -> Self {
        Self {
            width,
            height,
            color_mode,
            transparent_index: 0,
            frame_durations_ms: vec![100],
            layers: Vec::new(),
            images: BTreeMap::new(),
            palette: Vec::new(),
            tags: Vec::new(),
            slices: Vec::new(),
            grid: Grid::default(),
            user_data: UserData::default(),
            read_loss_notes: Vec::new(),
            next_image_id: 1,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frame_durations_ms.len()
    }

    pub fn ensure_frames(&mut self, n: usize) {
        while self.frame_durations_ms.len() < n {
            self.frame_durations_ms.push(100);
        }
    }

    pub fn add_image_layer(&mut self, name: impl Into<String>) -> LayerId {
        let id = LayerId(self.layers.len() as u16);
        self.layers.push(Layer::image(id, name));
        id
    }

    pub fn insert_image(&mut self, image: ImageBuffer) -> ImageId {
        let id = ImageId(self.next_image_id);
        self.next_image_id += 1;
        self.images.insert(id, image);
        id
    }

    pub fn set_cel(&mut self, layer: LayerId, frame: FrameIndex, cel: Cel) {
        self.ensure_frames(frame as usize + 1);
        if let Some(l) = self.layers.iter_mut().find(|l| l.id == layer) {
            l.cels.insert(frame, cel);
        }
    }

    pub fn cel_pixels(&self, layer: LayerId, frame: FrameIndex) -> AseResult<CelPixels> {
        let layer_ref = self
            .layers
            .iter()
            .find(|l| l.id == layer)
            .ok_or_else(|| AseError::new(alloc::format!("unknown layer {}", layer.0)))?;
        self.resolve_cel(layer_ref, frame, 0)
    }

    fn resolve_cel(&self, layer: &Layer, frame: FrameIndex, depth: u8) -> AseResult<CelPixels> {
        if depth > 32 {
            return Err(AseError::new("cel link cycle"));
        }
        let cel = layer
            .cels
            .get(&frame)
            .ok_or_else(|| AseError::new(alloc::format!("no cel on frame {frame}")))?;
        match cel {
            Cel::Linked { frame: src, .. } => self.resolve_cel(layer, *src, depth + 1),
            Cel::Image {
                image,
                x,
                y,
                opacity,
                ..
            } => {
                let buf = self
                    .images
                    .get(image)
                    .ok_or_else(|| AseError::new(alloc::format!("missing image {}", image.0)))?;
                Ok(match buf {
                    ImageBuffer::Rgba(img) => CelPixels::Rgba {
                        image: img.clone(),
                        x: *x,
                        y: *y,
                        opacity: *opacity,
                    },
                    ImageBuffer::Indexed(img) => CelPixels::Indexed {
                        image: img.clone(),
                        x: *x,
                        y: *y,
                        opacity: *opacity,
                    },
                    ImageBuffer::Grayscale(img) => CelPixels::Gray {
                        image: img.clone(),
                        x: *x,
                        y: *y,
                        opacity: *opacity,
                    },
                })
            }
        }
    }

    pub fn visible_image_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers
            .iter()
            .filter(|l| l.kind == LayerKind::Image && l.flags.visible)
    }
}

/// Top-level document container (one sprite for now).
#[derive(Clone, Debug)]
pub struct AseDoc {
    pub sprite: Sprite,
    pub path: Option<String>,
}

impl AseDoc {
    pub fn new(sprite: Sprite) -> Self {
        Self {
            sprite,
            path: None,
        }
    }
}
