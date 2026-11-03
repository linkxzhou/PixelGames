use std::fs;
use std::path::Path;

use pxl_core::{PxlError, PXL_VERSION};
use serde_json::{json, Map, Value};

use crate::cmd::sheet::Loaded;
use crate::error::CmdResult;
use crate::util::{self};

pub(crate) fn manifest_file_name(sheet_file: &str) -> String {
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
pub(crate) fn sheet_resources(sheet_json_path: &Path, ordered: &[Loaded]) -> Vec<Value> {
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
pub(crate) fn build_manifest(
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

pub(crate) fn validate_built_doc(
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
    let mut input = crate::cmd::qa_input::doc_input(doc, palette)?;
    if let Some(sprite) = sprite {
        crate::cmd::qa_input::apply_recipe_contract(&mut input, sprite)?;
    }
    Ok(pxl_qa::validate_with_config(&input, &config)?)
}
