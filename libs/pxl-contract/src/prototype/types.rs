//! Prototype library contracts: prototypes, surface recipes, asset instances
//! and the catalog lock that freezes the build input closure.
//!
//! Shape, surface, and instance are separate records so structure and material
//! vary independently. Nothing here produces pixels; step 300 expands a frozen
//! instance deterministically.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::digest::{sha256_digest, valid_digest, valid_id};


pub const PROTOTYPE_CONTRACT: &str = "prototype/v1";
pub const SURFACE_RECIPE_CONTRACT: &str = "surface-recipe/v1";
pub const ASSET_INSTANCE_CONTRACT: &str = "asset-instance/v1";
pub const CATALOG_LOCK_CONTRACT: &str = "catalog.lock/v1";
pub const SCENE_KIT_CONTRACT: &str = "scene-kit/v1";

/// Largest edge a single prototype tier may declare.
pub const MAX_TIER_EDGE: u16 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrototypeKind {
    Building,
    Plant,
    Crop,
    Terrain,
    Prop,
    Fence,
    SceneKit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    TopDown,
    Side,
}

/// Review state is data, never inferred. Only a human reviewer sets `Approved`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawLayer {
    Back,
    Front,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeTier {
    pub id: String,
    pub size: [u16; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParamRange {
    Int { min: i64, max: i64, default: i64 },
    Choice { options: Vec<String>, default: String },
    Flag { default: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Flag(bool),
    Int(i64),
    Text(String),
}

impl ParamRange {
    pub fn problems(&self, name: &str) -> Vec<String> {
        match self {
            ParamRange::Int { min, max, default } => {
                let mut out = Vec::new();
                if min > max {
                    out.push(format!("param '{name}' has min > max"));
                } else if default < min || default > max {
                    out.push(format!("param '{name}' default is outside its range"));
                }
                out
            }
            ParamRange::Choice { options, default } => {
                let mut out = Vec::new();
                let unique: BTreeSet<_> = options.iter().collect();
                if options.is_empty() || unique.len() != options.len() {
                    out.push(format!("param '{name}' needs unique, non-empty options"));
                } else if !options.contains(default) {
                    out.push(format!("param '{name}' default is not one of its options"));
                }
                out
            }
            ParamRange::Flag { .. } => Vec::new(),
        }
    }

    /// Returns a message when `value` is not allowed by this range.
    pub fn reject(&self, name: &str, value: &ParamValue) -> Option<String> {
        match (self, value) {
            (ParamRange::Int { min, max, .. }, ParamValue::Int(v)) if v >= min && v <= max => None,
            (ParamRange::Int { min, max, .. }, ParamValue::Int(v)) => {
                Some(format!("param '{name}'={v} is outside {min}..={max}"))
            }
            (ParamRange::Choice { options, .. }, ParamValue::Text(v)) if options.contains(v) => None,
            (ParamRange::Choice { options, .. }, ParamValue::Text(v)) => {
                Some(format!("param '{name}'='{v}' is not one of {options:?}"))
            }
            (ParamRange::Flag { .. }, ParamValue::Flag(_)) => None,
            _ => Some(format!("param '{name}' has the wrong value type")),
        }
    }
}

/// An attachment point other prototypes can join to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub id: String,
    pub at: [i32; 2],
    /// One of N, E, S, W.
    pub dir: String,
    pub width: u16,
    pub accepts: Vec<String>,
}

/// Visual bounds, ordering point and collision are separate on purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    /// x, y, w, h of the drawn area.
    pub visual: [i32; 4],
    /// Row used to order this object against others (usually its base line).
    pub sort_y: i32,
    pub collision: Vec<[i32; 4]>,
    pub interact: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerSplit {
    pub id: String,
    pub regions: Vec<String>,
    pub layer: DrawLayer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrototypeSource {
    /// "ascii" or "shapes".
    pub kind: String,
    pub path: String,
    pub digest: String,
}

/// A hand-authored structural variant. When the instance sets `param` to
/// `equals`, `rows` are overlaid on `region` (anchored at the region's
/// bounding-box top-left; `.` leaves the pixel unchanged). Parameters therefore
/// only choose between variants an artist drew; they never synthesize geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantRule {
    pub param: String,
    pub equals: ParamValue,
    pub region: String,
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prototype {
    pub contract: String,
    pub id: String,
    pub version: u32,
    pub kind: PrototypeKind,
    pub view: ViewKind,
    pub size_tiers: Vec<SizeTier>,
    pub params: BTreeMap<String, ParamRange>,
    pub regions: Vec<String>,
    pub connectors: Vec<Connector>,
    pub footprint: Footprint,
    pub layers: Vec<LayerSplit>,
    pub variants: Vec<VariantRule>,
    pub source: PrototypeSource,
    /// Ids of surface recipes or prototypes this one needs at build time.
    pub requires: Vec<String>,
    pub license: String,
    pub review: ReviewState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRecipe {
    pub contract: String,
    pub id: String,
    pub version: u32,
    pub material_id: String,
    /// Region kinds this recipe may be applied to (roof, wall, foliage, soil...).
    pub applies_to: Vec<String>,
    /// Name of the existing finish/recipe operation that step 300 runs.
    pub recipe: String,
    pub params: BTreeMap<String, i64>,
    /// Pixels per texture cell; keeps texture scale tied to the tier.
    pub scale_px: u16,
    pub license: String,
    pub review: ReviewState,
}

/// One placed member of a scene kit: a prototype at an offset, with its own
/// parameter choices. Kits describe relationships (porch, tree group, field
/// edge); they hold no pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitItem {
    pub prototype: String,
    pub at: [i32; 2],
    pub params: BTreeMap<String, ParamValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneKit {
    pub contract: String,
    pub id: String,
    pub version: u32,
    /// Width and height of the area the kit occupies, in pixels.
    pub bounds: [u16; 2],
    pub items: Vec<KitItem>,
    pub license: String,
    pub review: ReviewState,
}

impl SceneKit {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("kit serializes"))
    }

    /// Ids of the prototypes this kit places, sorted and unique.
    pub fn requires(&self) -> Vec<String> {
        let ids: BTreeSet<&String> = self.items.iter().map(|i| &i.prototype).collect();
        ids.into_iter().cloned().collect()
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != SCENE_KIT_CONTRACT {
            errors.push(format!("kit '{who}': contract must be {SCENE_KIT_CONTRACT}"));
        }
        if !valid_id(&self.id) {
            errors.push(format!("kit '{who}': id must be lowercase ascii words joined by '_'"));
        }
        if self.version == 0 {
            errors.push(format!("kit '{who}': version must be at least 1"));
        }
        if self.bounds.contains(&0) {
            errors.push(format!("kit '{who}': bounds must be positive"));
        }
        if self.items.len() < 2 {
            errors.push(format!("kit '{who}': a kit relates at least two items"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("kit '{who}': license is required"));
        }
        for (n, item) in self.items.iter().enumerate() {
            if !valid_id(&item.prototype) {
                errors.push(format!("kit '{who}': item {n} has an invalid prototype id"));
            }
            if item.at[0] < 0
                || item.at[1] < 0
                || item.at[0] >= i32::from(self.bounds[0])
                || item.at[1] >= i32::from(self.bounds[1])
            {
                errors.push(format!("kit '{who}': item {n} lies outside the kit bounds"));
            }
        }
        errors
    }

    /// Check every item against the library: the prototype exists and each
    /// parameter value is inside the range that prototype declares.
    pub fn validate_items(&self, prototypes: &BTreeMap<String, Prototype>) -> Vec<String> {
        let mut errors = Vec::new();
        for (n, item) in self.items.iter().enumerate() {
            let Some(proto) = prototypes.get(&item.prototype) else {
                errors.push(format!("kit '{}': item {n} uses missing prototype '{}'", self.id, item.prototype));
                continue;
            };
            for (name, value) in &item.params {
                match proto.params.get(name) {
                    None => errors.push(format!("kit '{}': item {n} sets unknown param '{name}'", self.id)),
                    Some(range) => errors.extend(
                        range.reject(name, value).map(|e| format!("kit '{}': item {n}: {e}", self.id)),
                    ),
                }
            }
        }
        errors
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrototypeRef {
    pub id: String,
    pub version: u32,
    pub digest: String,
}

/// Replacement pixels for one named region, in the same ASCII row form as the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionPatch {
    pub region: String,
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetInstance {
    pub contract: String,
    pub instance_id: String,
    pub prototype: PrototypeRef,
    pub size_tier: String,
    pub params: BTreeMap<String, ParamValue>,
    /// region -> surface recipe id.
    pub surfaces: BTreeMap<String, String>,
    pub seed: u64,
    pub patches: Vec<RegionPatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Prototype,
    Surface,
    Kit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedEntry {
    pub id: String,
    pub kind: EntryKind,
    pub version: u32,
    pub digest: String,
    pub requires: Vec<String>,
    pub review: ReviewState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogLock {
    pub contract: String,
    pub entries: Vec<LockedEntry>,
    pub digest: String,
}

