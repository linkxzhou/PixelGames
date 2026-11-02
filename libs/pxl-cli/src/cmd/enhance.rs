//! `pxl enhance` and `pxl enhance compare`: rebuild a native-resolution sprite
//! at 2x to 4x, report what each stage did, and lay the choices side by side.

use std::path::Path;

use pxl_art::cleanup::CleanReport;
use pxl_art::palette::RampOptions;
use pxl_art::{
    enhance, CleanOptions, EnhanceOptions, EnhanceReport, Enhanced, Light, OutlineMode, ShadeMode,
    Upscale,
};
use pxl_core::{IndexFrame, Palette, PxlError, PxlResult, Rgba};
use pxl_engine::{assemble, InputKind, RenderRequest};
use pxl_export::{
    composite_on, contact_sheet, decode_png_rgba, encode_rgba_png, render_asset_png, scaled_dims,
    Background, Panel,
};
use serde_json::{json, Value};

use super::args::{EnhanceArgs, EnhanceCommon, EnhanceCompareArgs, EnhanceRunArgs, EnhanceSub};
use crate::error::CmdResult;
use crate::util::{self, Ctx, DirResolver};

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
struct Source {
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
    let upscale = parse_upscale(&args.upscale)?;
    let source = load_source(&input, &args.common)?;
    let opts = build_options(&args.common, upscale)?;
    let done = enhance(&source.frame, &source.palette, &opts)?;
    util::guard_warnings(ctx, &merged_warnings(&source, &done.report))?;

    let bg = background(&args.common, &done.palette, false)?;
    let zoom = args.common.zoom.unwrap_or(1).max(1);
    let png = if args.asset {
        render_asset_png(&done.frame, &done.palette, zoom)?
    } else {
        let (w, h, rgba) = composite_on(&done.frame, &done.palette, zoom, bg)?;
        encode_rgba_png(w, h, &rgba)?
    };
    let hash = util::hash_of(&png);
    util::write_out(&out, &png, ctx.overwrite, ctx.json)?;

    let report = report_json(&source, &done, bg);
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
        json!(merged_warnings(&source, &done.report)),
    );
    if args.metrics && !ctx.json && !ctx.quiet {
        print_metrics(&[(opts.upscale.name().to_string(), &done.report)]);
    }
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn compare_cmd(ctx: &Ctx, args: EnhanceCompareArgs) -> CmdResult<()> {
    let source = load_source(&args.input, &args.common)?;
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
    let bg = background(&args.common, &source.palette, true)?;
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
        ..build_options(&args.common, Upscale::Copy)?
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
        let mode = parse_upscale(name)?;
        if !mode.supports(scale) {
            return Err(PxlError::usage(format!(
                "upscale '{}' does not support scale {scale}",
                mode.name()
            ))
            .with_hint("epx and xbr are 2x, scale3x is 3x; copy, contour and hybrid take 2 to 4")
            .into());
        }
        let opts = build_options(&args.common, mode)?;
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
                "metrics": metrics_json(&done.report),
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
        print_metrics(&rows);
    }
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn parse_upscale(text: &str) -> CmdResult<Upscale> {
    Upscale::parse(text).ok_or_else(|| {
        PxlError::usage(format!("unknown upscale '{text}'"))
            .with_hint("upscale is copy, epx, xbr, scale3x, contour, or hybrid")
            .into()
    })
}

fn build_options(common: &EnhanceCommon, upscale: Upscale) -> CmdResult<EnhanceOptions> {
    let outline = OutlineMode::parse(&common.outline).ok_or_else(|| {
        PxlError::usage(format!("unknown outline '{}'", common.outline))
            .with_hint("outline is keep, none, flat, selout, or selout_lit")
    })?;
    let light = Light::parse(&common.light).ok_or_else(|| {
        PxlError::usage(format!("unknown light '{}'", common.light))
            .with_hint("light is top_left, top_right, or top")
    })?;
    let shade = ShadeMode::parse(&common.shade).ok_or_else(|| {
        PxlError::usage(format!("unknown shade '{}'", common.shade))
            .with_hint("shade is none or volume")
    })?;
    let protect = match common.protect.as_str() {
        "auto" => true,
        "none" => false,
        _ => {
            return Err(
                PxlError::usage(format!("unknown protect '{}'", common.protect))
                    .with_hint("protect is auto or none")
                    .into(),
            )
        }
    };
    if !(2..=4).contains(&common.scale) {
        return Err(
            PxlError::usage(format!("--scale {} is outside 2..=4", common.scale))
                .with_hint("enhance multiplies the native size by 2, 3 or 4")
                .into(),
        );
    }
    if !(2..=4).contains(&common.levels) {
        return Err(PxlError::usage(format!("--levels {} is outside 2..=4", common.levels)).into());
    }
    if common.hue_shift > 100 {
        return Err(PxlError::usage(format!(
            "--hue-shift {} is outside 0..=100",
            common.hue_shift
        ))
        .into());
    }
    Ok(EnhanceOptions {
        scale: common.scale,
        upscale,
        protect,
        cleanup: parse_cleanup(&common.cleanup)?,
        outline,
        light,
        shade,
        ramps: RampOptions {
            levels: common.levels,
            hue_shift: common.hue_shift,
            max_colors: common.max_colors.clamp(2, 256),
            generate: true,
        },
        materials: None,
        background: Rgba::rgb(28, 26, 24),
    })
}

fn parse_cleanup(text: &str) -> CmdResult<CleanOptions> {
    match text {
        "all" => Ok(CleanOptions::ALL),
        "none" => Ok(CleanOptions::NONE),
        list => {
            let mut out = CleanOptions::NONE;
            for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                match item {
                    "orphan" | "orphans" => out.orphans = true,
                    "elbow" | "elbows" | "pixel_perfect" => out.elbows = true,
                    "notch" | "notches" => out.notches = true,
                    "rhythm" | "jaggy" => out.rhythm = true,
                    "symmetry" | "sym" => out.symmetry = true,
                    other => return Err(PxlError::usage(format!(
                        "unknown cleanup rule '{other}'"
                    ))
                    .with_hint(
                        "cleanup is all, none, or a list of orphan, elbow, notch, rhythm, symmetry",
                    )
                    .into()),
                }
            }
            Ok(out)
        }
    }
}

/// `compare` defaults to the checker so a dark outline is visible; a single
/// `enhance` keeps `auto`, which only moves off the classic backdrop when the
/// outline would vanish on it.
fn background(common: &EnhanceCommon, palette: &Palette, compare: bool) -> PxlResult<Background> {
    let default = if compare { "checker" } else { "auto" };
    let text = common.background.as_deref().unwrap_or(default);
    let zoom = common.zoom.unwrap_or(if compare { 4 } else { 1 }).max(1);
    // A checker square is `checker_cell` native pixels; the preview is scaled
    // by `scale * zoom` (compare) or `zoom` (enhance, already at full size).
    let per_native = if compare {
        zoom
    } else {
        zoom * common.scale as u32
    };
    Background::resolve(text, palette, common.checker_cell.max(1) * per_native)
}

fn load_source(input: &str, common: &EnhanceCommon) -> CmdResult<Source> {
    if input.ends_with(".png") || super::render::looks_like_png(input) {
        return load_png(input, common);
    }
    let text = util::read_text(input)?;
    let base = if input == "-" {
        std::env::current_dir().unwrap_or_default()
    } else {
        Path::new(input)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    };
    let res = DirResolver::new(&base);
    // A `.pxl.json` carries `frames` as row arrays, which `assemble` detects.
    let kind = if input.ends_with(".ascii.txt") || input.ends_with(".txt") {
        InputKind::AsciiText
    } else {
        InputKind::Auto
    };
    let req = RenderRequest {
        input: &text,
        kind,
        palette: common.palette.as_deref(),
        seed: 1,
        ..Default::default()
    };
    let (doc, palette, _, warnings) = assemble(&req, &res)?;
    let name = match &common.frame {
        Some(n) => n.clone(),
        None => {
            let names = doc.frame_names();
            if names.iter().any(|n| n == "base") {
                "base".to_string()
            } else {
                names.first().cloned().ok_or_else(|| {
                    PxlError::semantic("the input has no frames")
                        .with_hint("add a frame, or name one with --frame")
                })?
            }
        }
    };
    let frame = doc.frame(&name)?;
    Ok(Source {
        frame,
        palette,
        frame_name: name,
        warnings,
    })
}

/// A png is a preview of a native frame scaled by an integer. Find the largest
/// block size every aligned block of which is one colour, shrink back to the
/// native grid, and index the colours.
fn load_png(input: &str, common: &EnhanceCommon) -> CmdResult<Source> {
    let bytes = std::fs::read(input).map_err(|e| PxlError::io(format!("{input}: {e}")))?;
    let (w, h, rgba) = decode_png_rgba(&bytes)?;
    let block = detect_block(w, h, &rgba);
    let (nw, nh) = (w / block, h / block);
    if nw > pxl_core::MAX_DIM as u32 || nh > pxl_core::MAX_DIM as u32 {
        return Err(PxlError::semantic(format!(
            "{input} is {w}x{h} with a block size of {block}, so the native frame would be {nw}x{nh}"
        ))
        .with_hint("enhance takes native frames up to 128x128; pass a smaller png")
        .into());
    }
    let mut colors = vec![Rgba::TRANSPARENT];
    let mut frame = IndexFrame::new(nw as u16, nh as u16)?;
    for y in 0..nh {
        for x in 0..nw {
            let i = (((y * block) * w + x * block) * 4) as usize;
            let px = Rgba {
                r: rgba[i],
                g: rgba[i + 1],
                b: rgba[i + 2],
                a: if rgba[i + 3] == 0 { 0 } else { 255 },
            };
            if px.a == 0 {
                continue;
            }
            let idx = match colors.iter().position(|c| *c == px) {
                Some(p) => p,
                None => {
                    if colors.len() >= 256 {
                        return Err(PxlError::semantic(format!(
                            "{input} uses more than 255 colours"
                        ))
                        .with_hint("enhance works on palette art; reduce the colours first")
                        .into());
                    }
                    colors.push(px);
                    colors.len() - 1
                }
            };
            frame.set(x as i32, y as i32, idx as u8);
        }
    }
    // The darkest colour is the best guess for an outline when the png names none.
    let outline = (1..colors.len())
        .min_by_key(|&i| pxl_core::Oklab::from_rgba(colors[i]).l)
        .unwrap_or(1) as u8;
    let mut palette = Palette {
        id: util::stem_id(Path::new(input)),
        colors,
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    palette.special.outline = outline;
    palette.special.flash = outline;
    let mut warnings = Vec::new();
    if block > 1 {
        warnings.push(format!(
            "png is a {block}x copy of a {nw}x{nh} frame; enhancing the native frame"
        ));
    }
    let _ = common;
    Ok(Source {
        frame,
        palette,
        frame_name: "png".into(),
        warnings,
    })
}

/// Largest `b` for which `w` and `h` divide by `b` and every `b x b` block is
/// one RGBA value (alpha 0 blocks compare equal whatever their RGB).
fn detect_block(w: u32, h: u32, rgba: &[u8]) -> u32 {
    let same = |a: &[u8], b: &[u8]| -> bool {
        if a[3] == 0 && b[3] == 0 {
            true
        } else {
            a == b
        }
    };
    let limit = w.min(h).min(32);
    for b in (2..=limit).rev() {
        if w % b != 0 || h % b != 0 {
            continue;
        }
        let ok = (0..h / b).all(|by| {
            (0..w / b).all(|bx| {
                let at = |x: u32, y: u32| {
                    let i = (((by * b + y) * w + bx * b + x) * 4) as usize;
                    &rgba[i..i + 4]
                };
                let first = at(0, 0);
                (0..b).all(|y| (0..b).all(|x| same(first, at(x, y))))
            })
        });
        if ok {
            return b;
        }
    }
    1
}

fn merged_warnings(source: &Source, report: &EnhanceReport) -> Vec<String> {
    let mut out = source.warnings.clone();
    out.extend(report.warnings.iter().cloned());
    out
}

fn metrics_json(report: &EnhanceReport) -> Value {
    let mut map = serde_json::Map::new();
    for (id, value) in report.metrics.entries() {
        map.insert(id.into(), json!(value));
    }
    Value::Object(map)
}

fn clean_json(c: &CleanReport) -> Value {
    json!({
        "orphans": c.orphans,
        "elbows": c.elbows,
        "notches": c.notches,
        "rhythm": c.rhythm,
        "symmetry": c.symmetry,
    })
}

fn report_json(source: &Source, done: &Enhanced, bg: Background) -> Value {
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
fn print_metrics(rows: &[(String, &EnhanceReport)]) {
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
fn footprint(frame: &IndexFrame, zoom: u32) -> PxlResult<(u32, u32)> {
    scaled_dims(frame.w as u32, frame.h as u32, zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_block(w: u32, h: u32, b: u32, colors: &[[u8; 4]]) -> Vec<u8> {
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let c = colors[(((x / b) + (y / b)) as usize) % colors.len()];
                let i = ((y * w + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&c);
            }
        }
        out
    }

    #[test]
    fn the_largest_uniform_block_is_found() {
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 3, &[red, blue])),
            3
        );
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 1, &[red, blue])),
            1
        );
        assert_eq!(
            detect_block(12, 12, &opaque_block(12, 12, 6, &[red, blue])),
            6
        );
    }

    #[test]
    fn transparent_blocks_match_whatever_their_rgb() {
        let mut px = vec![0u8; 4 * 4 * 4];
        px[0] = 9;
        assert_eq!(detect_block(4, 4, &px), 4);
    }

    #[test]
    fn cleanup_names_are_validated() {
        assert_eq!(parse_cleanup("all").unwrap(), CleanOptions::ALL);
        assert_eq!(parse_cleanup("none").unwrap(), CleanOptions::NONE);
        let two = parse_cleanup("orphan,notch").unwrap();
        assert!(two.orphans && two.notches && !two.elbows);
        let modern = parse_cleanup("rhythm,symmetry").unwrap();
        assert!(modern.rhythm && modern.symmetry && !modern.orphans);
        assert!(parse_cleanup("pixel_perfect").unwrap().elbows);
        assert!(parse_cleanup("blur").is_err());
    }
}
