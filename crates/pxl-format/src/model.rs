use std::collections::BTreeMap;

use pxl_core::{colors_from_hex, Cycle, IndexFrame, Palette, PxlError, PxlResult, Special};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn v1() -> u32 {
    1
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CycleFile {
    pub indices: Vec<u8>,
    #[serde(default)]
    pub period: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpecialFile {
    #[serde(default)]
    pub transparent: u8,
    #[serde(default = "default_outline")]
    pub outline: u8,
    #[serde(default = "default_flash")]
    pub flash: u8,
}

fn default_outline() -> u8 {
    1
}
fn default_flash() -> u8 {
    15
}

impl Default for SpecialFile {
    fn default() -> Self {
        Self {
            transparent: 0,
            outline: 1,
            flash: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaletteFile {
    #[serde(default = "v1")]
    pub v: u32,
    #[serde(default)]
    pub id: String,
    pub colors: Vec<String>,
    #[serde(default)]
    pub ramps: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    pub cycles: BTreeMap<String, CycleFile>,
    #[serde(default)]
    pub special: SpecialFile,
}

impl PaletteFile {
    pub fn into_palette(self, fallback_id: &str) -> PxlResult<Palette> {
        let colors = colors_from_hex(&self.colors)?;
        let mut cycles = BTreeMap::new();
        for (name, c) in self.cycles {
            let period = if c.period == 0 {
                c.indices.len() as u32
            } else {
                c.period
            };
            cycles.insert(
                name,
                Cycle {
                    indices: c.indices,
                    period,
                },
            );
        }
        let id = if self.id.is_empty() {
            fallback_id.to_string()
        } else {
            self.id
        };
        let palette = Palette {
            id,
            colors,
            ramps: self.ramps,
            cycles,
            special: Special {
                transparent: self.special.transparent,
                outline: self.special.outline,
                flash: self.special.flash,
            },
        };
        palette.validate()?;
        Ok(palette)
    }
}

pub fn parse_palette(text: &str, fallback_id: &str) -> PxlResult<Palette> {
    let value: serde_json::Value = crate::parse_json(text, "palette")?;
    let file: PaletteFile = if value.get("colors").is_some() {
        serde_json::from_value(value).map_err(|e| {
            PxlError::schema(format!("palette: {e}")).with_hint("see `pxl schema palette`")
        })?
    } else if let Some(inner) = value.get("palette").cloned() {
        serde_json::from_value(inner).map_err(|e| {
            PxlError::schema(format!("palette: {e}")).with_hint("see `pxl schema palette`")
        })?
    } else {
        return Err(PxlError::schema("palette JSON needs a colors array")
            .with_hint("top-level colors, or a \"palette\" object that contains colors"));
    };
    file.into_palette(fallback_id)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pxl_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Ramp that tile seam checks compare against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_ramp: Option<String>,
    /// Parts spec file name, relative to the document, when this sprite is a rig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AnimFrames {
    Seq(Vec<String>),
    ByDir(BTreeMap<String, Vec<String>>),
}

impl Default for AnimFrames {
    fn default() -> Self {
        AnimFrames::Seq(Vec::new())
    }
}

impl AnimFrames {
    pub fn sequence(&self, dir: Option<&str>) -> PxlResult<Vec<String>> {
        match self {
            AnimFrames::Seq(v) => Ok(v.clone()),
            AnimFrames::ByDir(map) => {
                let key = dir.unwrap_or("");
                if let Some(v) = map.get(key) {
                    return Ok(v.clone());
                }
                if dir.is_none() && map.len() == 1 {
                    return Ok(map.values().next().cloned().unwrap_or_default());
                }
                Err(
                    PxlError::semantic(format!("animation has no frames for direction '{}'", key))
                        .with_hint("pass --dir N|E|S|W matching the anim's dirs"),
                )
            }
        }
    }

    pub fn dirs(&self) -> Vec<String> {
        match self {
            AnimFrames::Seq(_) => Vec::new(),
            AnimFrames::ByDir(map) => map.keys().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Anim {
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(rename = "loop", default = "default_true")]
    pub looped: bool,
    #[serde(default)]
    pub dirs: Vec<String>,
    #[serde(default)]
    pub frames: AnimFrames,
}

fn default_fps() -> u32 {
    8
}

impl Anim {
    pub fn dir_list(&self) -> Vec<String> {
        if !self.dirs.is_empty() {
            self.dirs.clone()
        } else {
            self.frames.dirs()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PxlDoc {
    #[serde(default = "v1")]
    pub v: u32,
    pub palette: String,
    pub w: u16,
    pub h: u16,
    #[serde(default)]
    pub frames: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub anims: BTreeMap<String, Anim>,
    #[serde(default)]
    pub meta: Meta,
}

impl PxlDoc {
    pub fn frame(&self, name: &str) -> PxlResult<IndexFrame> {
        let rows = self.frames.get(name).ok_or_else(|| {
            PxlError::semantic(format!("unknown frame '{name}'"))
                .with_hint("use `pxl inspect` to list frame names")
        })?;
        let frame = pxl_core::decode_matrix(rows)?;
        if frame.w != self.w || frame.h != self.h {
            return Err(PxlError::schema(format!(
                "frame '{name}' is {}x{}, document is {}x{}",
                frame.w, frame.h, self.w, self.h
            )));
        }
        Ok(frame)
    }

    pub fn insert_frame(&mut self, name: &str, frame: &IndexFrame) -> PxlResult<()> {
        if frame.w != self.w || frame.h != self.h {
            return Err(PxlError::semantic(format!(
                "frame '{name}' is {}x{} but the document is {}x{}",
                frame.w, frame.h, self.w, self.h
            ))
            .with_hint("rotate90 keeps a square canvas; non-square rotate90 does not change the document size"));
        }
        self.frames
            .insert(name.to_string(), pxl_core::encode_matrix(frame)?);
        Ok(())
    }

    pub fn frame_names(&self) -> Vec<String> {
        self.frames.keys().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateColorFile {
    #[serde(default)]
    pub ramp_pool: Vec<String>,
    #[serde(default)]
    pub row_hue_change: f64,
    #[serde(default)]
    pub edge: String,
    #[serde(default)]
    pub noise: f64,
    #[serde(default)]
    pub gradient: String,
}

impl Default for TemplateColorFile {
    fn default() -> Self {
        Self {
            ramp_pool: Vec::new(),
            row_hue_change: 0.2,
            edge: "outline".into(),
            noise: 0.25,
            gradient: "vertical".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateFile {
    #[serde(default = "v1")]
    pub v: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_mirror_x")]
    pub mirror: String,
    pub cells: Vec<String>,
    #[serde(default)]
    pub legend: BTreeMap<String, String>,
    #[serde(default)]
    pub color: TemplateColorFile,
}

fn default_mirror_x() -> String {
    "x".into()
}

pub fn parse_template(text: &str) -> PxlResult<TemplateFile> {
    let value: serde_json::Value = crate::parse_json(text, "template")?;
    let inner = value
        .get("template")
        .cloned()
        .filter(|v| v.get("cells").is_some())
        .unwrap_or(value);
    serde_json::from_value(inner).map_err(|e| {
        PxlError::schema(format!("template: {e}")).with_hint("cells is a list of mask strings")
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutlineDraw {
    pub c: u8,
    #[serde(default = "default_outer_mode")]
    pub mode: String,
    #[serde(default = "default_square_brush")]
    pub brush: String,
    #[serde(default = "one_u8")]
    pub width: u8,
}

fn default_outer_mode() -> String {
    "outer".into()
}
fn default_square_brush() -> String {
    "square".into()
}
fn one_u8() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DrawCmd {
    Rect {
        rect: [i32; 4],
        c: u8,
        #[serde(default)]
        pattern: Option<String>,
        #[serde(default)]
        mirror: Option<bool>,
    },
    Circle {
        circle: [i32; 3],
        c: u8,
        #[serde(default)]
        mirror: Option<bool>,
    },
    Line {
        line: [i32; 4],
        c: u8,
        #[serde(default)]
        mirror: Option<bool>,
    },
    Pixels {
        pixels: Vec<String>,
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
        #[serde(default)]
        mirror: Option<bool>,
    },
    Outline {
        outline: OutlineDraw,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShapesFile {
    pub w: u16,
    pub h: u16,
    #[serde(default)]
    pub palette: Option<String>,
    #[serde(default)]
    pub mirror: Option<String>,
    pub draw: Vec<DrawCmd>,
}

pub fn parse_shapes(text: &str) -> PxlResult<ShapesFile> {
    crate::parse_json(text, "shapes")
}
