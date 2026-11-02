use pxl_core::{PxlError, PxlResult};
use schemars::schema_for;
use serde_json::Value;

use crate::ascii::{AsciiSpec, LegendFile};
use crate::model::{PaletteFile, PxlDoc, ShapesFile};
use crate::ops::Op;
use crate::parts::PartsFile;
use crate::recipe::Recipe;
use crate::sprite::SpriteSpec;

/// Every schema name `pxl schema` accepts, including aliases. `doc` is the name
/// the harness and older notes use for the `.pxl.json` document (`pxl`).
pub const SCHEMA_KINDS: &[&str] = &[
    "recipe",
    "ops",
    "op",
    "pxl",
    "doc",
    "palette",
    "parts",
    "sheet",
    "shapes",
    "ascii",
    "legend",
    "sprite",
    "svg-subset",
    "visual-intent",
    "visual-intent-v2",
    "visual-plan",
    "visual-plan-v2",
    "prototype",
    "surface-recipe",
    "asset-instance",
    "catalog-lock",
    "scene-kit",
    "tileset",
    "module",
    "compiled-scene",
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
        "ascii" => serde_json::to_value(schema_for!(AsciiSpec)),
        "legend" => serde_json::to_value(schema_for!(LegendFile)),
        "sprite" => serde_json::to_value(schema_for!(SpriteSpec)),
        "svg-subset" => Ok(svg_subset_schema()),
        "visual-intent" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/visual-intent.schema.json"
        )),
        "visual-intent-v2" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/visual-intent.v2.schema.json"
        )),
        "visual-plan" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/visual-plan.schema.json"
        )),
        "visual-plan-v2" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/visual-plan.v2.schema.json"
        )),
        "prototype" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/prototype.schema.json"
        )),
        "surface-recipe" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/surface-recipe.schema.json"
        )),
        "asset-instance" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/asset-instance.schema.json"
        )),
        "catalog-lock" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/catalog-lock.schema.json"
        )),
        "scene-kit" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/scene-kit.schema.json"
        )),
        "tileset" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/tileset.schema.json"
        )),
        "module" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/module.schema.json"
        )),
        "compiled-scene" => serde_json::from_str(include_str!(
            "../../../../steps/contracts/compiled-scene.schema.json"
        )),
        "sheet" => Ok(sheet_schema()),
        other => {
            return Err(PxlError::usage(format!("unknown schema '{other}'"))
                .with_hint(format!("schema is one of: {}", SCHEMA_KINDS.join(", "))));
        }
    };
    value.map_err(|e| PxlError::io(format!("schema encode failed: {e}")))
}

/// The supported SVG subset as a machine-readable hint document (plan v3.0
/// section 5.3): elements, attributes and the `data-*` extensions the
/// rasterizer understands.
fn svg_subset_schema() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "pxl SVG subset",
        "type": "object",
        "required": ["svg"],
        "properties": {
            "svg": {
                "type": "element",
                "attributes": ["viewBox", "width", "height"],
                "children": {
                    "structure": ["g", "defs", "use (href/xlink:href)", "symbol"],
                    "shapes": ["rect (rx/ry)", "circle", "ellipse", "line", "polyline", "polygon", "path"],
                    "path_commands": ["M", "L", "H", "V", "C", "S", "Q", "T", "A", "Z"],
                    "transforms": ["translate", "scale", "rotate", "skewX", "skewY", "matrix"],
                    "gradients": ["linearGradient", "radialGradient"],
                    "clip": ["clipPath (shapes only)"]
                }
            }
        },
        "paint": {
            "fill": "palette hex or data-ramp",
            "stroke": "palette hex or data-ramp",
            "stroke-width": "number",
            "stroke-linejoin": ["miter", "round", "bevel"],
            "stroke-linecap": ["butt", "round", "square"],
            "fill-rule": ["nonzero", "evenodd"],
            "display": "none skips the subtree",
            "visibility": "visible | hidden",
            "style": "inline presentation attributes",
            "opacity": "only 0 or 1; other values warn and round to 0/1"
        },
        "unsupported_warns": ["switch", "foreignObject", "text", "image", "pattern", "mask", "filter", "nested svg", "external CSS"],
        "data_attributes": {
            "data-frame": "name this group as an animation frame",
            "data-role": ["outline", "highlight", "shadow", "eye", "detail"],
            "data-material": ["metal", "organic", "cloth", "stone", "liquid"],
            "data-ramp": "palette ramp id, with data-step for the level",
            "data-step": "ramp level",
            "data-min": "keep at least N pixels of this shape at every size",
            "data-sizes": "only appear at these tiers (comma list of 32/64/128/256)"
        },
        "authoring": [
            "viewBox should equal the target size, e.g. 0 0 64 64",
            "use palette hex colours or data-ramp; avoid opacity, filters and text",
            "outline via data-role=outline instead of stroke",
            "tag small parts (eyes, highlights) with data-role and data-min",
            "exclude small-tier details with data-sizes",
            "animate with g data-frame + use, integer translations"
        ]
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_schema_kind_resolves_to_a_json_object() {
        for kind in SCHEMA_KINDS {
            let value = schema_json(kind).unwrap_or_else(|e| panic!("{kind}: {e:?}"));
            assert!(value.is_object(), "{kind} did not resolve to an object");
        }
    }

    #[test]
    fn prototype_library_schemas_carry_their_contract_ids() {
        for (kind, id) in [
            ("prototype", "pxl/prototype/v1"),
            ("surface-recipe", "pxl/surface-recipe/v1"),
            ("asset-instance", "pxl/asset-instance/v1"),
            ("catalog-lock", "pxl/catalog-lock/v1"),
            ("scene-kit", "pxl/scene-kit/v1"),
            ("tileset", "pxl/tileset/v1"),
            ("module", "pxl/module/v1"),
            ("compiled-scene", "pxl/compiled-scene/v1"),
        ] {
            assert_eq!(schema_json(kind).unwrap()["$id"], id, "{kind}");
        }
    }

    #[test]
    fn capability_manifest_lists_the_registered_prototype_schemas() {
        let caps: Value = serde_json::from_str(crate::capabilities::CAPABILITIES_JSON).unwrap();
        let listed: Vec<&str> = caps["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for kind in [
            "prototype",
            "surface-recipe",
            "asset-instance",
            "catalog-lock",
            "scene-kit",
            "tileset",
            "module",
            "compiled-scene",
        ] {
            assert!(listed.contains(&kind), "capabilities.json is missing schema '{kind}'");
            assert!(SCHEMA_KINDS.contains(&kind), "SCHEMA_KINDS is missing '{kind}'");
        }
        assert_eq!(caps["visual"]["region_transforms"], serde_json::json!(["translate"]));
    }
}
