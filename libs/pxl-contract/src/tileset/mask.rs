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
