//! Level files from the pxl pipeline (`400-package/levels/*.json`).
//!
//! The grid stores one tile id per character. `tile_semantics` decides what a
//! tile blocks; the renderer uses the tile id as the sheet id, so every tile a
//! level names must be delivered by the asset package.
//!
//! Battle City bricks take two hits: the first hit marks the cell as cracked
//! (drawn with `brick_crack`) and the second clears it. The crack lives in a
//! parallel array so the grid keeps the logical tile id `b`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// Tile ids a shell can destroy. Steel is handled separately because it needs
/// a fully starred player shell.
pub const BREAKABLE_TILES: &[&str] = &["b"];
pub const STEEL_TILES: &[&str] = &["x"];

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EnemyComposition {
    #[serde(default)]
    pub basic: u32,
    #[serde(default)]
    pub fast: u32,
    #[serde(default)]
    pub power: u32,
    #[serde(default)]
    pub armor: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelFile {
    #[serde(default)]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tile_size_px: u32,
    pub grid_w: u32,
    pub grid_h: u32,
    pub grid: Vec<String>,
    #[serde(default)]
    pub tile_semantics: Vec<TileSemantic>,
    #[serde(default)]
    pub placements: Vec<Placement>,
    pub spawn: [u32; 2],
    pub exit: [u32; 2],
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub difficulty: u32,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub enemy_composition: Option<EnemyComposition>,
    #[serde(default)]
    pub enemy_spawns: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TileSemantic {
    pub tile_id: String,
    pub behaviour: String,
    #[serde(default)]
    pub blocks: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Placement {
    pub entity_id: String,
    pub pos: [u32; 2],
    #[serde(default)]
    pub dir: Option<String>,
}

/// A validated, engine-shaped level.
#[derive(Debug, Clone)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub w: i32,
    pub h: i32,
    /// Row-major tile ids, `w * h` entries.
    pub tiles: Vec<String>,
    /// Whether a brick cell took its first hit, row-major like `tiles`.
    pub cracked: Vec<bool>,
    pub defs: BTreeMap<String, TileSemantic>,
    pub spawn: (i32, i32),
    pub exit: (i32, i32),
    /// Cell holding the eagle base, from `placements`.
    pub base: Option<(i32, i32)>,
    pub objective: String,
    pub difficulty: u32,
    pub seed: u64,
    /// Enemy counts in Basic, Fast, Power, Armor order.
    pub composition: [u32; 4],
    pub enemy_spawns: Vec<(i32, i32)>,
    pub notes: String,
}

impl Level {
    /// Parse and validate one level file. `known_assets` is the set of asset
    /// ids the package delivers; every non-floor tile must be in it.
    pub fn from_json(text: &str, known_assets: &BTreeSet<String>) -> Result<Self, String> {
        let file: LevelFile = serde_json::from_str(text).map_err(|e| format!("level json: {e}"))?;
        if file.grid.len() as u32 != file.grid_h {
            return Err(format!(
                "{}: grid has {} rows but grid_h is {}",
                file.id,
                file.grid.len(),
                file.grid_h
            ));
        }
        let mut defs = BTreeMap::new();
        for semantic in &file.tile_semantics {
            if semantic.tile_id.chars().count() != 1 {
                return Err(format!(
                    "{}: tile id '{}' must be a single character",
                    file.id, semantic.tile_id
                ));
            }
            defs.insert(semantic.tile_id.clone(), semantic.clone());
        }
        let mut tiles = Vec::with_capacity((file.grid_w * file.grid_h) as usize);
        for (y, row) in file.grid.iter().enumerate() {
            if row.chars().count() as u32 != file.grid_w {
                return Err(format!(
                    "{}: row {y} is {} wide but grid_w is {}",
                    file.id,
                    row.chars().count(),
                    file.grid_w
                ));
            }
            for ch in row.chars() {
                let id = ch.to_string();
                let Some(def) = defs.get(&id) else {
                    return Err(format!(
                        "{}: cell ({y}) tile '{id}' has no semantics",
                        file.id
                    ));
                };
                if def.behaviour != "floor" && !known_assets.contains(&id) {
                    return Err(format!(
                        "{}: tile '{id}' is not delivered by the asset package",
                        file.id
                    ));
                }
                tiles.push(id);
            }
        }
        let base = file
            .placements
            .iter()
            .find(|p| p.entity_id == "hq_base")
            .map(|p| (p.pos[0] as i32, p.pos[1] as i32));
        let composition = file
            .enemy_composition
            .as_ref()
            .map(|c| [c.basic, c.fast, c.power, c.armor])
            .unwrap_or([5, 5, 5, 5]);
        let enemy_spawns = if file.enemy_spawns.is_empty() {
            vec![(0, 0), (6, 0), (12, 0)]
        } else {
            file.enemy_spawns
                .iter()
                .map(|p| (p[0] as i32, p[1] as i32))
                .collect()
        };
        let level = Self {
            id: file.id,
            name: file.name,
            w: file.grid_w as i32,
            h: file.grid_h as i32,
            cracked: vec![false; tiles.len()],
            tiles,
            defs,
            spawn: (file.spawn[0] as i32, file.spawn[1] as i32),
            exit: (file.exit[0] as i32, file.exit[1] as i32),
            base,
            objective: file.objective,
            difficulty: file.difficulty,
            seed: file.seed,
            composition,
            enemy_spawns,
            notes: file.notes,
        };
        if !level.in_bounds(level.spawn.0, level.spawn.1) {
            return Err(format!(
                "{}: spawn {:?} is outside the grid",
                level.id, level.spawn
            ));
        }
        if level.blocks_tank_at(level.spawn.0, level.spawn.1) {
            return Err(format!("{}: spawn {:?} is blocked", level.id, level.spawn));
        }
        if let Some(base) = level.base {
            if !level.in_bounds(base.0, base.1) {
                return Err(format!("{}: base {base:?} is outside the grid", level.id));
            }
        }
        Ok(level)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    pub fn at(&self, x: i32, y: i32) -> &str {
        if !self.in_bounds(x, y) {
            return "f";
        }
        &self.tiles[(y * self.w + x) as usize]
    }

    pub fn set_at(&mut self, x: i32, y: i32, tile: &str) {
        if self.in_bounds(x, y) {
            let index = (y * self.w + x) as usize;
            self.tiles[index] = tile.to_string();
            if tile != "b" {
                self.cracked[index] = false;
            }
        }
    }

    /// Tank movement: out of bounds counts as blocked, `blocks` comes from the
    /// level's own semantics.
    pub fn blocks_tank_at(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return true;
        }
        let id = self.at(x, y);
        self.defs.get(id).map(|d| d.blocks).unwrap_or(false)
    }

    /// Shell movement: water, ice and grass let shells through, walls stop them.
    pub fn blocks_bullet_at(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return true;
        }
        let id = self.at(x, y);
        match self.defs.get(id) {
            Some(def) => def.behaviour == "wall",
            None => false,
        }
    }

    pub fn is_breakable_at(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && BREAKABLE_TILES.contains(&self.at(x, y))
    }

    pub fn is_steel_at(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && STEEL_TILES.contains(&self.at(x, y))
    }

    pub fn is_cracked_at(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        self.cracked[(y * self.w + x) as usize]
    }

    /// Two-stage brick damage. Returns true when the cell became floor.
    pub fn hit_brick(&mut self, x: i32, y: i32) -> bool {
        if !self.is_breakable_at(x, y) {
            return false;
        }
        let index = (y * self.w + x) as usize;
        if self.cracked[index] {
            self.cracked[index] = false;
            self.tiles[index] = "f".to_string();
            true
        } else {
            self.cracked[index] = true;
            false
        }
    }

    /// Grass is drawn above tanks; everything else is terrain.
    pub fn is_grass_at(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && self.at(x, y) == "g"
    }

    pub fn is_ice_at(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && self.at(x, y) == "i"
    }

    /// Brick/steel cells touching the HQ, used by the shovel power-up.
    pub fn wall_cells(&self) -> Vec<(i32, i32)> {
        let Some(base) = self.base else {
            return Vec::new();
        };
        let mut cells = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (base.0 + dx, base.1 + dy);
                if self.in_bounds(x, y) && self.blocks_tank_at(x, y) {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    pub fn cell_of(&self, x: f32, y: f32) -> (i32, i32) {
        ((x / crate::sim::TILE) as i32, (y / crate::sim::TILE) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: &str = r#"{
        "schema_version": 3, "id": "t", "name": "T", "tile_size_px": 32,
        "grid_w": 3, "grid_h": 3,
        "grid": ["fff", "fbf", "fgf"],
        "tile_semantics": [
            {"tile_id": "f", "behaviour": "floor", "blocks": false},
            {"tile_id": "b", "behaviour": "wall", "blocks": true},
            {"tile_id": "g", "behaviour": "decor", "blocks": false}
        ],
        "placements": [{"entity_id": "hq_base", "pos": [1, 2]}],
        "spawn": [0, 0], "exit": [2, 2], "difficulty": 1, "seed": 1,
        "enemy_composition": {"basic": 4, "fast": 0, "power": 0, "armor": 0}
    }"#;

    fn known() -> BTreeSet<String> {
        ["b", "g"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_and_validates() {
        let level = Level::from_json(LEVEL, &known()).unwrap();
        assert_eq!(level.base, Some((1, 2)));
        assert_eq!(level.composition, [4, 0, 0, 0]);
        assert!(level.blocks_tank_at(1, 1));
        assert!(!level.blocks_tank_at(1, 2));
        assert!(level.blocks_bullet_at(1, 1));
        assert!(!level.blocks_bullet_at(1, 2));
        assert!(level.is_breakable_at(1, 1));
        assert!(level.is_grass_at(1, 2));
    }

    #[test]
    fn brick_takes_two_hits() {
        let mut level = Level::from_json(LEVEL, &known()).unwrap();
        assert!(!level.hit_brick(1, 1));
        assert!(level.is_cracked_at(1, 1));
        assert!(level.blocks_tank_at(1, 1));
        assert!(level.hit_brick(1, 1));
        assert!(!level.blocks_tank_at(1, 1));
        assert_eq!(level.at(1, 1), "f");
    }

    #[test]
    fn default_composition_and_spawns() {
        let text = LEVEL.replace(
            r#",
        "enemy_composition": {"basic": 4, "fast": 0, "power": 0, "armor": 0}"#,
            "",
        );
        let level = Level::from_json(&text, &known()).unwrap();
        assert_eq!(level.composition, [5, 5, 5, 5]);
        assert_eq!(level.enemy_spawns.len(), 3);
    }

    #[test]
    fn rejects_undelivered_tiles() {
        let mut assets = known();
        assets.remove("b");
        let err = Level::from_json(LEVEL, &assets).unwrap_err();
        assert!(err.contains("not delivered"), "{err}");
    }

    #[test]
    fn rejects_ragged_rows() {
        let broken = LEVEL.replace("\"fff\"", "\"ffff\"");
        let err = Level::from_json(&broken, &known()).unwrap_err();
        assert!(err.contains("grid_w"), "{err}");
    }
}
