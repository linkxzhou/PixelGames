//! Approved design levels. The JSON is the design revision, copied into the
//! runtime so a headless replay does not need the harness tree at runtime.

use serde::Deserialize;

use crate::sim::Dir;

pub const GRID: i32 = 12;

#[derive(Clone, Debug)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub grid: Vec<Vec<char>>,
    pub spawn: (i32, i32),
    pub exit: (i32, i32),
    pub hq: Option<(i32, i32)>,
    pub enemies: Vec<(i32, i32, Dir)>,
    pub seed: u64,
}

#[derive(Deserialize)]
struct LevelFile {
    id: String,
    name: String,
    grid: Vec<String>,
    spawn: [i32; 2],
    exit: [i32; 2],
    #[serde(default)]
    placements: Vec<Placement>,
    #[serde(default)]
    seed: u64,
}

#[derive(Deserialize)]
struct Placement {
    entity_id: String,
    pos: [i32; 2],
    #[serde(default)]
    dir: Option<String>,
}

impl Level {
    pub fn bundled(index: u32) -> Result<Self, String> {
        let text = match index {
            1 => include_str!("../data/levels/level_1.json"),
            2 => include_str!("../data/levels/level_2.json"),
            3 => include_str!("../data/levels/level_3.json"),
            other => return Err(format!("level {other} is not in the slice (1-3)")),
        };
        Self::parse(text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let file: LevelFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if file.grid.len() != GRID as usize {
            return Err(format!("{} grid height {}", file.id, file.grid.len()));
        }
        let mut grid = Vec::with_capacity(GRID as usize);
        for (y, row) in file.grid.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            if chars.len() != GRID as usize {
                return Err(format!("{} row {y} width {}", file.id, chars.len()));
            }
            grid.push(chars);
        }
        let mut hq = None;
        let mut enemies = Vec::new();
        for place in &file.placements {
            let pos = (place.pos[0], place.pos[1]);
            match place.entity_id.as_str() {
                "hq_base" => hq = Some(pos),
                "enemy_tank" => enemies.push((pos.0, pos.1, Dir::parse(place.dir.as_deref().unwrap_or("N")))),
                _ => {}
            }
        }
        Ok(Self {
            id: file.id,
            name: file.name,
            grid,
            spawn: (file.spawn[0], file.spawn[1]),
            exit: (file.exit[0], file.exit[1]),
            hq,
            enemies,
            seed: file.seed,
        })
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < GRID && y < GRID
    }

    pub fn tile(&self, x: i32, y: i32) -> char {
        if !self.in_bounds(x, y) {
            return '#';
        }
        self.grid[y as usize][x as usize]
    }

    pub fn set_tile(&mut self, x: i32, y: i32, tile: char) {
        if self.in_bounds(x, y) {
            self.grid[y as usize][x as usize] = tile;
        }
    }

    /// Tanks cannot enter brick, steel, or water. Bullets are separate.
    pub fn blocks_tank(&self, x: i32, y: i32) -> bool {
        matches!(self.tile(x, y), 'b' | 'x' | 'w' | '#')
    }

    pub fn stops_bullet(&self, x: i32, y: i32) -> bool {
        matches!(self.tile(x, y), 'b' | 'x' | '#')
    }
}
