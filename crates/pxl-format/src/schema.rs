use pxl_core::{PxlError, PxlResult};
use schemars::schema_for;
use serde_json::Value;

use crate::model::{PaletteFile, PxlDoc, ShapesFile};
use crate::ops::Op;
use crate::parts::PartsFile;
use crate::recipe::Recipe;

/// Every schema name `pxl schema` accepts, including aliases. `doc` is the name
/// the harness and older notes use for the `.pxl.json` document (`pxl`).
pub const SCHEMA_KINDS: &[&str] = &[
    "recipe", "ops", "op", "pxl", "doc", "palette", "parts", "sheet", "shapes",
];

pub fn schema_json(kind: &str) -> PxlResult<Value> {
    let value = match kind {
        "ops" => serde_json::to_value(schema_for!(Vec<Op>)),
        "op" => serde_json::to_value(schema_for!(Op)),
        "pxl" | "doc" => serde_json::to_value(schema_for!(PxlDoc)),
        "palette" => serde_json::to_value(schema_for!(PaletteFile)),
        "recipe" => recipe_schema(),
        "parts" => serde_json::to_value(schema_for!(PartsFile)),
        "shapes" => serde_json::to_value(schema_for!(ShapesFile)),
        "sheet" => Ok(sheet_schema()),
        other => {
            return Err(PxlError::usage(format!("unknown schema '{other}'"))
                .with_hint(format!("schema is one of: {}", SCHEMA_KINDS.join(", "))));
        }
    };
    value.map_err(|e| PxlError::io(format!("schema encode failed: {e}")))
}

fn recipe_schema() -> Result<Value, serde_json::Error> {
    serde_json::to_value(schema_for!(Recipe))
}

fn sheet_schema() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "AsepriteHashSheet",
        "type": "object",
        "required": ["frames", "meta"],
        "properties": {
            "frames": { "type": "object" },
            "meta": {
                "type": "object",
                "required": ["image", "size", "frameTags"],
                "properties": {
                    "image": { "type": "string" },
                    "format": { "type": "string" },
                    "size": {
                        "type": "object",
                        "required": ["w", "h"],
                        "properties": { "w": { "type": "integer" }, "h": { "type": "integer" } }
                    },
                    "frameTags": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["name", "from", "to"],
                            "properties": {
                                "name": { "type": "string" },
                                "from": { "type": "integer" },
                                "to": { "type": "integer" },
                                "direction": { "type": "string" }
                            }
                        }
                    },
                    "pxl": { "type": "object" }
                }
            }
        }
    })
}

pub fn schema_names() -> &'static [&'static str] {
    &["recipe", "ops", "pxl", "palette", "sheet"]
}
