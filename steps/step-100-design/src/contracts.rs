#![allow(dead_code)]

//! Design package contracts, absorbed verbatim from
//! `harness/src/contracts/{mod,design}.rs` in M3 (dual-track: the harness
//! keeps its own copy until M6).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Everything versioned in the design package shares this header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractHeader {
    pub schema_version: u32,
}

pub fn stable_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub const DESIGN_SCHEMA_VERSION: u32 = 3;

pub const ACCEPTANCE_CHECKS: &[&str] = &[
    "size.equals",
    "anim.frames",
    "anim.dirs",
    "anim.anchor",
    "asset.kind",
    "palette.profile",
    "tile.seamless",
    "silhouette_readable",
    "style_consistent",
    "animation_readable",
];

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
    #[serde(default)]
    pub size_profiles: Vec<SizeProfilePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_policy: Option<String>,
    #[serde(default)]
    pub review: ReviewPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeProfilePolicy {
    pub size_px: [u32; 2],
    pub max_colors: usize,
    #[serde(default)]
    pub outline: String,
    #[serde(default)]
    pub aa: String,
    #[serde(default)]
    pub shade: String,
    #[serde(default)]
    pub dither: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewPolicy {
    #[serde(default)]
    pub backgrounds: Vec<String>,
    #[serde(default)]
    pub uniform_display_extent: bool,
    #[serde(default)]
    pub tile_grid_3x3: bool,
    #[serde(default)]
    pub actual_animation_timing: bool,
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

/// Which sounds the game needs. A sibling of `asset-requirements.json`, with
/// its own version, because `design/v3` is closed to new fields. Optional: a
/// design without this file has no audio requirement.
pub const AUDIO_REQUIREMENTS_VERSION: u32 = 1;
pub const AUDIO_REQUIREMENTS_FILE: &str = "audio-requirements.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioRequirements {
    pub schema_version: u32,
    pub requirements: Vec<AudioRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioRequirement {
    pub audio_id: String,
    pub kind: AudioKind,
    pub required: bool,
    pub trigger: String,
    #[serde(default)]
    pub max_seconds: Option<f64>,
    #[serde(default, rename = "loop")]
    pub looped: Option<bool>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioKind {
    Sfx,
    Bgm,
}

/// Problems that make an audio requirements file unusable, as readable
/// strings. Empty means it is valid.
pub fn validate_audio_requirements(a: &AudioRequirements) -> Vec<String> {
    let mut problems = Vec::new();
    if a.schema_version != AUDIO_REQUIREMENTS_VERSION {
        problems.push(format!(
            "audio-requirements schema_version {} is unsupported; expected {AUDIO_REQUIREMENTS_VERSION}",
            a.schema_version
        ));
    }
    if a.requirements.is_empty() {
        problems.push("audio-requirements has no requirements".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in &a.requirements {
        if !stable_id(&r.audio_id) || r.audio_id.len() > 64 {
            problems.push(format!(
                "audio id '{}' is not a stable id (max 64)",
                r.audio_id
            ));
        }
        if !seen.insert(r.audio_id.as_str()) {
            problems.push(format!("audio id '{}' is duplicated", r.audio_id));
        }
        if r.trigger.trim().is_empty() {
            problems.push(format!("audio '{}' has an empty trigger", r.audio_id));
        }
        if let Some(seconds) = r.max_seconds {
            let limit = match r.kind {
                AudioKind::Sfx => 2.0,
                AudioKind::Bgm => 60.0,
            };
            if !seconds.is_finite() || seconds <= 0.0 || seconds > limit {
                problems.push(format!(
                    "audio '{}' max_seconds {seconds} is outside (0, {limit}] for {:?}",
                    r.audio_id, r.kind
                ));
            }
        }
        if r.kind == AudioKind::Sfx && r.looped == Some(true) {
            problems.push(format!("audio '{}' is an sfx and cannot loop", r.audio_id));
        }
    }
    problems
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
    #[serde(default)]
    pub visual: VisualRequirement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AssetAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_policy: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<AssetAcceptance>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisualRequirement {
    #[serde(default)]
    pub view: String,
    #[serde(default)]
    pub silhouette: String,
    #[serde(default)]
    pub materials: Vec<String>,
    #[serde(default)]
    pub protected_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetAnchor {
    pub center_x: i32,
    pub bottom_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetAcceptance {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: AcceptanceType,
    pub check: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceType {
    Machine,
    Human,
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
    #[serde(default)]
    pub mode: AnimMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_tolerance_px: Option<u32>,
    #[serde(default)]
    pub protected_features: Vec<String>,
    #[serde(default)]
    pub change_regions: Vec<String>,
    #[serde(default)]
    pub allow_terminal_empty: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimMode {
    #[default]
    Dynamic,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Sprite,
    Tile,
    Ui,
    Effect,
    Overlay,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStrategy {
    #[default]
    Template,
    Shapes,
    Ascii,
    Svg,
    TileGen,
    Parts,
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
