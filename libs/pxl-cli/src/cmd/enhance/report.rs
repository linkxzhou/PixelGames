//! `pxl enhance` and `pxl enhance compare`: rebuild a native-resolution sprite
//! at 2x to 4x, report what each stage did, and lay the choices side by side.


use pxl_art::cleanup::CleanReport;
use pxl_art::{
    EnhanceReport, Enhanced,
};
use pxl_core::{IndexFrame, PxlResult};
use pxl_export::{
    scaled_dims,
    Background,
};
use serde_json::{json, Value};


use super::Source;

pub(super) fn merged_warnings(source: &Source, report: &EnhanceReport) -> Vec<String> {
    let mut out = source.warnings.clone();
    out.extend(report.warnings.iter().cloned());
    out
}

pub(super) fn metrics_json(report: &EnhanceReport) -> Value {
    let mut map = serde_json::Map::new();
    for (id, value) in report.metrics.entries() {
        map.insert(id.into(), json!(value));
    }
    Value::Object(map)
}

pub(super) fn clean_json(c: &CleanReport) -> Value {
    json!({
        "orphans": c.orphans,
        "elbows": c.elbows,
        "notches": c.notches,
        "rhythm": c.rhythm,
        "symmetry": c.symmetry,
    })
}

pub(super) fn report_json(source: &Source, done: &Enhanced, bg: Background) -> Value {
    let r = &done.report;
    json!({
        "frame": source.frame_name,
        "native": [r.native.0, r.native.1],
        "scale": r.scale,
        "size": [done.frame.w, done.frame.h],
        "stages": r.stages,
        "protect": {
            "enabled": r.protect.enabled,
            "pixels": r.protect.pixels,
            "small": r.protect.small,
            "thin": r.protect.thin,
        },
        "outline_stripped": r.stripped,
        "outline_painted": r.outline.painted,
        "outline_repaired": r.outline.repaired,
        "clean": clean_json(&r.clean),
        "shade": {
            "volumes": r.shade.volumes,
            "flat": r.shade.flat,
            "lit": r.shade.lit,
            "shaded": r.shade.shaded,
            "speculars": r.shade.speculars,
        },
        "ramps": {
            "appended": r.ramps.appended,
            "generated": r.ramps.generated,
            "declared": r.ramps.declared,
            "skipped": r.ramps.skipped,
        },
        "palette_colors": done.palette.colors.len(),
        "background": bg.name(),
        "metrics": metrics_json(r),
        "warnings": r.warnings,
    })
}

/// A table with one column per variant, rows in the order the metrics are listed.
pub(super) fn print_metrics(rows: &[(String, &EnhanceReport)]) {
    let ids: Vec<&'static str> = rows
        .first()
        .map(|(_, r)| r.metrics.entries().iter().map(|(id, _)| *id).collect())
        .unwrap_or_default();
    let mut head = format!("{:<24}", "metric");
    for (name, _) in rows {
        head.push_str(&format!("{name:>10}"));
    }
    println!("{head}");
    for id in ids {
        let mut line = format!("{id:<24}");
        for (_, r) in rows {
            let value = r
                .metrics
                .entries()
                .into_iter()
                .find(|(k, _)| *k == id)
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "-".into());
            line.push_str(&format!("{value:>10}"));
        }
        println!("{line}");
    }
}

#[allow(dead_code)]
pub(super) fn footprint(frame: &IndexFrame, zoom: u32) -> PxlResult<(u32, u32)> {
    scaled_dims(frame.w as u32, frame.h as u32, zoom)
}

