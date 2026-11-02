//! ASCII layout input. The text grid carries layout only; the legend maps each
//! character to a palette colour, a ramp step, a block pattern, or a stamp.

use std::collections::BTreeMap;

use pxl_core::{PxlError, PxlResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::Op;


/// Canvas sizes an ASCII layout may target: the four size tiers, all square.
pub const ASCII_SIZES: &[u16] = &pxl_core::SIZE_TIERS;

pub(crate) fn v1() -> u32 {
    1
}
pub(crate) fn default_size() -> u16 {
    32
}
pub(crate) fn default_cell() -> [u16; 2] {
    [1, 1]
}
pub(crate) fn default_true() -> bool {
    true
}
pub(crate) fn default_fps() -> u32 {
    8
}
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}
pub(crate) fn is_zero(value: &i32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Charset {
    /// `0` is transparent; other digits are palette indices unless the legend
    /// remaps them. Any other character is rejected.
    #[default]
    Digits,
    /// Printable ASCII (0x20..=0x7E). `.` and space are transparent by default.
    Printable,
}

impl Charset {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "digits" => Ok(Charset::Digits),
            "printable" => Ok(Charset::Printable),
            other => Err(PxlError::usage(format!("unknown charset '{other}'"))
                .with_hint("charset is digits (0-9) or printable (ASCII 0x20-0x7E)")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Charset::Digits => "digits",
            Charset::Printable => "printable",
        }
    }

    pub fn accepts(self, c: char) -> bool {
        match self {
            Charset::Digits => c.is_ascii_digit(),
            Charset::Printable => (' '..='~').contains(&c),
        }
    }

    /// Character used in a delta patch to mean "leave this cell alone".
    pub fn default_keep(self) -> char {
        match self {
            Charset::Digits => '.',
            Charset::Printable => '_',
        }
    }

    /// Characters that paint nothing when the legend does not mention them.
    pub fn default_transparent(self) -> &'static [char] {
        match self {
            Charset::Digits => &['0'],
            Charset::Printable => &['.', ' '],
        }
    }

    /// Fill used by `--pad` for a ragged row.
    pub fn pad_char(self) -> char {
        match self {
            Charset::Digits => '0',
            Charset::Printable => ' ',
        }
    }
}

/// How a row string is written. `rle` lets a 128 or 256 row stay short:
/// `12.4a3b` is twelve `.`, four `a`, three `b`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RowEncoding {
    #[default]
    Rows,
    Rle,
}

impl RowEncoding {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "rows" => Ok(Self::Rows),
            "rle" => Ok(Self::Rle),
            other => Err(PxlError::usage(format!("unknown encoding '{other}'"))
                .with_hint("encoding is rows or rle")),
        }
    }
}

/// Expand one run-length row: `run := count? cell`. A count is decimal; a
/// cell is any non-digit, or `'` followed by any character, so digit cells
/// are written `'3` (one `3`) or `4'3` (four `3`). Spaces between runs are
/// separators; an escaped space `' ` is a space cell.
pub fn expand_rle(row: &str, path: &str) -> PxlResult<String> {
    let mut out = String::new();
    let mut count: Option<usize> = None;
    let mut chars = row.chars();
    while let Some(c) = chars.next() {
        let cell = if c.is_ascii_digit() {
            let n = count.unwrap_or(0) * 10 + (c as u8 - b'0') as usize;
            if n > pxl_core::MAX_DIM as usize {
                return Err(PxlError::schema(format!(
                    "ascii.rle_count: run of {n} in {path} is longer than a 256 row"
                ))
                .with_path(path));
            }
            count = Some(n);
            continue;
        } else if c == '\'' {
            chars.next().ok_or_else(|| {
                PxlError::schema(format!("ascii.rle_escape: {path} ends with a quote"))
                    .with_path(path)
                    .with_hint("a quote escapes the next character, e.g. 4'3 is four 3s")
            })?
        } else if c == ' ' && count.is_none() {
            // Spaces between runs are separators, not cells.
            continue;
        } else {
            c
        };
        let n = count.take().unwrap_or(1);
        if n == 0 {
            return Err(
                PxlError::schema(format!("ascii.rle_count: zero-length run in {path}"))
                    .with_path(path),
            );
        }
        for _ in 0..n {
            out.push(cell);
        }
    }
    if count.is_some() {
        return Err(PxlError::schema(format!(
            "ascii.rle_count: {path} ends with a count and no cell"
        ))
        .with_path(path)
        .with_hint("every count is followed by the cell it repeats"));
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    Center,
    BottomCenter,
    TopLeft,
}

impl Anchor {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "center" => Ok(Anchor::Center),
            "bottom-center" => Ok(Anchor::BottomCenter),
            "top-left" => Ok(Anchor::TopLeft),
            other => Err(PxlError::usage(format!("unknown anchor '{other}'"))
                .with_hint("anchor is center, bottom-center, or top-left")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaletteMode {
    /// Every legend colour must already exist in the palette.
    #[default]
    Strict,
    /// Unknown legend colours are appended, and a palette file is written.
    Extend,
}

impl PaletteMode {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "strict" => Ok(PaletteMode::Strict),
            "extend" => Ok(PaletteMode::Extend),
            other => Err(PxlError::usage(format!("unknown palette_mode '{other}'"))
                .with_hint("palette_mode is strict or extend")),
        }
    }
}

/// One character's paint. A number is a palette index, a string is either a
/// `#rrggbb` colour or a role name, and an object may add a pattern or stamp.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LegendValue {
    Index(u8),
    Text(String),
    Entry(LegendEntry),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegendEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub step: i32,
    /// `solid` (default), `checker`, `stripe_x[:N]`, `stripe_y[:N]`, or `bevel`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<u8>,
    /// Cell-sized picture in the standard text-matrix alphabet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl LegendEntry {
    /// The one paint source this entry declares. `role` may carry a colour as a
    /// hint for generated palettes; every other field is exclusive.
    pub fn source(&self) -> PxlResult<LegendSource> {
        let mut sources = 0;
        if self.index.is_some() {
            sources += 1;
        }
        if self.ramp.is_some() {
            sources += 1;
        }
        if self.stamp.is_some() {
            sources += 1;
        }
        // `role` may carry a colour as a derivation hint, so the pair counts once.
        if self.role.is_some() || self.color.is_some() {
            sources += 1;
        }
        if sources != 1 {
            return Err(PxlError::schema(
                "a legend entry needs exactly one of index, color, role, ramp, or stamp",
            )
            .with_hint("example: {\"index\": 7} or {\"ramp\": \"skin\", \"step\": 2}"));
        }
        if self.stamp.is_some() && self.pattern.is_some() {
            return Err(PxlError::schema("a stamp cannot also set pattern")
                .with_hint("a stamp fills the cell by itself"));
        }
        if self.role.is_none() && self.color.is_some() && self.index.is_some() {
            return Err(PxlError::schema(
                "a legend entry cannot set both color and index",
            ));
        }
        if let Some(role) = &self.role {
            let color = match &self.color {
                Some(text) => Some(pxl_core::parse_hex(text)?),
                None => None,
            };
            return Ok(LegendSource::Role(Role::parse(role)?, color));
        }
        if let Some(index) = self.index {
            return Ok(LegendSource::Index(index));
        }
        if let Some(ramp) = &self.ramp {
            return Ok(LegendSource::Ramp(ramp.clone(), self.step));
        }
        if let Some(rows) = &self.stamp {
            return Ok(LegendSource::Stamp(rows.clone()));
        }
        if let Some(color) = &self.color {
            return Ok(LegendSource::Color(pxl_core::parse_hex(color)?));
        }
        Err(PxlError::schema("a legend entry declares no paint"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegendSource {
    Index(u8),
    Color(pxl_core::Rgba),
    Role(Role, Option<pxl_core::Rgba>),
    Ramp(String, i32),
    Stamp(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Transparent,
    Outline,
    Flash,
}

impl Role {
    pub fn parse(value: &str) -> PxlResult<Self> {
        match value {
            "transparent" => Ok(Role::Transparent),
            "outline" => Ok(Role::Outline),
            "flash" => Ok(Role::Flash),
            other => Err(PxlError::semantic(format!("unknown legend role '{other}'"))
                .with_hint("roles are transparent, outline, or flash")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiLayer {
    pub name: String,
    pub rows: Vec<String>,
    #[serde(default)]
    pub x: i16,
    #[serde(default)]
    pub y: i16,
    #[serde(default = "default_true")]
    pub visible: bool,
}

/// One block of a tiled frame, placed at `at` (cells).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiTile {
    pub at: [u16; 2],
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AsciiFrame {
    /// A layout grid written inline.
    Rows { rows: Vec<String> },
    /// A `size` x `size` grid assembled from blocks; empty areas are omitted.
    /// Suits 128 and 256 canvases, where one block can be written and fixed
    /// without touching the rest.
    Tiles { tiles: Vec<AsciiTile> },
    /// Composite these layers, in order, bottom to top.
    Layers { layers: Vec<String> },
    /// Start from `from` and change cells with `patch`, then run `ops`.
    Delta {
        from: String,
        #[serde(default)]
        patch: Vec<AsciiPatch>,
        #[serde(default)]
        ops: Vec<Op>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiPatch {
    pub x: u16,
    pub y: u16,
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiAnim {
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(rename = "loop", default = "default_true")]
    pub looped: bool,
    #[serde(default)]
    pub dirs: Vec<String>,
    #[serde(default)]
    pub mode: crate::recipe::AnimMode,
    /// Flipbook: frame names in play order.
    #[serde(default)]
    pub frames: Vec<String>,
    /// JSON key is `macro`.
    #[serde(rename = "macro", default)]
    pub macro_call: Option<String>,
    /// Explicit per-frame op lists, used when `macro` is absent.
    #[serde(default)]
    pub ops: Vec<Vec<Op>>,
    /// Source frame for `macro` or `ops`.
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiBlockRef {
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AsciiSource {
    pub file: String,
    pub sha256: String,
    #[serde(default)]
    pub blocks: Vec<AsciiBlockRef>,
}

