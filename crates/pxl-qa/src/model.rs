use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub id: String,
    pub pass: bool,
    pub value: String,
    pub limit: String,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub rules: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.pass)
    }
}

#[derive(Clone, Debug)]
pub struct AnimSeq {
    /// Display key used in check ids. The first asset to use an animation name
    /// keeps it bare; later assets get `<asset>_<name>`.
    pub name: String,
    /// Asset that owns this animation. Empty for a single-asset document.
    pub asset: String,
    /// The animation's own name (`walk`, `move`), without any asset prefix.
    /// Project rules match on this, never on `name`.
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
}

#[derive(Clone, Debug)]
pub struct ValidateInput {
    pub palette: Palette,
    pub frames: Vec<(String, IndexFrame)>,
    pub anims: Vec<AnimSeq>,
    /// Reference sprite for each facing, used by the tank bbox check.
    pub facings: BTreeMap<String, IndexFrame>,
    pub tiles: Vec<(String, IndexFrame)>,
    pub tile_ramp: String,
    /// Ramp per tile frame when a sheet mixes materials. Falls back to
    /// `tile_ramp` for any tile that is not listed here.
    pub tile_ramps: BTreeMap<String, String>,
    /// Declared use of each asset (`sprite`, `tiles`, `effect`, `ui`,
    /// `overlay`), keyed by `AnimSeq::asset`. Missing means `sprite`.
    pub asset_kinds: BTreeMap<String, String>,
    /// Owning asset of each frame name, so per-frame rules follow the asset's
    /// declared use. A frame with no entry is judged as a sprite.
    pub frame_assets: BTreeMap<String, String>,
}

impl ValidateInput {
    pub(crate) fn ramp_for(&self, name: &str) -> &str {
        self.tile_ramps
            .get(name)
            .map(String::as_str)
            .unwrap_or(&self.tile_ramp)
    }

    pub(crate) fn kind_of_asset(&self, asset: &str) -> &str {
        self.asset_kinds
            .get(asset)
            .map(String::as_str)
            .unwrap_or("sprite")
    }

    pub(crate) fn kind_of_frame(&self, frame: &str) -> &str {
        match self.frame_assets.get(frame) {
            Some(asset) => self.kind_of_asset(asset),
            None => "sprite",
        }
    }
}
