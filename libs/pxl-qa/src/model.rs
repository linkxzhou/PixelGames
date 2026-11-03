use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PixelLocation {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Check {
    pub id: String,
    pub pass: bool,
    pub value: String,
    pub limit: String,
    pub severity: Severity,
    pub samples: Vec<PixelLocation>,
}

impl Check {
    pub fn failed(&self) -> bool {
        !self.pass && self.severity == Severity::Error
    }

    pub(crate) fn new(
        id: impl Into<String>,
        pass: bool,
        value: impl ToString,
        limit: impl Into<String>,
        severity: Severity,
    ) -> Self {
        Self {
            id: id.into(),
            pass,
            value: value.to_string(),
            limit: limit.into(),
            severity,
            samples: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Report {
    pub rules: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| !c.failed())
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimSeq {
    /// Display key used in check ids. The first asset to use an animation name
    /// keeps it bare; later assets get `<asset>_<name>`.
    pub name: String,
    /// Asset that owns this animation. Empty for a single-asset document.
    pub asset: String,
    /// Original animation name, retained as caller metadata only.
    pub base_name: String,
    pub dir: Option<String>,
    pub looped: bool,
    pub frames: Vec<IndexFrame>,
    /// Frame id for each entry of `frames`, in the same order. Identity, not
    /// pixel equality, decides which uses of a frame an exemption covers.
    pub frame_names: Vec<String>,
    /// Declared as a deliberate single-frame anim (idle/fly) by the frozen
    /// design. Static anims skip adjacent-frame difference checks; dynamic
    /// anims keep the minimum frame count and anti-duplication rules.
    pub static_mode: bool,
    pub anchor: Option<[i32; 2]>,
    pub anchor_tolerance_px: Option<u32>,
    pub protected_regions: Vec<pxl_core::Rect>,
    pub change_regions: Vec<pxl_core::Rect>,
    pub allow_duplicate_frames: Option<bool>,
    pub allow_empty_frames: Option<bool>,
    pub allow_terminal_empty: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct ValidateInput {
    pub palette: Palette,
    pub frames: Vec<(String, IndexFrame)>,
    pub anims: Vec<AnimSeq>,
    /// Additional named reference frames; no direction vocabulary is assumed.
    pub facings: BTreeMap<String, IndexFrame>,
    pub tiles: Vec<(String, IndexFrame)>,
    pub tile_ramp: String,
    /// Ramp per tile frame when a sheet mixes materials. Falls back to
    /// `tile_ramp` for any tile that is not listed here.
    pub tile_ramps: BTreeMap<String, String>,
    /// Caller metadata retained for input compatibility; never selects QA rules.
    pub asset_kinds: BTreeMap<String, String>,
    /// Frame ownership metadata retained for input compatibility.
    pub frame_assets: BTreeMap<String, String>,
}

impl ValidateInput {
    pub(crate) fn ramp_for(&self, name: &str) -> &str {
        self.tile_ramps
            .get(name)
            .map(String::as_str)
            .unwrap_or(&self.tile_ramp)
    }
}
