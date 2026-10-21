//! Paper-doll rigs. A part has a pivot, a parent attach point, and one sprite
//! per facing. Child pivot = parent top-left + attach. All coordinates are integers.

use std::collections::BTreeMap;

use pxl_core::{decode_matrix, PxlError, PxlResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn v1() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SpriteSrc {
    Rows(Vec<String>),
    /// Copy the sprite already declared for another facing (`"up": "down"`).
    Alias(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PartKey {
    #[serde(default)]
    pub dx: i32,
    #[serde(default)]
    pub dy: i32,
    #[serde(default)]
    pub rot90: Option<String>,
    #[serde(default)]
    pub flip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PartAnimDir {
    #[serde(default)]
    pub order: Vec<String>,
    pub frames: Vec<BTreeMap<String, PartKey>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PartAnim {
    /// Draw order for this action. Empty means the facing's order.
    #[serde(default)]
    pub order: Vec<String>,
    #[serde(default)]
    pub frames: Vec<BTreeMap<String, PartKey>>,
    #[serde(default)]
    pub dirs: BTreeMap<String, PartAnimDir>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PartDef {
    pub id: String,
    /// Pivot inside the part sprite, in pixels. Overridden per facing by `pivot_dir`.
    pub pivot: [i32; 2],
    #[serde(default)]
    pub pivot_dir: BTreeMap<String, [i32; 2]>,
    #[serde(default)]
    pub parent: Option<String>,
    /// Attach point in the parent sprite. The child's pivot lands here.
    #[serde(default)]
    pub attach: [i32; 2],
    #[serde(default)]
    pub attach_dir: BTreeMap<String, [i32; 2]>,
    /// Lower z is drawn first when a facing has no explicit order.
    #[serde(default)]
    pub z: i32,
    pub sprites: BTreeMap<String, SpriteSrc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PartsFile {
    #[serde(default = "v1")]
    pub v: u32,
    pub w: u16,
    pub h: u16,
    #[serde(default)]
    pub palette: Option<String>,
    /// Canvas pixel where the root part's pivot sits.
    pub anchor: [i32; 2],
    pub root: String,
    /// `right` → `left` means draw the left rig and flip the canvas.
    #[serde(default)]
    pub mirror: BTreeMap<String, String>,
    /// Facing → back-to-front part ids.
    #[serde(default)]
    pub order: BTreeMap<String, Vec<String>>,
    pub parts: Vec<PartDef>,
    #[serde(default)]
    pub anims: BTreeMap<String, PartAnim>,
}

impl PartsFile {
    pub fn part(&self, id: &str) -> PxlResult<&PartDef> {
        self.parts.iter().find(|p| p.id == id).ok_or_else(|| {
            PxlError::semantic(format!("unknown part '{id}'"))
                .with_hint("part ids come from the parts spec")
        })
    }

    pub fn sprite_dir<'a>(&'a self, dir: &'a str) -> &'a str {
        self.mirror.get(dir).map(|s| s.as_str()).unwrap_or(dir)
    }

    pub fn order_for(&self, dir: &str) -> Vec<String> {
        let key = self.sprite_dir(dir);
        if let Some(order) = self.order.get(key) {
            return order.clone();
        }
        let mut ids: Vec<(i32, String)> = self.parts.iter().map(|p| (p.z, p.id.clone())).collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        ids.into_iter().map(|(_, id)| id).collect()
    }

    pub fn pose_dir<'a>(&'a self, dir: Option<&'a str>) -> &'a str {
        let dir = dir.unwrap_or("down");
        self.mirror.get(dir).map(|s| s.as_str()).unwrap_or(dir)
    }

    /// Frames and optional draw-order override for `doll(anim=...)`.
    pub fn anim_pose(
        &self,
        name: &str,
        dir: &str,
    ) -> PxlResult<(Vec<BTreeMap<String, PartKey>>, Vec<String>)> {
        let anim = self.anims.get(name).ok_or_else(|| {
            PxlError::semantic(format!("parts spec has no anim '{name}'"))
                .with_hint("anims in the parts file are walk, idle, hoe, ...")
        })?;
        let pose_dir = self.pose_dir(Some(dir));
        if let Some(specific) = anim.dirs.get(pose_dir) {
            let order = if !specific.order.is_empty() {
                specific.order.clone()
            } else {
                anim.order.clone()
            };
            return Ok((specific.frames.clone(), order));
        }
        if anim.frames.is_empty() {
            return Err(PxlError::semantic(format!(
                "anim '{name}' has no frames for '{pose_dir}'"
            ))
            .with_hint("add frames or a dirs entry in the parts spec"));
        }
        Ok((anim.frames.clone(), anim.order.clone()))
    }
}

impl PartDef {
    pub fn pivot_for(&self, sprite_dir: &str) -> [i32; 2] {
        self.pivot_dir
            .get(sprite_dir)
            .copied()
            .unwrap_or(self.pivot)
    }

    pub fn attach_for(&self, sprite_dir: &str) -> [i32; 2] {
        self.attach_dir
            .get(sprite_dir)
            .copied()
            .unwrap_or(self.attach)
    }

    pub fn rows(&self, dir: &str) -> PxlResult<Vec<String>> {
        self.resolve_rows(dir, 0)
    }

    fn resolve_rows(&self, dir: &str, depth: u8) -> PxlResult<Vec<String>> {
        if depth > 4 {
            return Err(PxlError::schema(format!(
                "part '{}' sprite alias for '{dir}' is cyclic",
                self.id
            )));
        }
        match self.sprites.get(dir) {
            Some(SpriteSrc::Rows(rows)) => Ok(rows.clone()),
            Some(SpriteSrc::Alias(other)) => self.resolve_rows(other, depth + 1),
            None => Err(PxlError::semantic(format!(
                "part '{}' has no sprite for facing '{dir}'",
                self.id
            ))
            .with_hint("add a sprite, an alias, or a mirror entry for that facing")),
        }
    }
}

pub fn parse_parts(text: &str) -> PxlResult<PartsFile> {
    let file: PartsFile = crate::parse_json(text, "parts")?;
    if file.w == 0 || file.h == 0 || file.w > 64 || file.h > 64 {
        return Err(PxlError::schema(format!(
            "parts canvas {}x{} is outside 1..=64",
            file.w, file.h
        )));
    }
    if file.part(&file.root).is_err() {
        return Err(PxlError::schema(format!(
            "parts root '{}' is not a part id",
            file.root
        )));
    }
    let mut seen = BTreeMap::<String, ()>::new();
    for part in &file.parts {
        if seen.insert(part.id.clone(), ()).is_some() {
            return Err(PxlError::schema(format!("duplicate part '{}'", part.id)));
        }
        if let Some(parent) = &part.parent {
            if file.part(parent).is_err() {
                return Err(PxlError::schema(format!(
                    "part '{}' parent '{parent}' does not exist",
                    part.id
                )));
            }
        }
        for dir in part.sprites.keys() {
            let rows = part.rows(dir)?;
            if rows.is_empty() {
                return Err(PxlError::schema(format!(
                    "part '{}' facing '{dir}' has no rows",
                    part.id
                )));
            }
            decode_matrix(&rows).map_err(|e| {
                e.with_hint(format!(
                    "part '{}' facing '{dir}' rows must be equal width, '.' or 0-9a-z",
                    part.id
                ))
            })?;
            let pivot = part.pivot_for(dir);
            let w = rows[0].chars().count() as i32;
            let h = rows.len() as i32;
            if pivot[0] < 0 || pivot[1] < 0 || pivot[0] >= w || pivot[1] >= h {
                return Err(PxlError::schema(format!(
                    "part '{}' pivot {:?} is outside the {w}x{h} '{dir}' sprite",
                    part.id, pivot
                )));
            }
        }
    }
    Ok(file)
}
