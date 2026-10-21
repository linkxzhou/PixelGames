use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_steps() -> u32 {
    6
}
fn default_bayer() -> String {
    "bayer4".into()
}
fn default_outer() -> String {
    "outer".into()
}
fn default_square() -> String {
    "square".into()
}
fn default_anchor() -> String {
    "center".into()
}

/// A rectangle, a name resolved by a macro, or an `{x,y,w,h}` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Region {
    Rect([i32; 4]),
    Name(String),
    Obj { x: i32, y: i32, w: i32, h: i32 },
}

impl Region {
    pub fn to_rect(&self) -> Option<[i32; 4]> {
        match self {
            Region::Rect(r) => Some(*r),
            Region::Obj { x, y, w, h } => Some([*x, *y, *w, *h]),
            Region::Name(_) => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Region::Name(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ColorRef {
    Index(u8),
    Name(String),
}

/// One integer transform. Parameters are integers or small enums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Shift {
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        region: Option<Region>,
    },
    RowShift {
        #[serde(default)]
        rows: Option<[i32; 2]>,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
        #[serde(default)]
        pattern: Option<String>,
        #[serde(default)]
        phase: i32,
        #[serde(default)]
        region: Option<Region>,
        #[serde(default)]
        axis: Option<String>,
        /// Defaults to true when a region is set, otherwise false.
        #[serde(default)]
        wrap: Option<bool>,
    },
    Shear {
        #[serde(default = "default_axis_x")]
        axis: String,
        /// Pixel shift is `(row - pivot) / k`.
        #[serde(default = "one_i32")]
        k: i32,
        #[serde(default)]
        pivot: i32,
        #[serde(default)]
        region: Option<Region>,
    },
    Squash {
        #[serde(default)]
        sx: i32,
        #[serde(default)]
        sy: i32,
        #[serde(default = "default_anchor")]
        anchor: String,
        #[serde(default)]
        region: Option<Region>,
    },
    Flip {
        axis: String,
    },
    Rotate90 {
        dir: String,
    },
    /// Arbitrary-angle rotation. Rejected until M2 (cleanEdge).
    Rotate {
        angle: i32,
        #[serde(default)]
        pivot: Option<[i32; 2]>,
        #[serde(default)]
        algo: Option<String>,
    },
    PaletteSwap {
        map: String,
        #[serde(default)]
        region: Option<Region>,
    },
    PaletteCycle {
        cycle: String,
        #[serde(default)]
        step: i32,
        #[serde(default)]
        region: Option<Region>,
    },
    Shade {
        dir: String,
        #[serde(default = "one_i32")]
        amount: i32,
        #[serde(default)]
        region: Option<Region>,
    },
    Outline {
        #[serde(default = "default_outline_color")]
        color: ColorRef,
        #[serde(default = "default_outer")]
        mode: String,
        #[serde(default = "default_square")]
        brush: String,
        #[serde(default = "one_i32")]
        width: i32,
    },
    Dissolve {
        #[serde(default)]
        seed: u64,
        #[serde(default = "default_steps")]
        steps: u32,
        #[serde(default)]
        step: u32,
        #[serde(default = "default_bayer")]
        pattern: String,
    },
    MaskErase {
        #[serde(default)]
        region: Option<Region>,
        #[serde(default)]
        mask: Option<Vec<String>>,
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
    },
    MaskFill {
        color: ColorRef,
        #[serde(default)]
        region: Option<Region>,
        #[serde(default)]
        mask: Option<Vec<String>>,
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
    },
    PatchPixels {
        x: i32,
        y: i32,
        rows: Vec<String>,
    },
    Texture {
        gen: String,
        template: String,
        region: Region,
        #[serde(default)]
        ramp: Option<String>,
        #[serde(default)]
        seed: Option<u64>,
    },
    Noop {},
    /// Record an absolute pose for `part`. Offsets are integers; `rot90` is cw, ccw, or 180.
    PartTransform {
        part: String,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
        #[serde(default)]
        rot90: Option<String>,
        #[serde(default)]
        flip: Option<String>,
    },
    /// Draw parts in `parts` order (or the facing's order when empty).
    /// `dir` selects sprites, z-order, and a whole-canvas mirror such as right←left.
    Compose {
        #[serde(default)]
        parts: Vec<String>,
        #[serde(default)]
        dir: Option<String>,
    },
}

fn one_i32() -> i32 {
    1
}
fn default_axis_x() -> String {
    "x".into()
}
fn default_outline_color() -> ColorRef {
    ColorRef::Name("outline".into())
}

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Op::Shift { .. } => "shift",
            Op::RowShift { .. } => "row_shift",
            Op::Shear { .. } => "shear",
            Op::Squash { .. } => "squash",
            Op::Flip { .. } => "flip",
            Op::Rotate90 { .. } => "rotate90",
            Op::Rotate { .. } => "rotate",
            Op::PaletteSwap { .. } => "palette_swap",
            Op::PaletteCycle { .. } => "palette_cycle",
            Op::Shade { .. } => "shade",
            Op::Outline { .. } => "outline",
            Op::Dissolve { .. } => "dissolve",
            Op::MaskErase { .. } => "mask_erase",
            Op::MaskFill { .. } => "mask_fill",
            Op::PatchPixels { .. } => "patch_pixels",
            Op::Texture { .. } => "texture",
            Op::Noop { .. } => "noop",
            Op::PartTransform { .. } => "part_transform",
            Op::Compose { .. } => "compose",
        }
    }
}

pub const OP_NAMES: &[&str] = &[
    "shift",
    "row_shift",
    "shear",
    "squash",
    "flip",
    "rotate90",
    "rotate",
    "palette_swap",
    "palette_cycle",
    "shade",
    "outline",
    "dissolve",
    "mask_erase",
    "mask_fill",
    "patch_pixels",
    "texture",
    "noop",
    "part_transform",
    "compose",
];

pub fn parse_ops_value(text: &str) -> Result<Vec<Op>, pxl_core::PxlError> {
    let value: serde_json::Value = crate::parse_json(text, "ops")?;
    match value {
        serde_json::Value::Array(_) => crate::parse_json(text, "ops"),
        serde_json::Value::Object(_) => {
            let op: Op = crate::parse_json(text, "op")?;
            Ok(vec![op])
        }
        _ => Err(
            pxl_core::PxlError::schema("ops must be an object or an array")
                .with_hint("example: [{\"op\":\"flip\",\"axis\":\"x\"}]"),
        ),
    }
}
