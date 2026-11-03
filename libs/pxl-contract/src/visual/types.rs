use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};


pub const VISUAL_INTENT_CONTRACT: &str = "visual-intent/v1";
/// v2 adds the opt-in modern profile and palette families. Fields that only
/// exist in v2 are rejected in v1 files.
pub const VISUAL_INTENT_CONTRACT_V2: &str = "visual-intent/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualIntent {
    pub contract: String,
    pub profile: ProfileRef,
    pub pixel_grid: PixelGrid,
    pub palette_policy: PalettePolicy,
    pub roles: BTreeMap<String, RolePolicy>,
    pub value_script: ValueScript,
    pub materials: BTreeMap<String, MaterialPolicy>,
    pub asset_policies: Vec<AssetPolicy>,
    pub animation_policy: AnimationPolicy,
    pub scene_previews: Vec<ScenePreview>,
    pub review_policy: ReviewPolicy,
    pub sources: BTreeMap<String, ConfigOrigin>,
    pub risks: Vec<VisualRisk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modern: Option<crate::policy::ModernProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette_families: Vec<PaletteFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteFamily {
    pub id: String,
    pub palette: String,
    pub digest: String,
    pub semantic_digest: String,
    pub transparent_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Facing {
    Symmetric,
    Left,
    Right,
    FourWay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRef {
    pub id: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelGrid {
    pub native_size: [u16; 2],
    pub display_scale: u16,
    pub snap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteMode {
    Strict,
    ExplicitExtension,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalettePolicy {
    pub mode: PaletteMode,
    pub palette: String,
    pub allowed_indices: Vec<u8>,
    pub ui_palette: Option<String>,
    pub runtime_grade: bool,
    pub extension_budget: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RolePolicy {
    pub value: [u8; 2],
    pub saturation: [u8; 2],
    pub relative_contrast: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueScript {
    pub focal_roles: Vec<String>,
    pub rest_roles: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishPolicy {
    Keep,
    Flat,
    Volume,
    Selout,
    HardEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialResponse {
    Flat,
    Planar,
    Round,
    HardEdge,
    Emissive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialPolicy {
    pub ramp: String,
    pub finish: FinishPolicy,
    pub response: MaterialResponse,
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingMode {
    Baked,
    RuntimeLit,
    Hybrid,
    Unlit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPolicy {
    pub asset_id: String,
    pub kind: String,
    pub role: String,
    pub native_size: Option<[u16; 2]>,
    pub material_ids: Vec<String>,
    pub protected_regions: Vec<String>,
    pub finish: FinishPolicy,
    pub lighting: LightingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<Facing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPolicy {
    pub max_frames: u8,
    pub hold_terminal: bool,
    pub allow_squash: bool,
    pub allow_smear: bool,
    pub required_phases: Vec<String>,
    pub contact_tolerance: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenePreview {
    pub id: String,
    pub size: [u16; 2],
    pub background: String,
    pub scale: u16,
    pub tick: u32,
    pub instances: Vec<SceneInstance>,
    pub rois: Vec<ReviewRoi>,
    #[serde(default)]
    pub render_mode: Option<String>,
    #[serde(default)]
    pub lights: Vec<SceneLight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneLight {
    pub id: String,
    pub position: [i32; 2],
    pub radius: u32,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneInstance {
    pub asset_id: String,
    pub position: [i32; 2],
    pub layer: String,
    pub frame: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRoi {
    pub id: String,
    pub layer: String,
    pub rect: [u32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicy {
    pub required_dimensions: Vec<String>,
    pub min_reviewers: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOrigin {
    User,
    Project,
    Profile,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Open,
    Resolved,
    Accepted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualRisk {
    pub risk_id: String,
    pub target: String,
    pub check: String,
    pub evidence_required: String,
    pub status: RiskStatus,
    pub resolved_by: Option<EvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub report: String,
    pub entry: String,
    pub digest: String,
}

