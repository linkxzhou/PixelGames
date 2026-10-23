use pxl_core::PxlError;
use pxl_format::schema_json;
use pxl_gen::{builtin_palette, builtin_template, palette_ids, template_ids};
use pxl_ops::{macro_catalog, MACRO_NAMES};
use serde_json::{json, Value};

use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn schema_cmd(ctx: &Ctx, kind: &str) -> CmdResult<()> {
    let value = schema_json(kind)?;
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string(&value).map_err(|e| PxlError::io(e.to_string()))?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| PxlError::io(e.to_string()))?
        );
    }
    Ok(())
}

pub(super) fn list_cmd(ctx: &Ctx, kind: &str) -> CmdResult<()> {
    let items = match kind {
        "generators" => vec![
            "bollinger".into(),
            "shapes".into(),
            "noise-tile".into(),
            "parts".into(),
        ],
        "ops" => pxl_format::OP_NAMES
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        "macros" => MACRO_NAMES.iter().map(|s| (*s).to_string()).collect(),
        "templates" => template_ids().into_iter().map(|s| s.to_string()).collect(),
        "palettes" => palette_ids().into_iter().map(|s| s.to_string()).collect(),
        other => {
            return Err(PxlError::usage(format!("unknown list kind '{other}'"))
                .with_hint("list generators, ops, macros, templates, or palettes")
                .into());
        }
    };
    let mut body = util::ok_base("list");
    body.insert("kind".into(), json!(kind));
    body.insert("items".into(), json!(items));
    // Macros carry argument metadata (defaults, frame count, example) so
    // callers do not have to guess signatures like `pulse_glow(steps=...)`.
    if kind == "macros" {
        let catalog: Vec<Value> = macro_catalog()
            .into_iter()
            .map(|m| {
                let args: Vec<Value> = m
                    .args
                    .iter()
                    .map(|a| match a.default {
                        Some(d) => json!({"name": a.name, "default": d}),
                        None if a.required => json!({"name": a.name, "required": true}),
                        None => json!({"name": a.name, "default": null, "derived": true}),
                    })
                    .collect();
                json!({
                    "name": m.name,
                    "args": args,
                    "frames": m.frames,
                    "example": m.example,
                })
            })
            .collect();
        body.insert("macros".into(), json!(catalog));
    }
    util::emit_ok(ctx, &Value::Object(body));
    let _ = builtin_palette;
    let _ = builtin_template;
    Ok(())
}
