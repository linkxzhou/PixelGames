use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::{PxlError, PXL_VERSION};
use pxl_format::Recipe;
use pxl_gen::{generate_noise_tile, variant_seed};
use serde_json::{json, Map, Value};

use crate::cmd::args::{BuildArgs, RenderArgs};
use crate::cmd::document::doc_from_frames;
use crate::cmd::generate::spec_file_name;
use crate::cmd::inspect::{checks_json, load_qa_config, load_report_with_config};
use crate::cmd::render::render_cmd;
use crate::cmd::sheet::{pack_loaded, Loaded};
use crate::cmd::sprite::{build_sprite, overwrite_ctx};
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

use super::manifest::{build_manifest, manifest_file_name, sheet_resources, validate_built_doc};

pub(crate) fn build_into(
    ctx: &Ctx,
    args: &BuildArgs,
    out_dir: &Path,
) -> CmdResult<Map<String, Value>> {
    util::check_lock(ctx, None)?;
    let text = util::read_text(&args.recipe)?;
    let recipe: Recipe = pxl_format::parse_json(&text, "recipe")?;
    if recipe.gen_version != pxl_core::GEN_VERSION {
        return Err(PxlError::usage(format!(
            "unsupported gen_version {}; expected {}",
            recipe.gen_version,
            pxl_core::GEN_VERSION
        ))
        .into());
    }
    let qa_config = load_qa_config(args.qa_config.as_deref())?;
    if ctx.lock.is_some() && !crate::util::check_lock(ctx, Some(PXL_VERSION)).is_ok() {
        util::check_lock(ctx, Some(PXL_VERSION))?;
    }
    let recipe_path = PathBuf::from(&args.recipe);
    let recipe_dir = recipe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let out_dir = out_dir.to_path_buf();
    let visual = crate::cmd::visual::VisualContext::for_recipe(&recipe_path)?;
    // One threshold source for dry-run parity and the real build.
    let thresholds = visual.as_ref().map(|context| context.qa_thresholds());
    let palette = util::load_palette(&recipe.palette, &recipe_dir)?;
    palette.check_text_encodable()?;
    if let Some(context) = visual.as_ref() {
        context.check_palette_budget(&palette)?;
    }
    // P04: when the recipe carries a visual contract with families, every
    // sprite it builds must use its family's palette. Tile-only recipes have
    // no single asset id; the per-asset check runs per sprite below.
    let mut loaded = Vec::new();
    let family_of = |asset_id: &str,
                     visual: Option<&crate::cmd::visual::VisualContext>|
     -> CmdResult<Option<String>> {
        let Some(context) = visual else {
            return Ok(None);
        };
        context.check_recipe_family(asset_id, &recipe.palette, &palette)?;
        Ok(context
            .asset_family(asset_id)
            .map(|family| family.to_string()))
    };
    let pal_name = Path::new(&recipe.palette)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("palette.json")
        .to_string();
    let pal_src = if recipe_dir.join(&recipe.palette).is_file() {
        recipe_dir.join(&recipe.palette)
    } else {
        PathBuf::from(&recipe.palette)
    };
    let pal_bytes = if pal_src.is_file() {
        fs::read(&pal_src).map_err(|e| PxlError::io(format!("palette: {e}")))?
    } else {
        // Built-in palette: write canonical JSON.
        let file_text = match pal_name.as_str() {
            "tank.palette.json" | "tank" => {
                include_str!("../../../../../assets/palettes/tank.palette.json")
                    .as_bytes()
                    .to_vec()
            }
            _ => include_str!("../../../../../assets/palettes/p16.palette.json")
                .as_bytes()
                .to_vec(),
        };
        file_text
    };
    let stored_pal_name = if pal_name.ends_with(".json") {
        pal_name.clone()
    } else {
        format!("{pal_name}.palette.json")
    };
    util::write_out(
        out_dir.join(&stored_pal_name).to_str().unwrap(),
        &pal_bytes,
        true,
        false,
    )?;
    let seed = ctx.seed.unwrap_or(recipe.seed);
    let mut outputs = vec![stored_pal_name.clone()];
    let mut copied_specs = BTreeSet::new();
    let mut profile_reports = Vec::new();
    let mut final_palette = palette.clone();
    let mut visual_reports = Vec::new();
    for sprite in &recipe.sprites {
        if sprite.base.gen == "parts" {
            if let Some(spec) = &sprite.base.spec {
                let name = spec_file_name(spec);
                if copied_specs.insert(name.clone()) {
                    let src = recipe_dir.join(spec);
                    let bytes = fs::read(&src).map_err(|e| {
                        PxlError::io(format!("cannot read parts {}: {e}", src.display()))
                    })?;
                    util::write_out(out_dir.join(&name).to_str().unwrap(), &bytes, true, false)?;
                    outputs.push(name);
                }
            }
        }
    }
    for sprite in &recipe.sprites {
        let built = build_sprite(
            sprite,
            &palette,
            &stored_pal_name,
            seed,
            &recipe,
            &recipe_dir,
            &recipe_path,
            visual.as_ref(),
        )?;
        let doc = built.doc;
        final_palette = built.palette;
        if let Some(report) = built.visual {
            visual_reports.push(report);
        }
        profile_reports.push(validate_built_doc(
            &doc,
            &final_palette,
            Some(sprite),
            qa_config.as_ref(),
            ctx.strict,
            thresholds.as_ref(),
        )?);
        let path = out_dir.join(format!("{}.pxl.json", sprite.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", sprite.id));
        let family = family_of(&sprite.id, visual.as_ref())?;
        loaded.push(Loaded {
            id: sprite.id.clone(),
            doc,
            palette: final_palette.clone(),
            family,
        });
    }
    for tile in &recipe.tiles {
        let mut pairs = Vec::new();
        let n = tile.variants.max(1);
        for i in 0..n {
            let frame = generate_noise_tile(
                &tile.kind,
                tile.size,
                variant_seed(ctx.seed.unwrap_or(tile.seed), i),
                &palette,
                tile.ramp.as_deref(),
            )?;
            pairs.push((format!("tile_{i}"), frame));
        }
        let mut doc = doc_from_frames(&stored_pal_name, &pairs, tile.seed, "tiles")?;
        doc.meta.asset = Some(tile.id.clone());
        if let Some(ramp) = &tile.ramp {
            doc.meta.tile_ramp = Some(ramp.clone());
        }
        doc.meta.recipe = Some(
            recipe_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("recipe")
                .into(),
        );
        if let Some(context) = &visual {
            let application = context.apply(&mut doc, &palette, &recipe_dir)?;
            final_palette = application.palette;
            visual_reports.push(application.report);
        }
        profile_reports.push(validate_built_doc(
            &doc,
            &final_palette,
            None,
            qa_config.as_ref(),
            ctx.strict,
            thresholds.as_ref(),
        )?);
        let path = out_dir.join(format!("{}.pxl.json", tile.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", tile.id));
        let family = family_of(&tile.id, visual.as_ref())?;
        loaded.push(Loaded {
            id: tile.id.clone(),
            doc,
            palette: final_palette.clone(),
            family,
        });
    }
    if visual.is_some() {
        let palette_bytes = crate::cmd::visual::palette_bytes(&final_palette)?;
        util::write_out(
            out_dir.join(&stored_pal_name).to_str().unwrap(),
            &palette_bytes,
            true,
            false,
        )?;
        let effective = visual
            .as_ref()
            .map(|context| context.effective_style(visual_reports.clone()))
            .unwrap_or(Value::Null);
        util::write_out(
            out_dir.join("effective-style.json").to_str().unwrap(),
            serde_json::to_string_pretty(&effective)
                .map_err(|e| PxlError::io(format!("effective-style.json: {e}")))?
                .as_bytes(),
            true,
            false,
        )?;
        outputs.push("effective-style.json".into());
    }
    let order: Vec<String> = if recipe.sheet.sources.is_empty() {
        loaded.iter().map(|d| d.id.clone()).collect()
    } else {
        recipe.sheet.sources.clone()
    };
    let mut ordered = Vec::new();
    for id in &order {
        let doc = loaded
            .iter()
            .find(|d| d.id == *id)
            .ok_or_else(|| PxlError::semantic(format!("sheet source '{id}' was not built")))?;
        ordered.push(Loaded {
            id: doc.id.clone(),
            doc: doc.doc.clone(),
            palette: doc.palette.clone(),
            family: doc.family.clone(),
        });
    }
    let sheet_path = out_dir.join(&recipe.sheet.file);
    let (png, json_text, hash, _w, _h, _n, budget) = pack_loaded(
        &ordered,
        &recipe.sheet.layout,
        recipe.sheet.pad,
        sheet_path.to_str().unwrap_or("sheet.png"),
    )?;
    util::write_out(sheet_path.to_str().unwrap(), &png, true, false)?;
    let sheet_json_path = sheet_path.with_extension("json");
    util::write_out(
        sheet_json_path.to_str().unwrap(),
        json_text.as_bytes(),
        true,
        false,
    )?;
    outputs.push(recipe.sheet.file.clone());
    outputs.push(
        sheet_json_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sheet.json")
            .to_string(),
    );
    for preview in &recipe.preview {
        let dest = out_dir.join(&preview.file);
        if preview.sheet == Some(true) {
            render_cmd(
                &overwrite_ctx(ctx),
                RenderArgs {
                    input: sheet_path.to_string_lossy().into_owned(),
                    frame: None,
                    anim: None,
                    dir: None,
                    scale: preview.scale,
                    grid: preview.grid,
                    asset: false,
                    out: dest.to_string_lossy().into_owned(),
                },
            )?;
        } else {
            let source = preview.source.clone().unwrap_or_else(|| {
                recipe
                    .sprites
                    .first()
                    .map(|s| s.id.clone())
                    .unwrap_or_default()
            });
            let doc_path = out_dir.join(format!("{source}.pxl.json"));
            render_cmd(
                &overwrite_ctx(ctx),
                RenderArgs {
                    input: doc_path.to_string_lossy().into_owned(),
                    frame: None,
                    anim: preview.anim.clone(),
                    dir: preview.dir.clone(),
                    scale: preview.scale,
                    grid: preview.grid,
                    asset: false,
                    out: dest.to_string_lossy().into_owned(),
                },
            )?;
        }
        outputs.push(preview.file.clone());
    }
    let rules = recipe
        .validate
        .as_ref()
        .map(|v| v.rules.as_str())
        .unwrap_or("default");
    let rules = if ctx.strict && rules == "default" {
        "strict"
    } else {
        rules
    };
    let mut qa_config = qa_config;
    if let Some(config) = &mut qa_config {
        config.strict |= ctx.strict;
    }
    let report =
        load_report_with_config(sheet_json_path.to_str().unwrap(), rules, qa_config.as_ref())?;
    let effective_config = qa_config.unwrap_or_else(|| pxl_qa::QaConfig {
        strict: rules == "strict",
        ..pxl_qa::QaConfig::default()
    });
    let mut checks = checks_json(&report);
    let mut failed = report.checks.iter().filter(|c| c.failed()).count();
    for profile in &profile_reports {
        failed += profile.checks.iter().filter(|c| c.failed()).count();
        if let Some(items) = checks_json(profile).as_array().cloned() {
            let output = checks.as_array_mut().unwrap();
            for item in items {
                if let Some(existing) = output
                    .iter_mut()
                    .find(|existing| existing["id"] == item["id"])
                {
                    if item["pass"] == false {
                        *existing = item;
                    }
                } else {
                    output.push(item);
                }
            }
        }
    }
    if failed > 0 {
        return Err(CmdError::new(
            PxlError::qa(format!(
                "build wrote outputs but {failed} QA check(s) failed"
            ))
            .with_hint("read checks and adjust the recipe"),
        )
        .with_extra(json!({"cmd": "build", "checks": checks, "hash": hash})));
    }
    if visual.is_some() {
        outputs.extend(crate::cmd::visual_artifacts::write_visual_artifacts(
            &out_dir,
            &recipe_dir,
            &loaded,
            visual.as_ref(),
            visual_reports.clone(),
        )?);
    }
    let mut body = util::ok_base("build");
    body.insert("out".into(), json!(args.out));
    body.insert("outputs".into(), json!(outputs));
    body.insert("hash".into(), json!(hash));
    body.insert("checks".into(), checks);
    // P08: the whole-atlas RGBA8 budget for this recipe's single sheet.
    body.insert(
        "budget".into(),
        serde_json::to_value(budget).unwrap_or(Value::Null),
    );
    let manifest_name = manifest_file_name(&recipe.sheet.file);
    // The closure is shared with dry-run and includes transitive source files.
    let specs = crate::cmd::dependencies::recipe_dependencies(&recipe_dir, &recipe_path)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    // P04: collect the stable resource references the sheet json recorded so
    // the manifest carries them without re-deriving placement.
    let resources = sheet_resources(&sheet_json_path, &ordered);
    let manifest = build_manifest(
        &out_dir,
        &outputs,
        &text,
        &pal_bytes,
        specs,
        seed,
        recipe.gen_version,
        &effective_config,
        &report,
        resources,
    )?;
    util::write_out(
        out_dir.join(&manifest_name).to_str().unwrap(),
        manifest.as_bytes(),
        true,
        false,
    )?;
    body.insert("manifest".into(), json!(manifest_name));
    Ok(body)
}
