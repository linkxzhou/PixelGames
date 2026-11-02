use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::{PxlError, PXL_VERSION};
use pxl_format::Recipe;
use pxl_gen::{generate_noise_tile, variant_seed};
use serde_json::{json, Map, Value};

use super::args::{BuildArgs, RenderArgs};
use super::document::doc_from_frames;
use super::generate::spec_file_name;
use super::inspect::{checks_json, load_qa_config, load_report_with_config};
use super::render::render_cmd;
use super::sheet::{pack_loaded, Loaded};
use super::sprite::{build_sprite, overwrite_ctx};
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

pub(super) const STAGING_DIR: &str = ".staging";

pub(super) const REJECTED_DIR: &str = ".rejected";

/// Build into `<out>/.staging`, run QA there, and only then publish. A build
/// that fails at any point leaves the files already in `<out>` untouched; the
/// failed attempt is kept under `<out>/.rejected` for diagnosis. Each published
/// file replaces its predecessor atomically (rename), but publishing several
/// files is not one transaction. `<sheet>.build.json` is the commit record: it
/// is removed first and written last, so whenever it exists every file it
/// lists carries the digest it records.
pub(super) fn build_cmd(ctx: &Ctx, args: BuildArgs) -> CmdResult<()> {
    let final_dir = PathBuf::from(&args.out);
    if final_dir.exists() && !final_dir.is_dir() {
        return Err(
            PxlError::semantic(format!("{} is not a directory", final_dir.display()))
                .with_hint("pxl build -o takes a directory")
                .into(),
        );
    }
    let staging = final_dir.join(STAGING_DIR);
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| PxlError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| PxlError::io(format!("cannot create output dir: {e}")))?;
    match build_into(ctx, &args, &staging) {
        Ok(body) => {
            // The manifest is the bundle's commit record: drop the old one
            // before replacing files and publish the new one last, so a crash
            // mid-publish leaves no manifest claiming a mixed set of files.
            let manifest = body
                .get("manifest")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(name) = &manifest {
                let current = final_dir.join(name);
                if current.exists() {
                    fs::remove_file(&current).map_err(|e| {
                        PxlError::io(format!("cannot retire {}: {e}", current.display()))
                    })?;
                }
            }
            publish_tree(&staging, &final_dir, manifest.as_deref())?;
            if let Some(name) = &manifest {
                fs::rename(staging.join(name), final_dir.join(name))
                    .map_err(|e| PxlError::io(format!("cannot publish manifest {name}: {e}")))?;
            }
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir_all(final_dir.join(REJECTED_DIR));
            util::emit_ok(ctx, &Value::Object(body));
            Ok(())
        }
        Err(mut err) => {
            let rejected = final_dir.join(REJECTED_DIR);
            let _ = fs::remove_dir_all(&rejected);
            let kept = fs::read_dir(&staging)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
                && fs::rename(&staging, &rejected).is_ok();
            if !kept {
                let _ = fs::remove_dir_all(&staging);
            } else if let Some(Value::Object(extra)) = err.extra.as_mut() {
                extra.insert("rejected".into(), json!(rejected.to_string_lossy()));
            } else {
                err.extra = Some(json!({"rejected": rejected.to_string_lossy()}));
            }
            Err(err)
        }
    }
}

/// Move every file under `src` into `dst`, replacing what is there. A top-level
/// file named `hold` stays in `src` so the caller can publish it last.
pub(super) fn publish_tree(src: &Path, dst: &Path, hold: Option<&str>) -> CmdResult<()> {
    fs::create_dir_all(dst)
        .map_err(|e| PxlError::io(format!("cannot create {}: {e}", dst.display())))?;
    let entries = fs::read_dir(src)
        .map_err(|e| PxlError::io(format!("cannot read {}: {e}", src.display())))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            publish_tree(&from, &to, None)?;
        } else if hold.is_some_and(|h| entry.file_name().to_str() == Some(h)) {
            continue;
        } else {
            fs::rename(&from, &to)
                .map_err(|e| PxlError::io(format!("cannot publish {}: {e}", to.display())))?;
        }
    }
    Ok(())
}

pub(super) fn build_into(
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
    let visual = super::visual::VisualContext::for_recipe(&recipe_path)?;
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
                     visual: Option<&super::visual::VisualContext>|
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
                include_str!("../../../../assets/palettes/tank.palette.json")
                    .as_bytes()
                    .to_vec()
            }
            _ => include_str!("../../../../assets/palettes/p16.palette.json")
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
        let palette_bytes = super::visual::palette_bytes(&final_palette)?;
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
        outputs.extend(super::visual_artifacts::write_visual_artifacts(
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
    let specs = super::dependencies::recipe_dependencies(&recipe_dir, &recipe_path)?
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

/// `units.sheet.png` -> `units.build.json`. One manifest per sheet, so several
/// recipes may share an output directory without overwriting each other's.
pub(super) fn manifest_file_name(sheet_file: &str) -> String {
    let name = Path::new(sheet_file)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png");
    let stem = name
        .strip_suffix(".png")
        .unwrap_or(name)
        .trim_end_matches(".sheet");
    format!("{stem}.build.json")
}

/// Read the `resources` block the sheet json recorded and bind it to the
/// manifest: each entry gains the sheet file name and the asset's pxl document
/// reference (source_digest stays the digest of that document). Assets the
/// sheet did not place are dropped, keeping the manifest and the sheet json
/// in exact agreement.
fn sheet_resources(sheet_json_path: &Path, ordered: &[Loaded]) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(sheet_json_path) else {
        return Vec::new();
    };
    let Ok(sheet) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    // The png name is the json name with the sheet extension swapped back:
    // the resource points at the packed image, `sheet_json` at its metadata.
    let sheet_file = sheet_json_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.trim_end_matches(".json").to_string())
        .unwrap_or_else(|| "sheet.png".into())
        + ".png";
    let sheet_json = sheet_json_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.json")
        .to_string();
    // `sheet_json` nests the pxl block (and with it `resources`) under
    // `meta.pxl`; `resources` lives there, never at the document root.
    let Some(entries) = sheet
        .pointer("/meta/pxl/resources")
        .and_then(Value::as_array)
        else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let Some(asset_id) = entry.get("asset_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(asset) = ordered.iter().find(|loaded| loaded.id == asset_id) else {
            continue;
        };
        let mut bound = entry.clone();
        bound["sheet"] = json!(sheet_file);
        bound["sheet_json"] = json!(sheet_json);
        bound["document"] = json!(format!("{asset_id}.pxl.json"));
        bound["source_digest"] = json!(util::hash_of(
            pxl_format::to_pretty(&asset.doc)
                .unwrap_or_default()
                .as_bytes()
        ));
        out.push(bound);
    }
    out
}

/// The delivery record of one successful build: what went in (recipe, palette,
/// seed, versions), what came out (every file with its digest) and which QA
/// passed. Engineering QA and visual review are separate states; this tool
/// never claims the latter.
///
/// schema_version 3 (P04) adds the `resources` block: one stable resource id
/// per built asset with its palette family, source digest and sheet/frame
/// reference, so consumers address assets by a stable id instead of paths.
/// The sheet/frame names come from the sheet json that `pack_loaded` wrote.
#[allow(clippy::too_many_arguments)]
fn build_manifest(
    out_dir: &Path,
    outputs: &[String],
    recipe_text: &str,
    palette_bytes: &[u8],
    specs: Map<String, Value>,
    seed: u64,
    gen_version: u32,
    config: &pxl_qa::QaConfig,
    report: &pxl_qa::Report,
    resources: Vec<Value>,
) -> CmdResult<String> {
    let mut files = Map::new();
    for name in outputs {
        let bytes = fs::read(out_dir.join(name))
            .map_err(|e| PxlError::io(format!("cannot digest {name}: {e}")))?;
        files.insert(name.clone(), json!(util::hash_of(&bytes)));
    }
    let failed = report.checks.iter().filter(|c| c.failed()).count();
    let closure =
        serde_json::to_vec(&specs).map_err(|e| PxlError::io(format!("input closure: {e}")))?;
    let value = json!({
        "schema_version": 3,
        "pxl_version": PXL_VERSION,
        "gen_version": gen_version,
        "seed": seed,
        "algorithm_version": pxl_core::GEN_VERSION,
        "inputs": {
            "recipe": util::hash_of(recipe_text.as_bytes()),
            "palette": util::hash_of(palette_bytes),
            "specs": specs,
            "closure_digest": util::hash_of(&closure),
            "capability_digest": pxl_format::capability_digest(),
        },
        "outputs": files,
        "resources": resources,
        "qa": {
            "rules": report.rules,
            "config": config,
            "ok": failed == 0,
            "checks": report.checks.len(),
            "failed": failed,
        },
        "visual_review": "not_run",
    });
    let mut text =
        serde_json::to_string_pretty(&value).map_err(|e| PxlError::io(format!("manifest: {e}")))?;
    text.push('\n');
    Ok(text)
}

fn validate_built_doc(
    doc: &pxl_format::PxlDoc,
    palette: &pxl_core::Palette,
    sprite: Option<&pxl_format::RecipeSprite>,
    custom: Option<&pxl_qa::QaConfig>,
    strict: bool,
    thresholds: Option<&pxl_contract::policy::QaThresholds>,
) -> CmdResult<pxl_qa::Report> {
    let mut config = custom.cloned().unwrap_or_else(|| {
        // The effective policy wins over tier defaults whenever the visual
        // contract carries one: generation and QA share the same budgets.
        if let Some(thresholds) = thresholds {
            let mut config = pxl_qa::QaConfig::for_policy(thresholds);
            if doc.w >= 32 && doc.h >= 32 {
                if let Ok(profile) = pxl_core::SizeProfile::for_size(doc.w, doc.h) {
                    let tier = pxl_qa::QaConfig::for_profile(&profile);
                    config.max_components = tier.max_components;
                    config.max_isolated_pixels = tier.max_isolated_pixels;
                    config.max_color_specks = tier.max_color_specks;
                    config.min_content_coverage_bps = tier.min_content_coverage_bps;
                    config.copy2x_warn_bps = tier.copy2x_warn_bps;
                }
            }
            config
        } else if doc.w >= 32 && doc.h >= 32 {
            pxl_core::SizeProfile::for_size(doc.w, doc.h)
                .map(|profile| pxl_qa::QaConfig::for_profile(&profile))
                .unwrap_or_default()
        } else {
            pxl_qa::QaConfig::default()
        }
    });
    config.strict |= strict;
    let mut input = super::qa_input::doc_input(doc, palette)?;
    if let Some(sprite) = sprite {
        super::qa_input::apply_recipe_contract(&mut input, sprite)?;
    }
    Ok(pxl_qa::validate_with_config(&input, &config)?)
}
