//! ASCII layout input. The text grid carries layout only; the legend maps each
//! character to a palette colour, a ramp step, a block pattern, or a stamp.

use std::collections::BTreeMap;

use pxl_core::{PxlError, PxlResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::Op;


use super::charset::*;
use super::charset::{default_cell, default_fps, default_size, default_true, is_false, is_zero, v1};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AsciiSpec {
    #[serde(default = "v1")]
    pub v: u32,
    #[serde(default)]
    pub charset: Charset,
    #[serde(default = "default_size")]
    pub size: u16,
    /// `rows` (default) or `rle` for every row string in the spec.
    #[serde(default)]
    pub encoding: RowEncoding,
    #[serde(default = "default_cell")]
    pub cell: [u16; 2],
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default)]
    pub offset: [i16; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(default)]
    pub palette_mode: PaletteMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend_ref: Option<String>,
    #[serde(default)]
    pub legend: BTreeMap<String, LegendValue>,
    /// Patch character meaning "leave this cell alone". Defaults per charset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<char>,
    #[serde(default)]
    pub layers: Vec<AsciiLayer>,
    #[serde(default)]
    pub frames: BTreeMap<String, AsciiFrame>,
    #[serde(default)]
    pub anims: Vec<AsciiAnim>,
    #[serde(default)]
    pub post: Vec<Op>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<AsciiSource>,
}

impl AsciiSpec {
    /// A row as cells, after `rle` expansion.
    pub fn row_cells(&self, row: &str, path: &str) -> PxlResult<Vec<char>> {
        match self.encoding {
            RowEncoding::Rows => Ok(row.chars().collect()),
            RowEncoding::Rle => Ok(expand_rle(row, path)?.chars().collect()),
        }
    }

    pub fn keep_char(&self) -> char {
        self.keep.unwrap_or_else(|| self.charset.default_keep())
    }

    pub fn cell_w(&self) -> u16 {
        self.cell[0]
    }

    pub fn cell_h(&self) -> u16 {
        self.cell[1]
    }

    /// Checks the values every entry point has to agree on. Frame content is
    /// checked while compiling, where the palette is known.
    pub fn validate(&self) -> PxlResult<()> {
        if self.v != 1 {
            return Err(PxlError::schema(format!(
                "ascii spec version {} is not supported",
                self.v
            ))
            .with_path("/v")
            .with_hint("set v to 1"));
        }
        if !ASCII_SIZES.contains(&self.size) {
            return Err(PxlError::schema(format!(
                "ascii.bad_size: canvas size {} is not 32, 64, 128 or 256",
                self.size
            ))
            .with_path("/size")
            .with_hint("ascii layouts use a 32, 64, 128 or 256 pixel square canvas"));
        }
        if self.cell[0] == 0 || self.cell[1] == 0 || self.cell[0] > 64 || self.cell[1] > 64 {
            return Err(PxlError::schema(format!(
                "ascii.bad_cell: cell is {}x{}",
                self.cell[0], self.cell[1]
            ))
            .with_path("/cell")
            .with_hint("a cell is 1..=64 pixels on each side"));
        }
        if let Some(mirror) = &self.mirror {
            pxl_core::Mirror::parse(mirror).map_err(|e| e.with_path("/mirror"))?;
        }
        check_legend_keys(&self.legend, self.charset, "/legend")?;
        if let Some(keep) = self.keep {
            if self.legend.contains_key(&keep.to_string()) {
                return Err(PxlError::schema(format!(
                    "keep character '{keep}' is also a legend key"
                ))
                .with_path("/keep")
                .with_hint("keep must not paint pixels; pick a character the layout never uses"));
            }
        }
        if self.frames.is_empty() && self.layers.is_empty() {
            return Err(
                PxlError::schema("ascii.no_layout: the spec has no frames and no layers")
                    .with_hint("add a frames entry, or layers plus an empty frames map"),
            );
        }
        for (i, anim) in self.anims.iter().enumerate() {
            let sources = [
                !anim.frames.is_empty(),
                anim.macro_call.is_some(),
                !anim.ops.is_empty(),
            ]
            .iter()
            .filter(|on| **on)
            .count();
            if sources != 1 {
                return Err(PxlError::schema(format!(
                    "anim '{}' needs exactly one of frames, macro, or ops",
                    anim.name
                ))
                .with_path(format!("/anims/{i}")));
            }
        }
        Ok(())
    }
}

pub fn check_legend_keys(
    legend: &BTreeMap<String, LegendValue>,
    charset: Charset,
    path: &str,
) -> PxlResult<()> {
    for key in legend.keys() {
        let mut chars = key.chars();
        let Some(c) = chars.next() else {
            return Err(PxlError::schema("legend key is empty").with_path(path));
        };
        if chars.next().is_some() {
            return Err(
                PxlError::schema(format!("legend key '{key}' is more than one character"))
                    .with_path(format!("{path}/{key}"))
                    .with_hint("one character is one cell"),
            );
        }
        if !charset.accepts(c) {
            return Err(PxlError::schema(format!(
                "ascii.bad_char: legend key '{c}' is not allowed by charset {}",
                charset.name()
            ))
            .with_path(format!("{path}/{key}"))
            .with_hint(match charset {
                Charset::Digits => {
                    "charset digits allows 0-9; pass --charset printable for symbols"
                }
                Charset::Printable => "charset printable allows ASCII 0x20-0x7E",
            }));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LegendFile {
    #[serde(default = "v1")]
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<Charset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(default)]
    pub legend: BTreeMap<String, LegendValue>,
}


pub fn parse_ascii_spec(text: &str) -> PxlResult<AsciiSpec> {
    let spec: AsciiSpec = crate::parse_json(text, "ascii spec")?;
    spec.validate()?;
    Ok(spec)
}

pub fn parse_legend_file(text: &str) -> PxlResult<LegendFile> {
    let file: LegendFile = crate::parse_json(text, "ascii legend")?;
    if let Some(charset) = file.charset {
        check_legend_keys(&file.legend, charset, "/legend")?;
    }
    Ok(file)
}
