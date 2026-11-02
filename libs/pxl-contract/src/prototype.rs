//! Prototype library contracts: prototypes, surface recipes, asset instances
//! and the catalog lock that freezes the build input closure.
//!
//! Shape, surface, and instance are separate records so structure and material
//! vary independently. Nothing here produces pixels; step 300 expands a frozen
//! instance deterministically.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    fn problems(&self, name: &str) -> Vec<String> {
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

pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut hex = String::with_capacity(71);
    hex.push_str("sha256:");
    for byte in out {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Ids are lowercase ascii words joined by underscores, so they stay stable
/// as file names and as cross-reference keys.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !value.starts_with(|c: char| c.is_ascii_digit())
}

impl Prototype {
    /// Digest over the serialized record. Field order is fixed by the struct
    /// and all maps are ordered, so equal content gives an equal digest.
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("prototype serializes"))
    }

    pub fn reference(&self) -> PrototypeRef {
        PrototypeRef { id: self.id.clone(), version: self.version, digest: self.digest() }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != PROTOTYPE_CONTRACT {
            errors.push(format!("prototype '{who}': contract must be {PROTOTYPE_CONTRACT}"));
        }
        if !valid_id(&self.id) {
            errors.push(format!("prototype '{who}': id must be lowercase ascii words joined by '_'"));
        }
        if self.version == 0 {
            errors.push(format!("prototype '{who}': version must be at least 1"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("prototype '{who}': license is required"));
        }
        if !valid_digest(&self.source.digest) {
            errors.push(format!("prototype '{who}': source digest must be sha256"));
        }
        if !matches!(self.source.kind.as_str(), "ascii" | "shapes") || self.source.path.trim().is_empty() {
            errors.push(format!("prototype '{who}': source must be ascii or shapes with a path"));
        }

        let mut max_h = 0i32;
        let mut tier_ids = BTreeSet::new();
        if self.size_tiers.is_empty() {
            errors.push(format!("prototype '{who}': at least one size tier is required"));
        }
        for tier in &self.size_tiers {
            if !tier_ids.insert(tier.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate size tier '{}'", tier.id));
            }
            if tier.size.contains(&0) || tier.size.iter().any(|edge| *edge > MAX_TIER_EDGE) {
                errors.push(format!("prototype '{who}': tier '{}' must be 1..={MAX_TIER_EDGE} per edge", tier.id));
            }
            max_h = max_h.max(i32::from(tier.size[1]));
        }

        for (name, range) in &self.params {
            if !valid_id(name) {
                errors.push(format!("prototype '{who}': param name '{name}' is not a valid id"));
            }
            errors.extend(range.problems(name).into_iter().map(|e| format!("prototype '{who}': {e}")));
        }

        let mut region_ids = BTreeSet::new();
        if self.regions.is_empty() {
            errors.push(format!("prototype '{who}': at least one region is required"));
        }
        for region in &self.regions {
            if !valid_id(region) || !region_ids.insert(region.as_str()) {
                errors.push(format!("prototype '{who}': region '{region}' is invalid or duplicated"));
            }
        }

        let mut connector_ids = BTreeSet::new();
        for connector in &self.connectors {
            if !connector_ids.insert(connector.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate connector '{}'", connector.id));
            }
            if !matches!(connector.dir.as_str(), "N" | "E" | "S" | "W") {
                errors.push(format!("prototype '{who}': connector '{}' dir must be N/E/S/W", connector.id));
            }
            if connector.width == 0 || connector.accepts.is_empty() {
                errors.push(format!("prototype '{who}': connector '{}' needs a width and accepted kinds", connector.id));
            }
        }

        if self.footprint.visual[2] <= 0 || self.footprint.visual[3] <= 0 {
            errors.push(format!("prototype '{who}': visual bounds must have positive size"));
        }
        if max_h > 0 && (self.footprint.sort_y < 0 || self.footprint.sort_y > max_h) {
            errors.push(format!("prototype '{who}': sort_y must lie within the tallest tier"));
        }
        if self.footprint.collision.iter().any(|r| r[2] <= 0 || r[3] <= 0) {
            errors.push(format!("prototype '{who}': collision boxes must have positive size"));
        }

        let mut layer_ids = BTreeSet::new();
        for layer in &self.layers {
            if !layer_ids.insert(layer.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate layer '{}'", layer.id));
            }
            if layer.regions.is_empty() || layer.regions.iter().any(|r| !region_ids.contains(r.as_str())) {
                errors.push(format!("prototype '{who}': layer '{}' references an unknown region", layer.id));
            }
        }

        for (n, rule) in self.variants.iter().enumerate() {
            match self.params.get(&rule.param) {
                None => errors.push(format!("prototype '{who}': variant {n} uses unknown param '{}'", rule.param)),
                Some(range) => {
                    if let Some(message) = range.reject(&rule.param, &rule.equals) {
                        errors.push(format!("prototype '{who}': variant {n}: {message}"));
                    }
                }
            }
            if !region_ids.contains(rule.region.as_str()) {
                errors.push(format!("prototype '{who}': variant {n} targets unknown region '{}'", rule.region));
            }
            if rule.rows.is_empty() || rule.rows.iter().any(|r| r.is_empty()) {
                errors.push(format!("prototype '{who}': variant {n} needs non-empty rows"));
            }
        }

        if self.requires.iter().any(|r| r == &self.id) {
            errors.push(format!("prototype '{who}': cannot require itself"));
        }
        errors
    }
}

impl SurfaceRecipe {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("surface serializes"))
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != SURFACE_RECIPE_CONTRACT {
            errors.push(format!("surface '{who}': contract must be {SURFACE_RECIPE_CONTRACT}"));
        }
        if !valid_id(&self.id) || !valid_id(&self.material_id) {
            errors.push(format!("surface '{who}': id and material_id must be valid ids"));
        }
        if self.version == 0 {
            errors.push(format!("surface '{who}': version must be at least 1"));
        }
        if self.applies_to.is_empty() {
            errors.push(format!("surface '{who}': applies_to must name at least one region kind"));
        }
        if self.recipe.trim().is_empty() {
            errors.push(format!("surface '{who}': recipe name is required"));
        }
        if self.scale_px == 0 {
            errors.push(format!("surface '{who}': scale_px must be positive"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("surface '{who}': license is required"));
        }
        errors
    }
}

impl AssetInstance {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("instance serializes"))
    }

    /// Checks the instance against the exact prototype it points to.
    /// `surfaces` maps surface id -> recipe, so missing recipes are reported.
    pub fn validate_against(
        &self,
        prototype: &Prototype,
        surfaces: &BTreeMap<String, SurfaceRecipe>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.instance_id;
        if self.contract != ASSET_INSTANCE_CONTRACT {
            errors.push(format!("instance '{who}': contract must be {ASSET_INSTANCE_CONTRACT}"));
        }
        if !valid_id(&self.instance_id) {
            errors.push(format!("instance '{who}': instance_id must be a valid id"));
        }
        if self.prototype.id != prototype.id || self.prototype.version != prototype.version {
            errors.push(format!(
                "instance '{who}': points at {}@{} but prototype is {}@{}",
                self.prototype.id, self.prototype.version, prototype.id, prototype.version
            ));
        } else if self.prototype.digest != prototype.digest() {
            errors.push(format!("instance '{who}': prototype digest does not match the library entry"));
        }
        if !prototype.size_tiers.iter().any(|t| t.id == self.size_tier) {
            errors.push(format!("instance '{who}': unknown size tier '{}'", self.size_tier));
        }
        for (name, value) in &self.params {
            match prototype.params.get(name) {
                None => errors.push(format!("instance '{who}': unknown param '{name}'")),
                Some(range) => errors.extend(range.reject(name, value).map(|e| format!("instance '{who}': {e}"))),
            }
        }
        for (region, surface_id) in &self.surfaces {
            if !prototype.regions.contains(region) {
                errors.push(format!("instance '{who}': surface set on unknown region '{region}'"));
            }
            if !surfaces.contains_key(surface_id) {
                errors.push(format!("instance '{who}': surface '{surface_id}' is not in the catalog"));
            }
        }
        for patch in &self.patches {
            if !prototype.regions.contains(&patch.region) {
                errors.push(format!("instance '{who}': patch targets unknown region '{}'", patch.region));
            }
            if patch.rows.is_empty() {
                errors.push(format!("instance '{who}': patch for '{}' has no rows", patch.region));
            }
        }
        errors
    }
}

impl CatalogLock {
    fn compute_digest(entries: &[LockedEntry]) -> String {
        sha256_digest(&serde_json::to_vec(entries).expect("entries serialize"))
    }

    /// Sorts entries by id so the lock does not depend on discovery order.
    pub fn seal(mut entries: Vec<LockedEntry>) -> Self {
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        let digest = Self::compute_digest(&entries);
        Self { contract: CATALOG_LOCK_CONTRACT.into(), entries, digest }
    }

    /// Verifies ordering, digests, dependency closure and (optionally) review state.
    pub fn verify(&self, require_approved: bool) -> Vec<String> {
        let mut errors = Vec::new();
        if self.contract != CATALOG_LOCK_CONTRACT {
            errors.push(format!("catalog lock: contract must be {CATALOG_LOCK_CONTRACT}"));
        }
        let mut ids = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                errors.push(format!("catalog lock: duplicate entry '{}'", entry.id));
            }
            if previous.is_some_and(|p| p >= entry.id.as_str()) {
                errors.push(format!("catalog lock: entry '{}' is out of order", entry.id));
            }
            previous = Some(entry.id.as_str());
            if !valid_digest(&entry.digest) {
                errors.push(format!("catalog lock: entry '{}' has an invalid digest", entry.id));
            }
            if require_approved && entry.review != ReviewState::Approved {
                errors.push(format!("catalog lock: entry '{}' is not approved ({:?})", entry.id, entry.review));
            }
        }
        for entry in &self.entries {
            for dep in &entry.requires {
                if !ids.contains(dep.as_str()) {
                    errors.push(format!("catalog lock: '{}' requires missing entry '{dep}'", entry.id));
                }
            }
        }
        if let Some(id) = self.find_cycle() {
            errors.push(format!("catalog lock: dependency cycle through '{id}'"));
        }
        if self.digest != Self::compute_digest(&self.entries) {
            errors.push("catalog lock: digest does not match its entries".into());
        }
        errors
    }

    /// Returns an entry id that lies on a cycle, if any.
    fn find_cycle(&self) -> Option<String> {
        let deps: BTreeMap<&str, &[String]> =
            self.entries.iter().map(|e| (e.id.as_str(), e.requires.as_slice())).collect();
        // 0 = unvisited, 1 = on the current path, 2 = done
        let mut state: BTreeMap<&str, u8> = BTreeMap::new();

        fn visit<'a>(
            id: &'a str,
            deps: &BTreeMap<&'a str, &'a [String]>,
            state: &mut BTreeMap<&'a str, u8>,
        ) -> Option<String> {
            match state.get(id).copied().unwrap_or(0) {
                1 => return Some(id.to_string()),
                2 => return None,
                _ => {}
            }
            state.insert(id, 1);
            if let Some(next) = deps.get(id) {
                for dep in next.iter() {
                    if deps.contains_key(dep.as_str()) {
                        if let Some(found) = visit(dep.as_str(), deps, state) {
                            return Some(found);
                        }
                    }
                }
            }
            state.insert(id, 2);
            None
        }

        for id in deps.keys().copied() {
            if let Some(found) = visit(id, &deps, &mut state) {
                return Some(found);
            }
        }
        None
    }
}

#[cfg(test)]
mod kit_tests {
    use super::*;

    fn kit() -> SceneKit {
        SceneKit {
            contract: SCENE_KIT_CONTRACT.into(),
            id: "porch_group".into(),
            version: 1,
            bounds: [48, 32],
            items: vec![
                KitItem { prototype: "door_wood".into(), at: [16, 8], params: BTreeMap::new() },
                KitItem { prototype: "flowerpot".into(), at: [4, 24], params: BTreeMap::new() },
            ],
            license: "original".into(),
            review: ReviewState::Pending,
        }
    }

    #[test]
    fn a_consistent_kit_validates_and_lists_its_dependencies() {
        let k = kit();
        assert_eq!(k.validate(), Vec::<String>::new());
        assert_eq!(k.requires(), vec!["door_wood".to_string(), "flowerpot".to_string()]);
        assert_eq!(k.digest(), kit().digest());
    }

    #[test]
    fn bad_kits_are_rejected_with_reasons() {
        let mut k = kit();
        k.items.truncate(1);
        k.items[0].at = [99, 0];
        k.bounds = [48, 0];
        k.license.clear();
        let errors = k.validate().join("\n");
        for needle in ["bounds must be positive", "at least two items", "outside the kit bounds", "license"] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }

    #[test]
    fn items_must_name_real_prototypes_and_legal_params() {
        let mut k = kit();
        k.items[1].params.insert("tall".into(), ParamValue::Flag(true));
        let errors = k.validate_items(&BTreeMap::new()).join("\n");
        assert!(errors.contains("missing prototype 'door_wood'"), "{errors}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_digest() -> String {
        sha256_digest(b"source")
    }

    fn house() -> Prototype {
        Prototype {
            contract: PROTOTYPE_CONTRACT.into(),
            id: "house_gable".into(),
            version: 1,
            kind: PrototypeKind::Building,
            view: ViewKind::TopDown,
            size_tiers: vec![SizeTier { id: "m".into(), size: [48, 48] }],
            params: BTreeMap::from([
                ("width_units".into(), ParamRange::Int { min: 2, max: 4, default: 3 }),
                ("door_side".into(), ParamRange::Choice { options: vec!["left".into(), "right".into()], default: "left".into() }),
                ("has_chimney".into(), ParamRange::Flag { default: true }),
            ]),
            regions: vec!["roof".into(), "wall".into(), "door".into()],
            connectors: vec![Connector { id: "porch".into(), at: [24, 47], dir: "S".into(), width: 8, accepts: vec!["path".into()] }],
            footprint: Footprint { visual: [0, 0, 48, 48], sort_y: 47, collision: vec![[0, 24, 48, 24]], interact: Some([24, 47]) },
            layers: vec![LayerSplit { id: "roof_front".into(), regions: vec!["roof".into()], layer: DrawLayer::Front }],
            variants: vec![VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(true), region: "roof".into(), rows: vec!["..11".into()] }],
            source: PrototypeSource { kind: "ascii".into(), path: "house_gable.ascii".into(), digest: zero_digest() },
            requires: vec!["roof_tile".into()],
            license: "original".into(),
            review: ReviewState::Pending,
        }
    }

    fn roof_tile() -> SurfaceRecipe {
        SurfaceRecipe {
            contract: SURFACE_RECIPE_CONTRACT.into(),
            id: "roof_tile".into(),
            version: 1,
            material_id: "tile".into(),
            applies_to: vec!["roof".into()],
            recipe: "tile_rows".into(),
            params: BTreeMap::new(),
            scale_px: 3,
            license: "original".into(),
            review: ReviewState::Pending,
        }
    }

    fn instance(proto: &Prototype) -> AssetInstance {
        AssetInstance {
            contract: ASSET_INSTANCE_CONTRACT.into(),
            instance_id: "farm_house".into(),
            prototype: proto.reference(),
            size_tier: "m".into(),
            params: BTreeMap::from([("width_units".into(), ParamValue::Int(3))]),
            surfaces: BTreeMap::from([("roof".into(), "roof_tile".into())]),
            seed: 7,
            patches: Vec::new(),
        }
    }

    fn surfaces() -> BTreeMap<String, SurfaceRecipe> {
        BTreeMap::from([("roof_tile".into(), roof_tile())])
    }

    fn entry(id: &str, kind: EntryKind, requires: &[&str], review: ReviewState) -> LockedEntry {
        LockedEntry {
            id: id.into(),
            kind,
            version: 1,
            digest: sha256_digest(id.as_bytes()),
            requires: requires.iter().map(|s| s.to_string()).collect(),
            review,
        }
    }

    /// A serialized record must expose exactly the keys its JSON Schema requires,
    /// so the Rust types and the published contract cannot drift apart silently.
    fn assert_matches_schema(value: &serde_json::Value, schema_text: &str, what: &str) {
        let schema: serde_json::Value = serde_json::from_str(schema_text).expect("schema parses");
        let required: BTreeSet<&str> = schema["required"]
            .as_array()
            .expect("required list")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        let properties: BTreeSet<&str> = schema["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .map(String::as_str)
            .collect();
        let actual: BTreeSet<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
        assert_eq!(actual, required, "{what}: serialized keys differ from the schema's required keys");
        assert_eq!(properties, required, "{what}: schema has optional or extra properties");
        assert_eq!(schema["additionalProperties"], false, "{what}: schema must reject unknown fields");
    }

    #[test]
    fn rust_types_and_json_schemas_stay_in_step() {
        let proto = house();
        assert_matches_schema(
            &serde_json::to_value(&proto).unwrap(),
            include_str!("../../../steps/contracts/prototype.schema.json"),
            "prototype",
        );
        assert_matches_schema(
            &serde_json::to_value(roof_tile()).unwrap(),
            include_str!("../../../steps/contracts/surface-recipe.schema.json"),
            "surface-recipe",
        );
        assert_matches_schema(
            &serde_json::to_value(instance(&proto)).unwrap(),
            include_str!("../../../steps/contracts/asset-instance.schema.json"),
            "asset-instance",
        );
        let lock = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Approved)]);
        assert_matches_schema(
            &serde_json::to_value(&lock).unwrap(),
            include_str!("../../../steps/contracts/catalog-lock.schema.json"),
            "catalog-lock",
        );
    }

    #[test]
    fn enums_serialize_to_the_values_the_schemas_allow() {
        let proto = serde_json::to_value(house()).unwrap();
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../steps/contracts/prototype.schema.json")).unwrap();
        let allowed = |path: &[&str]| -> Vec<String> {
            let mut node = &schema["properties"];
            for key in path {
                node = &node[*key];
            }
            node["enum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
        };
        assert!(allowed(&["kind"]).contains(&proto["kind"].as_str().unwrap().to_string()));
        assert!(allowed(&["view"]).contains(&proto["view"].as_str().unwrap().to_string()));
        assert!(allowed(&["review"]).contains(&proto["review"].as_str().unwrap().to_string()));
        for kind in [PrototypeKind::Building, PrototypeKind::Plant, PrototypeKind::Crop, PrototypeKind::Terrain, PrototypeKind::Prop, PrototypeKind::Fence, PrototypeKind::SceneKit] {
            let text = serde_json::to_value(kind).unwrap();
            assert!(allowed(&["kind"]).contains(&text.as_str().unwrap().to_string()), "{text}");
        }
    }

    #[test]
    fn valid_records_pass() {
        let proto = house();
        assert_eq!(proto.validate(), Vec::<String>::new());
        assert_eq!(roof_tile().validate(), Vec::<String>::new());
        assert_eq!(instance(&proto).validate_against(&proto, &surfaces()), Vec::<String>::new());
    }

    #[test]
    fn digest_is_stable_and_changes_with_content() {
        let a = house();
        let mut b = house();
        assert_eq!(a.digest(), b.digest());
        b.version = 2;
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn invalid_prototype_is_rejected_with_reasons() {
        let mut proto = house();
        proto.size_tiers[0].size = [300, 48];
        proto.params.insert("width_units".into(), ParamRange::Int { min: 5, max: 2, default: 3 });
        proto.layers[0].regions = vec!["missing".into()];
        proto.connectors[0].dir = "Q".into();
        proto.license = String::new();
        let errors = proto.validate().join("\n");
        for needle in ["per edge", "min > max", "unknown region", "N/E/S/W", "license"] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }

    #[test]
    fn variant_rules_must_reference_real_params_regions_and_values() {
        let mut proto = house();
        proto.variants = vec![
            VariantRule { param: "colour".into(), equals: ParamValue::Flag(true), region: "roof".into(), rows: vec!["1".into()] },
            VariantRule { param: "door_side".into(), equals: ParamValue::Text("up".into()), region: "roof".into(), rows: vec!["1".into()] },
            VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(true), region: "moat".into(), rows: vec!["1".into()] },
            VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(false), region: "roof".into(), rows: Vec::new() },
        ];
        let errors = proto.validate().join("\n");
        for needle in ["unknown param 'colour'", "not one of", "unknown region 'moat'", "non-empty rows"] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }

    #[test]
    fn instance_param_and_reference_errors_are_reported() {
        let proto = house();
        let mut inst = instance(&proto);
        inst.params.insert("width_units".into(), ParamValue::Int(9));
        inst.params.insert("colour".into(), ParamValue::Text("red".into()));
        inst.params.insert("door_side".into(), ParamValue::Text("up".into()));
        inst.size_tier = "xl".into();
        inst.surfaces.insert("moat".into(), "roof_tile".into());
        inst.surfaces.insert("wall".into(), "no_such_surface".into());
        inst.patches.push(RegionPatch { region: "chimney".into(), rows: vec![".".into()] });
        let errors = inst.validate_against(&proto, &surfaces()).join("\n");
        for needle in ["outside 2..=4", "unknown param 'colour'", "not one of", "unknown size tier", "unknown region 'moat'", "not in the catalog", "patch targets unknown region"] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }

    #[test]
    fn stale_prototype_digest_is_detected() {
        let proto = house();
        let inst = instance(&proto);
        let mut changed = house();
        changed.footprint.sort_y = 40;
        let errors = inst.validate_against(&changed, &surfaces());
        assert!(errors.iter().any(|e| e.contains("digest does not match")), "{errors:?}");
    }

    #[test]
    fn catalog_lock_seal_ignores_input_order() {
        let a = vec![
            entry("house_gable", EntryKind::Prototype, &["roof_tile"], ReviewState::Approved),
            entry("roof_tile", EntryKind::Surface, &[], ReviewState::Approved),
        ];
        let mut b = a.clone();
        b.reverse();
        let (la, lb) = (CatalogLock::seal(a), CatalogLock::seal(b));
        assert_eq!(la.digest, lb.digest);
        assert_eq!(la.verify(true), Vec::<String>::new());
    }

    #[test]
    fn catalog_lock_rejects_missing_dependency_cycle_tamper_and_unapproved() {
        let missing = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &["ghost"], ReviewState::Approved)]);
        assert!(missing.verify(true).iter().any(|e| e.contains("missing entry 'ghost'")));

        let cycle = CatalogLock::seal(vec![
            entry("a", EntryKind::Prototype, &["b"], ReviewState::Approved),
            entry("b", EntryKind::Prototype, &["a"], ReviewState::Approved),
        ]);
        assert!(cycle.verify(true).iter().any(|e| e.contains("cycle")));

        let mut tampered = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Approved)]);
        tampered.entries[0].version = 9;
        assert!(tampered.verify(true).iter().any(|e| e.contains("digest does not match")));

        let pending = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Pending)]);
        assert!(pending.verify(true).iter().any(|e| e.contains("not approved")));
        assert_eq!(pending.verify(false), Vec::<String>::new());
    }
}
