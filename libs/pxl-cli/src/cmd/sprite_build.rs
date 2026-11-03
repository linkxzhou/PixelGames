//! `pxl sprite build`: compile a v3 sprite spec into a document plus preview.
//! `pxl profile show`: print the defaults a size tier uses.

use std::path::Path;

use pxl_core::{PxlError, SizeProfile, SizeTier};
use pxl_engine::build_one;
use pxl_export::render_preview_png;
use serde_json::{json, Value};

use super::args::{ProfileArgs, SpriteBuildArgs};
use crate::error::CmdResult;
use crate::util::{self, Ctx, DirResolver};

pub(super) fn sprite_build(ctx: &Ctx, args: SpriteBuildArgs) -> CmdResult<()> {
    use std::collections::BTreeMap;

    let input = Path::new(&args.input);
    let base = input.parent().unwrap_or_else(|| Path::new("."));
    let res = DirResolver::new(base);
    let text = util::read_text(&args.input)?;
    let spec = pxl_format::parse_sprite_spec(&text)?;
    let out_dir = Path::new(&args.out)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let stem = Path::new(&args.out)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sprite")
        .to_string();
    let doc_name = args
        .doc
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{stem}.pxl.json"));
    let mut sizes = vec![(spec.size, spec.height(), stem.clone())];
    sizes.extend(
        spec.variants
            .iter()
            .map(|size| (*size, *size, format!("{stem}@{size}"))),
    );
    let mut prepared = Vec::new();
    let mut all_warnings = Vec::new();
    let mut all_checks = Vec::new();
    let mut qa_errors = Vec::new();
    let mut expected_frames = None;
    for (w, h, name) in sizes {
        let built = build_one(&spec, w, h, args.palette.as_deref(), &res)?;
        let mut doc = built.doc.clone();
        if args.palette.is_none() {
            doc.palette = format!("{name}.palette.json");
        }
        let profile = SizeProfile::for_size(w, h)?;
        let mut config = pxl_qa::QaConfig::for_profile(&profile);
        config.strict = ctx.strict;
        let qa_input = super::qa_input::doc_input(&doc, &built.palette)?;
        let report = pxl_qa::validate_with_config(&qa_input, &config)?;
        all_warnings.extend(built.warnings.iter().cloned());
        all_warnings.extend(
            report
                .checks
                .iter()
                .filter(|check| !check.pass && check.severity != pxl_qa::Severity::Error)
                .map(|check| format!("{name}:{}", check.id)),
        );
        all_checks.extend(report.checks.iter().map(|check| {
            json!({
                "id": format!("{name}:{}", check.id),
                "pass": check.pass,
                "severity": check.severity,
            })
        }));
        let names = doc.frame_names();
        let frames_ok = expected_frames
            .as_ref()
            .is_none_or(|expected| expected == &names);
        if expected_frames.is_none() {
            expected_frames = Some(names);
        }
        let anchor_ok = if let Some(expected) = spec.anchor_bp {
            let frame = doc
                .frame("base")
                .or_else(|_| doc.frame(&doc.frame_names()[0]))?;
            normalized_anchor(&frame).is_some_and(|actual| {
                let tolerance = spec.anchor_tolerance_bp.unwrap_or(0);
                actual[0].abs_diff(expected[0]) <= tolerance
                    && actual[1].abs_diff(expected[1]) <= tolerance
            })
        } else {
            true
        };
        all_checks.push(json!({
            "id": format!("{name}:variant.contract"),
            "pass": frames_ok && anchor_ok,
            "severity": "error",
            "expected": {"frames": expected_frames, "anchor_bp": spec.anchor_bp},
        }));
        if !report.ok() || !frames_ok || !anchor_ok {
            qa_errors.push(name.clone());
        }
        prepared.push((name, built, doc, profile));
    }
    all_warnings.sort();
    all_warnings.dedup();
    util::guard_warnings(ctx, &all_warnings)?;
    if !qa_errors.is_empty() {
        return Err(crate::error::CmdError::new(PxlError::qa(format!(
            "sprite size(s) failed QA: {}",
            qa_errors.join(", ")
        )))
        .with_extra(json!({"checks": all_checks, "warnings": all_warnings})));
    }

    let mut outputs = BTreeMap::new();
    let mut variants = Vec::new();
    let mut body = util::ok_base("sprite");
    for (index, (name, built, doc, profile)) in prepared.iter().enumerate() {
        let doc_text = pxl_format::to_pretty(doc)?;
        outputs.insert(format!("{name}.pxl.json"), doc_text.into_bytes());
        if args.palette.is_none() {
            let palette_text = pxl_format::to_pretty(&palette_file(&built.palette, name))?;
            outputs.insert(format!("{name}.palette.json"), palette_text.into_bytes());
        }
        let frame_name = doc
            .frame_names()
            .into_iter()
            .find(|n| n == "base")
            .or_else(|| doc.frame_names().first().cloned())
            .ok_or_else(|| PxlError::semantic("the spec produced no frames"))?;
        let frame = doc.frame(&frame_name)?;
        let zoom = if args.zoom == 0 {
            profile.preview_scale
        } else {
            args.zoom.min(profile.preview_scale)
        };
        let png = render_preview_png(&frame, &built.palette, zoom, false)?;
        outputs.insert(format!("{name}.png"), png);
        if index == 0 {
            outputs.insert(
                format!("{doc_name}"),
                pxl_format::to_pretty(doc)?.into_bytes(),
            );
            body.insert("out".into(), json!(args.out));
            body.insert(
                "doc".into(),
                json!(out_dir.join(&doc_name).to_string_lossy()),
            );
            body.insert("palette".into(), json!(doc.palette));
            body.insert("size".into(), json!([doc.w, doc.h]));
            body.insert("profile".into(), json!(profile.tier.name()));
            body.insert("zoom".into(), json!(zoom));
            body.insert("frame".into(), json!(frame_name));
            body.insert("colors".into(), json!(built.palette.colors.len()));
        } else {
            variants.push(json!({
                "size": doc.w,
                "doc": out_dir.join(format!("{name}.pxl.json")).to_string_lossy(),
                "out": out_dir.join(format!("{name}.png")).to_string_lossy(),
                "colors": built.palette.colors.len(),
                "warnings": built.warnings,
            }));
        }
    }
    let output_digests = outputs
        .iter()
        .map(|(name, bytes)| (name.clone(), json!(util::hash_of(bytes))))
        .collect::<serde_json::Map<_, _>>();
    let manifest_name = format!("{stem}.sprite.build.json");
    let manifest = json!({
        "schema_version": 2,
        "pxl_version": pxl_core::PXL_VERSION,
        "gen_version": pxl_core::GEN_VERSION,
        "inputs": {
            "spec": util::hash_of(text.as_bytes()),
            "dependencies": super::dependencies::sprite_dependencies(base, input)?,
            "palette_override": args.palette,
        },
        "outputs": output_digests,
        "qa": {"strict": ctx.strict, "checks": all_checks},
        "visual_review": "not_run",
    });
    outputs.insert(
        manifest_name.clone(),
        pxl_format::to_pretty(&manifest)?.into_bytes(),
    );

    let staging = out_dir.join(super::build::STAGING_DIR);
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| PxlError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| PxlError::io(format!("cannot create {}: {e}", staging.display())))?;
    let write_ctx = super::sprite::overwrite_ctx(ctx);
    for (name, bytes) in &outputs {
        util::write_out(
            staging.join(name).to_str().unwrap(),
            bytes,
            true,
            write_ctx.json,
        )?;
    }
    super::build::publish_tree(&staging, out_dir, Some(&manifest_name))?;
    std::fs::rename(staging.join(&manifest_name), out_dir.join(&manifest_name))
        .map_err(|e| PxlError::io(format!("cannot publish {manifest_name}: {e}")))?;
    let _ = std::fs::remove_dir_all(&staging);
    body.insert("manifest".into(), json!(manifest_name));
    body.insert("variants".into(), Value::Array(variants));
    body.insert("warnings".into(), json!(all_warnings));
    body.insert("checks".into(), json!(all_checks));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn normalized_anchor(frame: &pxl_core::IndexFrame) -> Option<[u32; 2]> {
    let mut min_x = frame.w as u32;
    let mut max_x = 0u32;
    let mut bottom = None;
    for y in 0..frame.h as u32 {
        for x in 0..frame.w as u32 {
            if frame.get(x as i32, y as i32) != 0 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                bottom = Some(y);
            }
        }
    }
    bottom.map(|bottom| {
        let center = (min_x + max_x) / 2;
        [
            center.saturating_mul(10000) / (frame.w as u32).max(1),
            bottom.saturating_mul(10000) / (frame.h as u32).max(1),
        ]
    })
}

fn palette_file(palette: &pxl_core::Palette, id: &str) -> pxl_format::PaletteFile {
    use std::collections::BTreeMap;
    pxl_format::PaletteFile {
        v: 1,
        id: id.to_string(),
        colors: palette.colors.iter().map(|c| c.to_hex()).collect(),
        ramps: palette
            .ramps
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
        cycles: Default::default(),
        special: pxl_format::SpecialFile {
            transparent: palette.special.transparent,
            outline: palette.special.outline,
            flash: palette.special.flash,
        },
    }
}

pub(super) fn profile_show(ctx: &Ctx, args: ProfileArgs) -> CmdResult<()> {
    let tiers: Vec<SizeTier> = match &args.tier {
        Some(text) => vec![SizeTier::parse(text)?],
        None => SizeTier::ALL.to_vec(),
    };
    let list: Vec<Value> = tiers
        .iter()
        .map(|t| {
            let p = SizeProfile::of(*t);
            json!({
                "tier": p.tier.name(),
                "side": p.side(),
                "max_colors": p.max_colors,
                "ramp_levels": p.ramp_levels,
                "outline": p.outline.name(),
                "aa": p.aa.name(),
                "shade": p.shade,
                "dither": p.dither,
                "rhythm": p.rhythm,
                "svg_samples": p.svg_samples,
                "min_feature": p.min_feature,
                "preview_scale": p.preview_scale,
                "qa": {
                    "max_colors": p.qa_max_colors,
                    "max_components": p.qa_max_components,
                    "max_isolated_pixels": p.qa_max_isolated,
                    "max_color_specks": p.qa_max_specks,
                    "min_content_coverage_bps": p.qa_min_content_bps,
                    "copy2x_warn_bps": p.qa_copy2x_warn_bps,
                },
                "ascii": {
                    "rows": p.side() <= 64,
                    "rle": true,
                    "tiles": p.side() >= 128,
                },
            })
        })
        .collect();
    let mut body = util::ok_base("profile");
    if list.len() == 1 {
        body.insert("profile".into(), list[0].clone());
    } else {
        body.insert("profiles".into(), Value::Array(list.clone()));
    }
    if ctx.json {
        util::emit_ok(ctx, &Value::Object(body));
        return Ok(());
    }
    println!(
        "{:<6}{:>6}{:>7}{:>7}{:>16}{:>6}{:>7}{:>6}{:>9}{:>9}",
        "tier", "side", "colors", "ramps", "outline", "aa", "shade", "dith", "svg smp", "preview"
    );
    for v in &list {
        println!(
            "{:<6}{:>6}{:>7}{:>7}{:>16}{:>6}{:>7}{:>6}{:>9}{:>9}",
            v["tier"].as_str().unwrap_or(""),
            v["side"],
            v["max_colors"],
            v["ramp_levels"],
            v["outline"].as_str().unwrap_or(""),
            v["aa"].as_str().unwrap_or(""),
            v["shade"],
            v["dither"],
            v["svg_samples"],
            v["preview_scale"],
        );
    }
    Ok(())
}
