use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ops::Op;

fn v1() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeBase {
    pub gen: String,
    #[serde(default)]
    pub spec: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub ramp: Option<String>,
    #[serde(default)]
    pub mirror: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectionStep {
    pub id: String,
    pub from: String,
    #[serde(default)]
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeAnim {
    pub name: String,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(rename = "loop", default = "default_true")]
    pub looped: bool,
    #[serde(default)]
    pub dirs: Vec<String>,
    /// JSON key is `macro`.
    #[serde(rename = "macro", default)]
    pub macro_call: Option<String>,
    /// Explicit per-frame op lists, used when `macro` is absent.
    /// Each entry is the op list for one frame, applied to the direction frame.
    #[serde(default)]
    pub frames: Vec<Vec<Op>>,
}

fn default_fps() -> u32 {
    8
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeSprite {
    pub id: String,
    pub base: RecipeBase,
    #[serde(default)]
    pub post: Vec<Op>,
    #[serde(default)]
    pub directions: Vec<DirectionStep>,
    #[serde(default)]
    pub anims: Vec<RecipeAnim>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeTile {
    pub id: String,
    #[serde(default = "default_dirt")]
    pub kind: String,
    #[serde(default = "default_size")]
    pub size: u16,
    pub seed: u64,
    #[serde(default = "default_variants")]
    pub variants: u32,
    #[serde(default)]
    pub ramp: Option<String>,
}

fn default_dirt() -> String {
    "dirt".into()
}
fn default_size() -> u16 {
    16
}
fn default_variants() -> u32 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeSheet {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "one_u32")]
    pub pad: u32,
    #[serde(default = "default_sheet_name")]
    pub file: String,
}

fn default_layout() -> String {
    "rows-by-anim".into()
}
fn one_u32() -> u32 {
    1
}
fn default_sheet_name() -> String {
    "sheet.sheet.png".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipePreview {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub sheet: Option<bool>,
    #[serde(default)]
    pub anim: Option<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default = "one_u32")]
    pub scale: u32,
    #[serde(default)]
    pub grid: bool,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecipeValidate {
    #[serde(default = "default_rules")]
    pub rules: String,
}

fn default_rules() -> String {
    "default".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Recipe {
    #[serde(default = "v1")]
    pub v: u32,
    pub asset: String,
    #[serde(default = "v1")]
    pub gen_version: u32,
    #[serde(default)]
    pub seed: u64,
    pub palette: String,
    #[serde(default)]
    pub sprites: Vec<RecipeSprite>,
    #[serde(default)]
    pub tiles: Vec<RecipeTile>,
    pub sheet: RecipeSheet,
    #[serde(default)]
    pub preview: Vec<RecipePreview>,
    #[serde(default)]
    pub validate: Option<RecipeValidate>,
}

impl RecipeAnim {
    pub fn macro_name(&self) -> Option<&str> {
        self.macro_call.as_deref()
    }
}
