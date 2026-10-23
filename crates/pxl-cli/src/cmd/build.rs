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
use super::inspect::{checks_json, load_report};
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
                fs::rename(staging.join(name), final_dir.join(name)).map_err(|e| {
                    PxlError::io(format!("cannot publish manifest {name}: {e}"))
                })?;
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
    if ctx.lock.is_some() && !crate::util::check_lock(ctx, Some(PXL_VERSION)).is_ok() {
        util::check_lock(ctx, Some(PXL_VERSION))?;
    }
    let recipe_path = PathBuf::from(&args.recipe);
    let recipe_dir = recipe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let out_dir = out_dir.to_path_buf();
    let palette = util::load_palette(&recipe.palette, &recipe_dir)?;
    palette.check_text_encodable()?;
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
    let mut loaded = Vec::new();
    let mut outputs = vec![stored_pal_name.clone()];
    let mut copied_specs = BTreeSet::new();
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
        let doc = build_sprite(
            sprite,
            &palette,
            &stored_pal_name,
            seed,
            &recipe,
            &recipe_dir,
            &recipe_path,
        )?;
        let path = out_dir.join(format!("{}.pxl.json", sprite.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", sprite.id));
        loaded.push(Loaded {
            id: sprite.id.clone(),
            doc,
            palette: palette.clone(),
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
        let path = out_dir.join(format!("{}.pxl.json", tile.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", tile.id));
        loaded.push(Loaded {
            id: tile.id.clone(),
            doc,
            palette: palette.clone(),
        });
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
        });
    }
    let sheet_path = out_dir.join(&recipe.sheet.file);
    let (png, json_text, hash, _w, _h, _n) = pack_loaded(
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
    let report = load_report(sheet_json_path.to_str().unwrap(), rules)?;
    let checks = checks_json(&report);
    if !report.ok() {
        return Err(CmdError::new(
            PxlError::qa(format!(
                "build wrote outputs but {} QA check(s) failed",
                report.checks.iter().filter(|c| !c.pass).count()
            ))
            .with_hint("read checks and adjust the recipe"),
        )
        .with_extra(json!({"cmd": "build", "checks": checks, "hash": hash})));
    }
    let mut body = util::ok_base("build");
    body.insert("out".into(), json!(args.out));
    body.insert("outputs".into(), json!(outputs));
    body.insert("hash".into(), json!(hash));
    body.insert("checks".into(), checks);
    let manifest_name = manifest_file_name(&recipe.sheet.file);
    // Every spec the recipe reads is an input too: a changed shapes file must
    // change the manifest even when the recipe text does not.
    let mut specs = Map::new();
    for sprite in &recipe.sprites {
        if let Some(spec) = &sprite.base.spec {
            let bytes = fs::read(recipe_dir.join(spec))
                .map_err(|e| PxlError::io(format!("cannot digest spec {spec}: {e}")))?;
            specs.insert(spec.clone(), json!(util::hash_of(&bytes)));
        }
    }
    let manifest = build_manifest(
        &out_dir,
        &outputs,
        &text,
        &pal_bytes,
        specs,
        seed,
        recipe.gen_version,
        rules,
        &report,
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

/// The delivery record of one successful build: what went in (recipe, palette,
/// seed, versions), what came out (every file with its digest) and which QA
/// passed. Engineering QA and visual review are separate states; this tool
/// never claims the latter.
#[allow(clippy::too_many_arguments)]
fn build_manifest(
    out_dir: &Path,
    outputs: &[String],
    recipe_text: &str,
    palette_bytes: &[u8],
    specs: Map<String, Value>,
    seed: u64,
    gen_version: u32,
    rules: &str,
    report: &pxl_qa::Report,
) -> CmdResult<String> {
    let mut files = Map::new();
    for name in outputs {
        let bytes = fs::read(out_dir.join(name))
            .map_err(|e| PxlError::io(format!("cannot digest {name}: {e}")))?;
        files.insert(name.clone(), json!(util::hash_of(&bytes)));
    }
    let failed = report.checks.iter().filter(|c| !c.pass).count();
    let value = json!({
        "schema_version": 1,
        "pxl_version": PXL_VERSION,
        "gen_version": gen_version,
        "seed": seed,
        "inputs": {
            "recipe": util::hash_of(recipe_text.as_bytes()),
            "palette": util::hash_of(palette_bytes),
            "specs": specs,
        },
        "outputs": files,
        "qa": {
            "rules": rules,
            "ok": failed == 0,
            "checks": report.checks.len(),
            "failed": failed,
        },
        "visual_review": "not_run",
    });
    let mut text = serde_json::to_string_pretty(&value)
        .map_err(|e| PxlError::io(format!("manifest: {e}")))?;
    text.push('\n');
    Ok(text)
}
