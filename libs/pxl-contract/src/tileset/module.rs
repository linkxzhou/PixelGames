// Tileset, module (metatile) and compiled-scene contracts (P05).
//
// Three layers, deliberately separate:
//   tileset/v1      — stable tile ids, sizes, families, 47-mask connection
//                     types and variant candidates. Holds no pixels.
//   module/v1       — a layout in tile cells. Members are tiles, nested
//                     modules, pixel-anchored decorations (optionally a
//                     scene-kit reference) and independent collision cells.
//   compiled-scene/v1 — the flattened result of expanding a module tree:
//                     resource ids, layers, sort keys, pixel positions and a
//                     collision map. Scenes reference compiled resources only;
//                     the runtime never executes a recipe.
//
// Expansion limits are enforced at compile time (depth, instance count, pixel
// budget) so a legal-but-exponential input fails closed instead of hanging.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::prototype::{sha256_digest, valid_digest, valid_id, ReviewState};


use super::mask::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub contract: String,
    pub id: String,
    pub version: u32,
    /// Module size in tile cells.
    pub size: [u32; 2],
    /// Terrain layers resolved through 47-mask coverage at compile time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terrain: Vec<TerrainLayer>,
    /// Hand-picked tiles; the author chose the exact tile, no mask involved.
    pub tiles: Vec<TileCell>,
    pub modules: Vec<ModuleCell>,
    pub decor: Vec<Decor>,
    /// Independent collision cells; changing decoration never touches these.
    pub collision: Vec<CollisionCell>,
    pub license: String,
    pub review: ReviewState,
}

impl Module {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("module serializes"))
    }

    pub fn requires(&self) -> Vec<String> {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        ids.extend(self.terrain.iter().map(|t| t.tileset.clone()));
        ids.extend(self.tiles.iter().map(|t| t.tileset.clone()));
        ids.extend(self.modules.iter().map(|m| m.module.clone()));
        ids.extend(self.decor.iter().filter_map(|d| d.scene_kit.clone()));
        ids.into_iter().collect()
    }

    /// Structural validation: ids, sizes, member placement, decor shape.
    /// Cross-record references (tileset existence, nested module existence)
    /// are checked by the compiler against the loaded library.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != MODULE_CONTRACT {
            errors.push(format!("module '{who}': contract must be {MODULE_CONTRACT}"));
        }
        if !valid_id(&self.id) {
            errors.push(format!("module '{who}': id must be a valid id"));
        }
        if self.version == 0 {
            errors.push(format!("module '{who}': version must be at least 1"));
        }
        if self.size.contains(&0) {
            errors.push(format!("module '{who}': size must be positive"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("module '{who}': license is required"));
        }
        let (w, h) = (self.size[0] as i32, self.size[1] as i32);
        let mut decor_ids = BTreeSet::new();
        for layer in &self.terrain {
            if layer.cells.is_empty() {
                errors.push(format!(
                    "module '{who}': terrain layer of '{}' has no cells",
                    layer.tileset
                ));
            }
            let mut seen = BTreeSet::new();
            for at in &layer.cells {
                if !seen.insert((at[0], at[1])) {
                    errors.push(format!(
                        "module '{who}': terrain layer of '{}' repeats cell {},{}",
                        layer.tileset, at[0], at[1]
                    ));
                }
                if at[0] < 0 || at[1] < 0 || at[0] >= w || at[1] >= h {
                    errors.push(format!(
                        "module '{who}': terrain cell {},{} of '{}' lies outside the {}x{} grid",
                        at[0], at[1], layer.tileset, w, h
                    ));
                }
            }
        }
        for tile in &self.tiles {
            if tile.at[0] < 0 || tile.at[1] < 0 || tile.at[0] >= w || tile.at[1] >= h {
                errors.push(format!(
                    "module '{who}': tile '{}' of '{}' lies outside the {}x{} grid",
                    tile.tile, tile.tileset, w, h
                ));
            }
        }
        for cell in &self.modules {
            if cell.module == self.id {
                errors.push(format!("module '{who}': cannot nest itself"));
            }
            if cell.at[0] < 0 || cell.at[1] < 0 {
                errors.push(format!(
                    "module '{who}': nested module '{}' has a negative offset",
                    cell.module
                ));
            }
        }
        for decor in &self.decor {
            if !decor_ids.insert(decor.id.as_str()) {
                errors.push(format!("module '{who}': duplicate decor '{}'", decor.id));
            }
            if !valid_id(&decor.id) {
                errors.push(format!("module '{who}': decor '{}' is not a valid id", decor.id));
            }
            match (&decor.resource_id, &decor.scene_kit) {
                (Some(_), Some(_)) => {
                    errors.push(format!(
                        "module '{who}': decor '{}' sets both resource_id and scene_kit",
                        decor.id
                    ));
                }
                (None, None) => {
                    errors.push(format!(
                        "module '{who}': decor '{}' needs a resource_id or a scene_kit",
                        decor.id
                    ));
                }
                _ => {}
            }
        }
        for cell in &self.collision {
            if cell.at[0] < 0 || cell.at[1] < 0 || cell.at[0] >= w || cell.at[1] >= h {
                errors.push(format!(
                    "module '{who}': collision cell at {},{} lies outside the {}x{} grid",
                    cell.at[0], cell.at[1], w, h
                ));
            }
            if cell.kind.trim().is_empty() {
                errors.push(format!(
                    "module '{who}': collision cell at {},{} needs a kind",
                    cell.at[0], cell.at[1]
                ));
            }
        }
        errors
    }
}

/// One expanded draw instance in a compiled scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneInstance {
    /// Stable id unique within the scene (`inst-…` or a readable name).
    pub id: String,
    /// Compiled resource id (`res-…`), never a recipe path.
    pub resource_id: String,
    /// Draw layer for z-ordering.
    pub layer: DecorLayer,
    /// Sort key: the world y row used to order instances.
    pub sort_y: i32,
    /// Pixel position in scene space.
    pub at: [i32; 2],
    /// Optional mirror; must survive the intent's facing policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledScene {
    pub contract: String,
    pub id: String,
    pub version: u32,
    /// Scene bounds in pixels.
    pub bounds: [u32; 2],
    /// Fully expanded instances, deterministic order.
    pub instances: Vec<SceneInstance>,
    /// Independent collision map: cell -> kind.
    pub collision: BTreeMap<String, String>,
    /// Seed the expansion froze; the same inputs and seed always
    /// produce byte-identical output.
    pub seed: u64,
    /// Digests of every module, tileset and kit this scene was built from,
    /// so a stale scene is detectable without re-expanding.
    pub depends: BTreeMap<String, String>,
    pub license: String,
    pub review: ReviewState,
}

impl CompiledScene {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("scene serializes"))
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != COMPILED_SCENE_CONTRACT {
            errors.push(format!("scene '{who}': contract must be {COMPILED_SCENE_CONTRACT}"));
        }
        if !valid_id(&self.id) {
            errors.push(format!("scene '{who}': id must be a valid id"));
        }
        if self.version == 0 {
            errors.push(format!("scene '{who}': version must be at least 1"));
        }
        if self.bounds.contains(&0) {
            errors.push(format!("scene '{who}': bounds must be positive"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("scene '{who}': license is required"));
        }
        let mut ids = BTreeSet::new();
        let (w, h) = (self.bounds[0] as i64, self.bounds[1] as i64);
        for inst in &self.instances {
            if !ids.insert(inst.id.as_str()) {
                errors.push(format!("scene '{who}': duplicate instance '{}'", inst.id));
            }
            if !inst.resource_id.starts_with("res-") {
                errors.push(format!(
                    "scene '{who}': instance '{}' does not reference a compiled resource id",
                    inst.id
                ));
            }
            if inst.at[0] < 0 || inst.at[1] < 0 || i64::from(inst.at[0]) >= w || i64::from(inst.at[1]) >= h {
                errors.push(format!(
                    "scene '{who}': instance '{}' at {},{} lies outside the {}x{} bounds",
                    inst.id, inst.at[0], inst.at[1], w, h
                ));
            }
        }
        for (cell, kind) in &self.collision {
            let (cx, cy) = match cell.split_once(',') {
                Some((x, y)) => (x.parse::<i64>().ok(), y.parse::<i64>().ok()),
                None => (None, None),
            };
            match (cx, cy) {
                (Some(x), Some(y)) => {
                    if x < 0 || y < 0 || x >= w || y >= h {
                        errors.push(format!("scene '{who}': collision cell '{cell}' lies outside the bounds"));
                    }
                }
                _ => errors.push(format!("scene '{who}': collision key '{cell}' is not 'x,y'")),
            }
            if kind.trim().is_empty() {
                errors.push(format!("scene '{who}': collision cell '{cell}' has an empty kind"));
            }
        }
        for (id, digest) in &self.depends {
            if !valid_id(id) || !valid_digest(digest) {
                errors.push(format!("scene '{who}': dependency '{id}' has an invalid id or digest"));
            }
        }
        errors
    }
}

