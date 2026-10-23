//! Design package contracts: brief, game design, style guide, tuning,
//! levels, asset requirements, acceptance plan, design review.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{stable_id, ContractHeader};

pub const DESIGN_SCHEMA_VERSION: u32 = 1;

/// Raw user intent snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brief {
    #[serde(flatten)]
    pub header: ContractHeader,
    pub game_id: String,
    /// Free-text creative brief.
    pub text: String,
    /// Constraints the user stated explicitly (source = "user").
    #[serde(default)]
    pub constraints: Vec<BriefItem>,
    /// Defaults applied because the user did not say (source = "default").
    #[serde(default)]
    pub assumptions: Vec<BriefItem>,
    /// Explicitly out of scope.
    #[serde(default)]
    pub exclusions: Vec<BriefItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefItem {
    pub key: String,
    pub value: String,
    pub source: BriefSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefSource {
    User,
    Default,
    Excluded,
}

/// Structured gameplay facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDesign {
    #[serde(flatten)]
    pub header: ContractHeader,
    pub game_id: String,
    pub genre: String,
    /// One-sentence core loop, e.g. "navigate, shoot, survive, reach exit".
    pub core_loop: String,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub entities: Vec<EntityDef>,
    #[serde(default)]
    pub rules: Vec<RuleDef>,
    #[serde(default)]
    pub win_condition: Option<WinLose>,
    #[serde(default)]
    pub lose_condition: Option<WinLose>,
    /// Requirement IDs this design covers (for acceptance coverage).
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    #[serde(default)]
    pub scope: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub id: String,
    pub name: String,
    /// Input binding, e.g. "arrow/WASD", "space".
    pub input: String,
    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub id: String,
    pub kind: EntityKind,
    #[serde(default)]
    pub desc: String,
    /// Actions this entity can perform or react to.
    #[serde(default)]
    pub actions: Vec<String>,
    /// Movement speed in tiles/sec if it moves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Player,
    Enemy,
    Bullet,
    Prop,
    Pickup,
    Goal,
    Ui,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDef {
    pub id: String,
    pub statement: String,
    /// Entities this rule references (must close).
    #[serde(default)]
    pub entities: Vec<String>,
    /// Actions this rule references (must close).
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinLose {
    pub id: String,
    pub statement: String,
    /// Entity/state references that must exist.
    #[serde(default)]
    pub refs: Vec<String>,
}

/// Visual production constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleGuide {
    #[serde(flatten)]
    pub header: ContractHeader,
    /// Logical resolution of one screen.
    pub logical_resolution: [u32; 2],
    pub tile_size_px: u32,
    pub sprite_size_px: u32,
    /// Palette file path relative to the design package.
    pub palette: String,
    /// Transparent palette index.
    pub transparent_index: u8,
    /// Outline policy: "always", "never", "soft".
    pub outline: String,
    /// Facing direction names required by gameplay (subset of N,E,S,W,...).
    pub directions: Vec<String>,
    /// Animation constraints: fps range.
    pub anim_fps_min: u32,
    pub anim_fps_max: u32,
    /// Nearest-neighbour filtering required by the renderer.
    pub nearest_filtering: bool,
}

/// Numeric tuning parameters referenced by rule IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tuning {
    #[serde(flatten)]
    pub header: ContractHeader,
    #[serde(default)]
    pub params: Vec<TuningParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningParam {
    pub id: String,
    pub unit: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    /// Rule IDs that use this parameter.
    #[serde(default)]
    pub rule_refs: Vec<String>,
}

/// Engine-independent logical map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    #[serde(flatten)]
    pub header: ContractHeader,
    pub id: String,
    pub name: String,
    pub tile_size_px: u32,
    pub grid_w: u32,
    pub grid_h: u32,
    /// Rows of logical tile IDs. Length must equal grid_h, each row grid_w.
    pub grid: Vec<String>,
    /// Tile semantics declared on this map (id -> behaviour).
    #[serde(default)]
    pub tile_semantics: Vec<TileSemantic>,
    #[serde(default)]
    pub placements: Vec<Placement>,
    pub spawn: [u32; 2],
    pub exit: [u32; 2],
    /// Objective description + referenced requirement.
    #[serde(default)]
    pub objective: String,
    pub difficulty: u32,
    pub seed: u64,
    /// Design notes: teaching goal, challenge composition.
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileSemantic {
    pub tile_id: String,
    pub behaviour: TileBehaviour,
    #[serde(default)]
    pub blocks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileBehaviour {
    Floor,
    Wall,
    Hazard,
    Water,
    GoalPad,
    Decor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub entity_id: String,
    pub pos: [u32; 2],
    /// Optional facing for entities with directions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Optional patrol path for enemies (must stay walkable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<[u32; 2]>,
}

/// Asset requirement: the single input to the assets stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRequirements {
    #[serde(flatten)]
    pub header: ContractHeader,
    #[serde(default)]
    pub requirements: Vec<AssetRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRequirement {
    pub asset_id: String,
    pub kind: AssetKind,
    pub required: bool,
    /// Entities / tiles / levels that consume this asset.
    #[serde(default)]
    pub consumer_refs: Vec<String>,
    pub style_ref: String,
    pub size_px: [u32; 2],
    #[serde(default)]
    pub directions: Vec<String>,
    #[serde(default)]
    pub anims: Vec<AnimRequirement>,
    /// Gameplay semantics (e.g. "blocks", "player", "goal"); never guessed
    /// from colours.
    #[serde(default)]
    pub semantics: String,
    #[serde(default)]
    pub strategy: AssetStrategy,
    /// Tile assets that must tile seamlessly.
    #[serde(default)]
    pub seamless: bool,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimRequirement {
    pub name: String,
    pub fps: u32,
    pub looped: bool,
    /// Frames per direction (>=1).
    pub frames: u32,
    /// Direction names when the anim is directional.
    #[serde(default)]
    pub dirs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Sprite,
    Tile,
    Ui,
    Effect,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStrategy {
    /// Bollinger mask template: a pxl built-in id or an authored
    /// `{id}.template.json`.
    #[default]
    Template,
    /// Generate from a shapes spec.
    Shapes,
    /// Noise tile generator (dirt/grass/sand/ore) via recipe `tiles[]`.
    TileGen,
    /// Parts/paper-doll rig.
    Parts,
    /// Not producible with current pxl: external dependency.
    External,
}

/// Acceptance plan: requirement -> gate -> stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptancePlan {
    #[serde(flatten)]
    pub header: ContractHeader,
    #[serde(default)]
    pub items: Vec<AcceptanceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceItem {
    pub requirement_id: String,
    pub description: String,
    /// Gate or check id that verifies it.
    pub gate_id: String,
    pub stage: Stage,
    /// Evidence type: "report", "checks", "manifest", "human".
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Design,
    Assets,
    Engine,
    Playtest,
    Human,
}

/// Design review bundle: hard gate results, soft review notes, approvals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignReview {
    #[serde(flatten)]
    pub header: ContractHeader,
    #[serde(default)]
    pub hard_gate_results: Vec<GateResult>,
    #[serde(default)]
    pub soft_review: Vec<SoftNote>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub pending_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub gate_id: String,
    pub passed: bool,
    #[serde(default)]
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftNote {
    pub topic: String,
    pub note: String,
}

pub fn validate_ids(g: &GameDesign) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    for a in &g.actions {
        if !stable_id(&a.id) {
            errs.push(format!("action id '{}' is not a stable id", a.id));
        }
    }
    for e in &g.entities {
        if !stable_id(&e.id) {
            errs.push(format!("entity id '{}' is not a stable id", e.id));
        }
    }
    let entity_ids: Vec<&String> = g.entities.iter().map(|e| &e.id).collect();
    let action_ids: Vec<&String> = g.actions.iter().map(|a| &a.id).collect();
    for r in &g.rules {
        if !stable_id(&r.id) {
            errs.push(format!("rule id '{}' is not a stable id", r.id));
        }
        for e in &r.entities {
            if !entity_ids.contains(&e) {
                errs.push(format!("rule '{}' references unknown entity '{}'", r.id, e));
            }
        }
        for a in &r.actions {
            if !action_ids.contains(&a) {
                errs.push(format!("rule '{}' references unknown action '{}'", r.id, a));
            }
        }
    }
    for (name, cond) in [
        ("win_condition", &g.win_condition),
        ("lose_condition", &g.lose_condition),
    ] {
        if let Some(c) = cond {
            for r in &c.refs {
                if !entity_ids.contains(&r) && !action_ids.contains(&r) {
                    errs.push(format!("{name} '{}' references unknown '{}'", c.id, r));
                }
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ids_and_reference_closure() {
        let g = GameDesign {
            header: ContractHeader {
                schema_version: DESIGN_SCHEMA_VERSION,
            },
            game_id: "tank".into(),
            genre: "top_down_tank".into(),
            core_loop: "move, shoot, survive".into(),
            actions: vec![ActionDef {
                id: "move".into(),
                name: "Move".into(),
                input: "wasd".into(),
                desc: "".into(),
            }],
            entities: vec![EntityDef {
                id: "player_tank".into(),
                kind: EntityKind::Player,
                desc: "".into(),
                actions: vec!["move".into()],
                speed: Some(3.0),
            }],
            rules: vec![RuleDef {
                id: "r1".into(),
                statement: "player moves with move".into(),
                entities: vec!["player_tank".into()],
                actions: vec!["move".into()],
            }],
            win_condition: Some(WinLose {
                id: "win".into(),
                statement: "reach exit".into(),
                refs: vec!["player_tank".into()],
            }),
            lose_condition: None,
            requirement_ids: vec![],
            scope: BTreeMap::new(),
        };
        assert!(validate_ids(&g).is_ok());
        let mut broken = g.clone();
        broken.rules[0].entities.push("ghost".into());
        assert!(validate_ids(&broken).is_err());
    }
}
