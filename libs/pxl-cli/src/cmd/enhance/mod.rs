//! `pxl enhance` and `pxl enhance compare`: rebuild a native-resolution sprite
//! at 2x to 4x, report what each stage did, and lay the choices side by side.


use pxl_art::{
    enhance, CleanOptions, EnhanceOptions, EnhanceReport, Enhanced, OutlineMode,
    Upscale,
};
use pxl_core::{IndexFrame, Palette, PxlError};
use pxl_export::{
    composite_on, contact_sheet, encode_rgba_png, render_asset_png, Panel,
};
use serde_json::{json, Value};

use crate::cmd::args::{EnhanceArgs, EnhanceCompareArgs, EnhanceRunArgs, EnhanceSub};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

mod load;
mod report;

/// Every upscaler, in the order `compare` lists them.
const ALL_UPSCALERS: [Upscale; 6] = [
    Upscale::Copy,
    Upscale::Epx,
    Upscale::Scale3x,
    Upscale::Xbr,
    Upscale::Contour,
    Upscale::Hybrid,
];

pub(super) fn enhance_cmd(ctx: &Ctx, args: EnhanceArgs) -> CmdResult<()> {
    match args.action {
        Some(EnhanceSub::Compare(a)) => compare_cmd(ctx, a),
        None => run_cmd(ctx, args.run),
    }
}

/// The frame and palette an input describes, before any enhancement.
pub(super) struct Source {
    frame: IndexFrame,
    palette: Palette,
    frame_name: String,
    warnings: Vec<String>,
}

fn run_cmd(ctx: &Ctx, args: EnhanceRunArgs) -> CmdResult<()> {
    let (Some(input), Some(out)) = (args.input.clone(), args.out.clone()) else {
        return Err(PxlError::usage("enhance needs an input and -o")
            .with_hint(
                "pxl enhance <input> -o out.png, or pxl enhance compare <input> -o sheet.png",
            )
            .into());
    };
    let upscale = load::parse_upscale(&args.upscale)?;
    let source = load::load_source(&input, &args.common)?;
    let opts = load::build_options(&args.common, upscale)?;
    let done = enhance(&source.frame, &source.palette, &opts)?;
    util::guard_warnings(ctx, &report::merged_warnings(&source, &done.report))?;

    let bg = load::background(&args.common, &done.palette, false)?;
    let zoom = args.common.zoom.unwrap_or(1).max(1);
    let png = if args.asset {
        render_asset_png(&done.frame, &done.palette, zoom)?
    } else {
        let (w, h, rgba) = composite_on(&done.frame, &done.palette, zoom, bg)?;
        encode_rgba_png(w, h, &rgba)?
    };
    let hash = util::hash_of(&png);
    util::write_out(&out, &png, ctx.overwrite, ctx.json)?;

    let report = report::report_json(&source, &done, bg);
    if let Some(path) = &args.report {
        let text = serde_json::to_string_pretty(&report)
            .map_err(|e| PxlError::io(format!("report: {e}")))?;
        util::write_out(path, text.as_bytes(), ctx.overwrite, ctx.json)?;
    }
    let mut body = util::ok_base("enhance");
    body.insert("out".into(), json!(out));
    body.insert("hash".into(), json!(hash));
    body.insert("png_w".into(), json!(done.frame.w as u32 * zoom));
    body.insert("png_h".into(), json!(done.frame.h as u32 * zoom));
    body.insert("zoom".into(), json!(zoom));
    body.insert(
        "purpose".into(),
        json!(if args.asset { "asset" } else { "preview" }),
    );
    body.insert("report".into(), report);
    body.insert(
        "warnings".into(),
        json!(report::merged_warnings(&source, &done.report)),
    );
    if args.metrics && !ctx.json && !ctx.quiet {
        report::print_metrics(&[(opts.upscale.name().to_string(), &done.report)]);
    }
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn compare_cmd(ctx: &Ctx, args: EnhanceCompareArgs) -> CmdResult<()> {
    let source = load::load_source(&args.input, &args.common)?;
    let scale = args.common.scale;
    let names: Vec<String> = match &args.variants {
        Some(list) => list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => ALL_UPSCALERS
            .iter()
            .filter(|u| u.supports(scale))
            .map(|u| u.name().to_string())
            .collect(),
    };
    if names.is_empty() {
        return Err(PxlError::usage("no variants to compare")
            .with_hint("pass --variants copy,hybrid, or choose a --scale that an upscaler supports")
            .into());
    }
    let bg = load::background(&args.common, &source.palette, true)?;
    let zoom = args.common.zoom.unwrap_or(4).max(1);
    // Panels are native-sized times scale times zoom, so every variant covers
    // the same footprint and can be compared by eye.
    let native_zoom = zoom;

    let mut panels = Vec::new();
    let mut results: Vec<(String, Enhanced)> = Vec::new();
    // The baseline: native pixels, nearest-neighbour, drawn outline kept.
    let baseline = EnhanceOptions {
        outline: OutlineMode::Keep,
        cleanup: CleanOptions::NONE,
        ..load::build_options(&args.common, Upscale::Copy)?
    };
    let base = enhance(&source.frame, &source.palette, &baseline)?;
    let (bw, bh, brgba) = composite_on(&base.frame, &base.palette, native_zoom, bg)?;
    panels.push(Panel {
        w: bw,
        h: bh,
        rgba: brgba,
        title: format!("copy {scale}x"),
        lines: vec![
            "baseline".into(),
            format!("{}x{}", base.frame.w, base.frame.h),
        ],
    });

    for name in &names {
        let mode = load::parse_upscale(name)?;
        if !mode.supports(scale) {
            return Err(PxlError::usage(format!(
                "upscale '{}' does not support scale {scale}",
                mode.name()
            ))
            .with_hint("epx and xbr are 2x, scale3x is 3x; copy, contour and hybrid take 2 to 4")
            .into());
        }
        let opts = load::build_options(&args.common, mode)?;
        let done = enhance(&source.frame, &source.palette, &opts)?;
        let (w, h, rgba) = composite_on(&done.frame, &done.palette, native_zoom, bg)?;
        let m = &done.report.metrics;
        panels.push(Panel {
            w,
            h,
            rgba,
            title: mode.name().to_string(),
            lines: vec![
                format!("{}x{}", done.frame.w, done.frame.h),
                format!("new {}", m.new_info_bps),
                format!("jag {}", m.jaggy_irregular),
                format!("elbow {}", m.lshape_count),
                format!("orph {}", m.orphan_count),
                format!("small {}", m.small_feature_kept_bps),
                format!("dl {}", m.outline_bg_dl),
                format!("ramp {}", m.ramp_coverage_bps),
                format!("range {}", m.value_range),
            ],
        });
        results.push((mode.name().to_string(), done));
    }

    let (sw, sh, sheet) = contact_sheet(&panels, 1)?;
    let png = encode_rgba_png(sw, sh, &sheet)?;
    let hash = util::hash_of(&png);
    util::write_out(&args.out, &png, ctx.overwrite, ctx.json)?;

    let mut warnings = source.warnings.clone();
    for (name, done) in &results {
        for w in &done.report.warnings {
            warnings.push(format!("{name}: {w}"));
        }
    }
    util::guard_warnings(ctx, &warnings)?;
    let variants: Vec<Value> = results
        .iter()
        .map(|(name, done)| {
            json!({
                "upscale": name,
                "size": [done.frame.w, done.frame.h],
                "metrics": report::metrics_json(&done.report),
            })
        })
        .collect();
    let mut body = util::ok_base("enhance_compare");
    body.insert("out".into(), json!(args.out));
    body.insert("hash".into(), json!(hash));
    body.insert("png_w".into(), json!(sw));
    body.insert("png_h".into(), json!(sh));
    body.insert("scale".into(), json!(scale));
    body.insert("zoom".into(), json!(zoom));
    body.insert("background".into(), json!(bg.name()));
    body.insert("frame".into(), json!(source.frame_name));
    body.insert("variants".into(), Value::Array(variants));
    body.insert("warnings".into(), json!(warnings));
    if !ctx.json && !ctx.quiet {
        let rows: Vec<(String, &EnhanceReport)> = results
            .iter()
            .map(|(n, d)| (n.clone(), &d.report))
            .collect();
        report::print_metrics(&rows);
    }
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}


#[cfg(test)]
mod tests;
