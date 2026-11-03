//! Unified sprite source spec (v3). One file describes what the art is, how
//! big it is, and which finishing the size tier implies; every entry point
//! (`pxl sprite build`, recipes, the engine) reads the same file.
//!
//! ```json
//! {
//!   "v": 3,
//!   "size": 128,
//!   "palette": "palette.json",
//!   "source": { "kind": "svg", "file": "knight.svg" },
//!   "finish": { "outline": "selout_inner", "shade": "volume" },
//!   "profile": { "max_colors": 40 },
//!   "variants": [32, 64]
//! }
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use pxl_core::{PxlError, PxlResult, SizeTier};

fn v3() -> u32 {
    3
}
fn default_size() -> u16 {
    64
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Inline rows/rle/tiles, or `file` holding a `*.ascii.json`/`*.ascii.txt`.
    Ascii,
    Svg,
    Shapes,
    /// Another `*.pxl.json` document.
    Doc,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    pub kind: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Inline SVG text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    /// ASCII layout compiled at this size, then upscaled to `size`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<u16>,
    /// ASCII rows, when the layout is inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Raster {
    /// Samples per pixel and axis; unset picks by size tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u8>,
    /// `strict`, `nearest` or `extend`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_bands: Option<usize>,
    /// Snap frame translations to whole pixels.
    #[serde(default)]
    pub snap: bool,
}

impl Default for Raster {
    fn default() -> Self {
        Self {
            samples: None,
            color_mode: None,
            gradient_bands: None,
            snap: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FinishSpec {
    /// `keep`, `none`, `flat`, `selout`, `selout_lit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<String>,
    /// `none` or `volume`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade: Option<String>,
    /// `off`, `corners`, `on`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aa: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dither: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_lines: Option<bool>,
    /// `top_left`, `top_right`, `top`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    /// `all`, `none`, or a list of orphan, elbow, notch, rhythm, symmetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<String>,
    /// Skip the finishing pass entirely.
    #[serde(default)]
    pub skip: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ProfileOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_colors: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_levels: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpriteSpec {
    #[serde(default = "v3")]
    pub v: u32,
    #[serde(default = "default_size")]
    pub size: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    /// `strict` or `extend`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub source: Source,
    #[serde(default)]
    pub raster: Raster,
    #[serde(default)]
    pub finish: FinishSpec,
    #[serde(default)]
    pub profile: ProfileOverrides,
    /// Extra sizes to write from the same source (SVG only).
    #[serde(default)]
    pub variants: Vec<u16>,
    /// Optional bottom-center anchor in basis points of the native canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_bp: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_tolerance_bp: Option<u32>,
}

impl SpriteSpec {
    pub fn height(&self) -> u16 {
        self.h.unwrap_or(self.size)
    }

    pub fn tier(&self) -> PxlResult<SizeTier> {
        SizeTier::for_size(self.size, self.height())
    }

    pub fn validate(&self) -> PxlResult<()> {
        if self.v != 3 {
            return Err(PxlError::schema(format!(
                "sprite spec version {} is not supported",
                self.v
            ))
            .with_path("/v")
            .with_hint("set v to 3"));
        }
        self.tier()?;
        if self.source.file.is_none() && self.source.svg.is_none() && self.source.rows.is_none() {
            return Err(
                PxlError::schema("sprite.no_input: source needs file, svg, or rows")
                    .with_path("/source"),
            );
        }
        if self.source.native.is_some() && !matches!(self.source.kind, SourceKind::Ascii) {
            return Err(PxlError::schema(
                "sprite.bad_native: source.native only applies to an ascii layout",
            )
            .with_path("/source/native"));
        }
        for v in &self.variants {
            SizeTier::for_size(*v, *v)?;
        }
        if self.variants.contains(&self.size) {
            return Err(PxlError::schema(
                "sprite.variant_duplicate: variants repeat the main size",
            )
            .with_path("/variants"));
        }
        Ok(())
    }
}

pub fn parse_sprite_spec(text: &str) -> PxlResult<SpriteSpec> {
    let spec: SpriteSpec = crate::parse_json(text, "sprite spec")?;
    spec.validate()?;
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_svg_spec_parses() {
        let s = parse_sprite_spec(r##"{"v":3,"size":128,"source":{"kind":"svg","file":"k.svg"}}"##)
            .unwrap();
        assert_eq!(s.height(), 128);
        assert_eq!(s.tier().unwrap(), SizeTier::S128);
        assert!(s.raster.snap);
    }

    #[test]
    fn bad_versions_sizes_and_inputs_are_named() {
        for (json, code) in [
            (
                r##"{"v":2,"source":{"kind":"svg","file":"k.svg"}}"##,
                "sprite spec version",
            ),
            (
                r##"{"v":3,"size":300,"source":{"kind":"svg","file":"k.svg"}}"##,
                "profile.bad_size",
            ),
            (r##"{"v":3,"source":{"kind":"svg"}}"##, "sprite.no_input"),
            (
                r##"{"v":3,"size":64,"source":{"kind":"svg","file":"k.svg"},"variants":[64]}"##,
                "sprite.variant_duplicate",
            ),
            (
                r##"{"v":3,"size":64,"source":{"kind":"svg","file":"k.svg","native":32}}"##,
                "sprite.bad_native",
            ),
        ] {
            let e = parse_sprite_spec(json).unwrap_err();
            assert!(e.message.contains(code), "{json}: {}", e.message);
        }
    }
}
