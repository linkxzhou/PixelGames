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

pub const TILESET_CONTRACT: &str = "tileset/v1";
pub const MODULE_CONTRACT: &str = "module/v1";
pub const COMPILED_SCENE_CONTRACT: &str = "compiled-scene/v1";

/// Capability id announced when a build emits compiled scenes.
pub const COMPILED_SCENE_CAPABILITY: &str = "compiled-scene/v1";

/// Hard nesting limits for module expansion. Fail closed before allocating.
pub const MAX_MODULE_DEPTH: u32 = 8;
pub const MAX_EXPANDED_INSTANCES: usize = 100_000;
/// Upper bound on expanded draw pixels (sum of member canvas areas).
pub const MAX_EXPANDED_PIXELS: u64 = 64 * 1024 * 1024;

/// The 47 reduced-neighbour masks, matching `scene-select.js` bit for bit:
/// N=1 E=2 S=4 W=8 NE=16 SE=32 SW=64 NW=128; a diagonal bit only counts when
/// both orthogonal neighbours beside it are set.
pub const MASK_N: u8 = 1;
pub const MASK_E: u8 = 2;
pub const MASK_S: u8 = 4;
pub const MASK_W: u8 = 8;
pub const MASK_NE: u8 = 16;
pub const MASK_SE: u8 = 32;
pub const MASK_SW: u8 = 64;
pub const MASK_NW: u8 = 128;

/// Reduce an 8-bit raw neighbourhood to the canonical mask: diagonal bits
/// survive only when both their orthogonal neighbours are set. This is the
/// Rust twin of `neighbourMask`/`reachableMasks` in `scene-select.js`;
/// fixtures keep the two implementations honest.
pub fn reduce_mask(raw: u8) -> u8 {
    let (n, e, s, w) = (raw & MASK_N != 0, raw & MASK_E != 0, raw & MASK_S != 0, raw & MASK_W != 0);
    let mut mask = raw & 0x0f;
    if raw & MASK_NE != 0 && n && e {
        mask |= MASK_NE;
    }
    if raw & MASK_SE != 0 && s && e {
        mask |= MASK_SE;
    }
    if raw & MASK_SW != 0 && s && w {
        mask |= MASK_SW;
    }
    if raw & MASK_NW != 0 && n && w {
        mask |= MASK_NW;
    }
    mask
}

/// The 47 distinct masks reachable after diagonal reduction, ascending.
pub fn reachable_masks() -> Vec<u8> {
    let mut seen = BTreeSet::new();
    for bits in 0..=255u8 {
        seen.insert(reduce_mask(bits));
    }
    seen.into_iter().collect()
}

/// Human-readable mask shape for diagnostics (`describeMask` twin).
pub fn describe_mask(mask: u8) -> String {
    let mut edges = Vec::new();
    if mask & MASK_N != 0 {
        edges.push("N");
    }
    if mask & MASK_E != 0 {
        edges.push("E");
    }
    if mask & MASK_S != 0 {
        edges.push("S");
    }
    if mask & MASK_W != 0 {
        edges.push("W");
    }
    let mut corners = Vec::new();
    if mask & MASK_NE != 0 {
        corners.push("NE");
    }
    if mask & MASK_SE != 0 {
        corners.push("SE");
    }
    if mask & MASK_SW != 0 {
        corners.push("SW");
    }
    if mask & MASK_NW != 0 {
        corners.push("NW");
    }
    let edge_text = if edges.is_empty() { "-".to_string() } else { edges.join("") };
    if corners.is_empty() {
        edge_text
    } else {
        format!("{edge_text}+{}", corners.join(","))
    }
}

/// One tile of a family: what it draws and which connection masks it covers.
/// `exact_masks` are masks the art shows exactly; `approximate_masks` are
/// explicit, reported approximations. A mask in neither list is a gap the
/// compiler must surface, never a silent default tile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileDef {
    pub id: String,
    /// Resource id of the compiled sprite this tile draws (`res-…`).
    pub resource_id: String,
    /// Masks this tile renders exactly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exact_masks: Vec<u8>,
    /// Masks this tile only approximates; reported separately, never exact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approximate_masks: Vec<u8>,
    /// Variant candidates the deterministic picker may choose from.
    /// Empty means this tile has a single look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tileset {
    pub contract: String,
    pub id: String,
    pub version: u32,
    /// Palette family this whole tileset belongs to.
    pub palette_family: String,
    /// Tile edge length in pixels; tiles are square.
    pub tile_size: u16,
    pub tiles: Vec<TileDef>,
    pub license: String,
    pub review: ReviewState,
}

impl Tileset {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("tileset serializes"))
    }

    /// Every mask of the 47, mapped to the tile that covers it. Returns
    /// (exact, approximated, missing) tile ids per mask so the three cases
    /// are always reported separately.
    pub fn mask_coverage(&self) -> (BTreeMap<u8, String>, BTreeMap<u8, String>, Vec<u8>) {
        let mut exact = BTreeMap::new();
        let mut approximated = BTreeMap::new();
        let mut missing = Vec::new();
        for mask in reachable_masks() {
            if let Some(tile) = self.tiles.iter().find(|t| t.exact_masks.contains(&mask)) {
                exact.insert(mask, tile.id.clone());
            } else if let Some(tile) = self.tiles.iter().find(|t| t.approximate_masks.contains(&mask)) {
                approximated.insert(mask, tile.id.clone());
            } else {
                missing.push(mask);
            }
        }
        (exact, approximated, missing)
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != TILESET_CONTRACT {
            errors.push(format!("tileset '{who}': contract must be {TILESET_CONTRACT}"));
        }
        if !valid_id(&self.id) || !valid_id(&self.palette_family) {
            errors.push(format!("tileset '{who}': id and palette_family must be valid ids"));
        }
        if self.version == 0 {
            errors.push(format!("tileset '{who}': version must be at least 1"));
        }
        if self.tile_size == 0 {
            errors.push(format!("tileset '{who}': tile_size must be positive"));
        }
        if self.tiles.is_empty() {
            errors.push(format!("tileset '{who}': at least one tile is required"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("tileset '{who}': license is required"));
        }
        let mut tile_ids = BTreeSet::new();
        let reachable: BTreeSet<u8> = reachable_masks().into_iter().collect();
        for tile in &self.tiles {
            if !tile_ids.insert(tile.id.as_str()) {
                errors.push(format!("tileset '{who}': duplicate tile '{}'", tile.id));
            }
            if tile.resource_id.trim().is_empty() {
                errors.push(format!("tileset '{who}': tile '{}' needs a resource_id", tile.id));
            }
            let mut seen = BTreeSet::new();
            for mask in tile.exact_masks.iter().chain(&tile.approximate_masks) {
                if !seen.insert(mask) {
                    errors.push(format!(
                        "tileset '{who}': tile '{}' lists mask {} twice",
                        tile.id,
                        describe_mask(*mask)
                    ));
                }
                if !reachable.contains(mask) {
                    errors.push(format!(
                        "tileset '{who}': tile '{}' claims unreachable mask {} ({})",
                        tile.id,
                        describe_mask(*mask),
                        mask
                    ));
                }
            }
            if !tile.exact_masks.is_empty()
                && tile.approximate_masks.iter().any(|m| tile.exact_masks.contains(m))
            {
                errors.push(format!(
                    "tileset '{who}': tile '{}' marks the same mask exact and approximate",
                    tile.id
                ));
            }
        }
        errors
    }
}

/// One tile placed at a cell of a module grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileCell {
    pub tileset: String,
    pub tile: String,
    /// Cell coordinates in this module's grid.
    pub at: [i32; 2],
}

/// A nested module placed inside another module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleCell {
    pub module: String,
    pub at: [i32; 2],
    /// Variant choice for this placement; deterministic once fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// A pixel-anchored decoration. Either a compiled resource (`resource_id`) or
/// a scene-kit reference (`scene_kit`); exactly one of the two.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decor {
    pub id: String,
    /// Pixel offset from the module origin.
    pub at: [i32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    /// References a scene-kit/v1 record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_kit: Option<String>,
    /// Draw layer for this decoration.
    pub layer: DecorLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorLayer {
    Back,
    Front,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionCell {
    /// Cell coordinates; collision is independent of tiles and decorations.
    pub at: [i32; 2],
    /// Collision semantics (solid, water, stairs…); free-form id.
    pub kind: String,
}

/// A terrain layer: cells of one material inside a module grid. The compiler
/// computes each cell's 47-mask from same-layer neighbours and resolves the
/// tile through the tileset's coverage — exact first, explicit approximation
/// second, and a gap is always an error, never a default tile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainLayer {
    pub tileset: String,
    /// Cells of this layer, in module grid coordinates.
    pub cells: Vec<[i32; 2]>,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn tileset() -> Tileset {
        let fill = TileDef {
            id: "grass_fill".into(),
            resource_id: "res-environment-grass-fill".into(),
            exact_masks: vec![255],
            approximate_masks: vec![],
            variants: vec![],
        };
        Tileset {
            contract: TILESET_CONTRACT.into(),
            id: "terrain_grass".into(),
            version: 1,
            palette_family: "environment".into(),
            tile_size: 32,
            tiles: vec![fill],
            license: "original".into(),
            review: ReviewState::Pending,
        }
    }

    fn module() -> Module {
        Module {
            contract: MODULE_CONTRACT.into(),
            id: "landmark_plaza".into(),
            version: 1,
            size: [4, 4],
            terrain: vec![TerrainLayer { tileset: "terrain_grass".into(), cells: vec![[0, 0]] }],
            tiles: vec![TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] }],
            modules: vec![],
            decor: vec![Decor {
                id: "fountain".into(),
                at: [64, 32],
                resource_id: Some("res-environment-fountain".into()),
                scene_kit: None,
                layer: DecorLayer::Front,
            }],
            collision: vec![CollisionCell { at: [2, 2], kind: "solid".into() }],
            license: "original".into(),
            review: ReviewState::Pending,
        }
    }

    #[test]
    fn the_47_reduced_masks_match_the_frozen_count_and_bit_order() {
        let masks = reachable_masks();
        assert_eq!(masks.len(), 47, "the diagonal reduction yields exactly 47 masks");
        assert!(masks.windows(2).all(|w| w[0] < w[1]));
        // A diagonal without both orthogonal neighbours is reduced away.
        assert_eq!(reduce_mask(MASK_NE), 0);
        assert_eq!(reduce_mask(MASK_N | MASK_NE), MASK_N);
        assert_eq!(reduce_mask(MASK_N | MASK_E | MASK_NE), MASK_N | MASK_E | MASK_NE);
        // The full 8-neighbourhood survives untouched.
        assert_eq!(reduce_mask(0xff), 0xff);
        assert_eq!(describe_mask(0), "-");
        assert_eq!(describe_mask(MASK_N | MASK_E | MASK_NE), "NE+NE");
    }

    #[test]
    fn a_tileset_reports_exact_approximate_and_missing_masks_separately() {
        let mut set = tileset();
        set.tiles[0].exact_masks = vec![255];
        set.tiles.push(TileDef {
            id: "grass_edge_n".into(),
            resource_id: "res-environment-grass-edge-n".into(),
            exact_masks: vec![MASK_E | MASK_S | MASK_SE | MASK_SW | MASK_W],
            approximate_masks: vec![MASK_S | MASK_SW | MASK_W],
            variants: vec!["a".into()],
        });
        assert!(set.validate().is_empty(), "{:?}", set.validate());
        let (exact, approximated, missing) = set.mask_coverage();
        assert!(exact.contains_key(&255));
        assert!(approximated.contains_key(&(MASK_S | MASK_SW | MASK_W)));
        assert!(missing.len() > 40, "most of the 47 masks are gaps: {:?}", missing.len());
    }

    #[test]
    fn tilesets_reject_bad_ids_unreachable_masks_and_double_claiming() {
        let mut set = tileset();
        set.tiles[0].exact_masks = vec![255, 200]; // NE bit alone is unreachable
        set.tiles[0].approximate_masks = vec![255]; // also claimed as approximate
        let errors = set.validate().join("\n");
        for needle in ["unreachable mask", "exact and approximate", "lists mask"] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
        assert!(!errors.contains("duplicate tile"));
        let mut dupe = tileset();
        dupe.tiles.push(dupe.tiles[0].clone());
        assert!(dupe.validate().iter().any(|e| e.contains("duplicate tile")));
    }

    #[test]
    fn modules_validate_structure_but_check_references_at_compile_time() {
        let m = module();
        assert!(m.validate().is_empty(), "{:?}", m.validate());
        assert_eq!(m.requires(), vec!["terrain_grass".to_string()]);

        let mut bad = module();
        bad.size = [0, 4];
        bad.tiles[0].at = [9, 9];
        bad.terrain.push(TerrainLayer { tileset: "terrain_grass".into(), cells: vec![[9, 9]] });
        bad.modules.push(ModuleCell { module: "landmark_plaza".into(), at: [-1, 0], variant: None });
        bad.decor.push(Decor {
            id: "fountain".into(),
            at: [0, 0],
            resource_id: None,
            scene_kit: None,
            layer: DecorLayer::Back,
        });
        bad.collision[0].at = [8, 8];
        let errors = bad.validate().join("\n");
        for needle in [
            "size must be positive",
            "outside the 0x4 grid",
            "terrain cell 9,9",
            "cannot nest itself",
            "negative offset",
            "duplicate decor",
            "needs a resource_id or a scene_kit",
        ] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }

    #[test]
    fn compiled_scenes_reject_duplicate_ids_out_of_bounds_and_bad_keys() {
        let scene = CompiledScene {
            contract: COMPILED_SCENE_CONTRACT.into(),
            id: "scene_plaza".into(),
            version: 1,
            bounds: [128, 128],
            instances: vec![SceneInstance {
                id: "inst-0001".into(),
                resource_id: "res-environment-grass-fill".into(),
                layer: DecorLayer::Back,
                sort_y: 32,
                at: [0, 0],
                mirror: None,
            }],
            collision: BTreeMap::from([("2,2".into(), "solid".into())]),
            seed: 7,
            depends: BTreeMap::from([("terrain_grass".into(), sha256_digest(b"tileset"))]),
            license: "original".into(),
            review: ReviewState::Pending,
        };
        assert!(scene.validate().is_empty(), "{:?}", scene.validate());
        assert_eq!(scene.digest(), scene.digest());

        let mut bad = scene.clone();
        bad.instances[0].resource_id = "terrain_grass".into();
        bad.instances[0].at = [200, 0];
        bad.instances.push(bad.instances[0].clone());
        bad.collision.insert("oops".into(), " ".into());
        let errors = bad.validate().join("\n");
        for needle in [
            "does not reference a compiled resource id",
            "outside the 128x128 bounds",
            "duplicate instance",
            "not 'x,y'",
            "empty kind",
        ] {
            assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
        }
    }
}
