use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use pxl_format::PxlDoc;
use pxl_qa::{validate, Report};
use serde_json::{json, Value};

use super::args::{InspectArgs, ValidateArgs};
use super::qa_input::{doc_input, sheet_input};
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

pub(super) fn inspect_cmd(ctx: &Ctx, args: InspectArgs) -> CmdResult<()> {
    let (doc, doc_path) = util::load_doc(&args.input)?;
    let palette = util::doc_palette(&doc, &doc_path).ok();
    let names: Vec<String> = if let Some(frame) = &args.frame {
        vec![frame.clone()]
    } else {
        doc.frame_names()
    };
    let mut listed = Vec::new();
    for name in &names {
        let frame = doc.frame(name)?;
        let stats = frame.stats();
        let ascii = pxl_core::encode_matrix(&frame)?;
        if !ctx.json && !ctx.quiet {
            println!(
                "{name} {}x{} colors {} opaque {} components {} isolated {}",
                frame.w, frame.h, stats.colors_used, stats.opaque, stats.components, stats.isolated
            );
            if let Some(b) = stats.bbox {
                println!("  bbox x={} y={} w={} h={}", b.x, b.y, b.w, b.h);
            }
            for row in &ascii {
                println!("  {row}");
            }
        }
        listed.push(json!({
            "name": name,
            "colors_used": stats.colors_used,
            "opaque": stats.opaque,
            "components": stats.components,
            "isolated": stats.isolated,
            "bbox": stats.bbox.map(|b| json!({"x": b.x, "y": b.y, "w": b.w, "h": b.h})),
            "ascii": ascii,
        }));
    }
    let mut body = util::ok_base("inspect");
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("palette".into(), json!(doc.palette));
    body.insert(
        "palette_colors".into(),
        json!(palette.as_ref().map(|p| p.colors.len()).unwrap_or(0)),
    );
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert(
        "anims".into(),
        json!(doc.anims.keys().cloned().collect::<Vec<_>>()),
    );
    body.insert("view".into(), Value::Array(listed));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn validate_cmd(ctx: &Ctx, args: ValidateArgs) -> CmdResult<()> {
    let report = load_report(&args.input, &args.rules)?;
    let failed = report.checks.iter().filter(|c| !c.pass).count();
    let checks = checks_json(&report);
    if report.ok() {
        let mut body = util::ok_base("validate");
        body.insert("rules".into(), json!(report.rules));
        body.insert("checks".into(), checks);
        util::emit_ok(ctx, &Value::Object(body));
        Ok(())
    } else {
        Err(CmdError::new(
            PxlError::qa(format!("{failed} check(s) failed"))
                .with_hint("read the checks array and adjust the recipe"),
        )
        .with_extra(json!({"cmd": "validate", "rules": report.rules, "checks": checks})))
    }
}

pub(super) fn checks_json(report: &Report) -> Value {
    Value::Array(
        report
            .checks
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "pass": c.pass,
                    "value": c.value,
                    "limit": c.limit,
                })
            })
            .collect(),
    )
}

pub(super) fn load_report(path: &str, rules: &str) -> CmdResult<Report> {
    let text = util::read_text(path)?;
    let value: Value = pxl_format::parse_json(&text, "validate input")?;
    if value.get("meta").and_then(|m| m.get("frameTags")).is_some() {
        let input = sheet_input(Path::new(path), &value)?;
        return Ok(validate(&input, rules)?);
    }
    let (doc, doc_path) = {
        let doc: PxlDoc = pxl_format::parse_json(&text, "pxl document")?;
        (doc, PathBuf::from(path))
    };
    let palette = util::doc_palette(&doc, &doc_path)?;
    Ok(validate(&doc_input(&doc, &palette)?, rules)?)
}
