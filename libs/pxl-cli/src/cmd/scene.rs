//! `pxl scene compile`: expand a module tree into a deterministic
//! compiled-scene/v1 file.
//!
//! Library layout (all JSON, nothing executable):
//!
//! ```text
//! <library>/tileset/<id>.json    tileset/v1
//! <library>/module/<id>.json     module/v1
//! <library>/kit/<id>.json        scene-kit/v1
//! ```
//!
//! The compiler never reads files outside the library directory and never
//! executes a recipe: instances reference compiled resource ids only.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::prototype::{ReviewState, SceneKit};
use pxl_contract::tileset::{Module, Tileset};
use pxl_core::PxlError;
use pxl_gen::{compile_scene, KitPlacement, KitResolver, ModuleLibrary, SceneBudget};
use serde_json::{json, Value};

use super::args::{SceneCmd, SceneCompileArgs};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn scene_cmd(ctx: &Ctx, action: SceneCmd) -> CmdResult<()> {
    match action {
        SceneCmd::Compile(args) => compile_cmd(ctx, args),
    }
}

struct SceneLibrary {
    tilesets: BTreeMap<String, Tileset>,
    modules: BTreeMap<String, Module>,
    kits: BTreeMap<String, SceneKit>,
    problems: Vec<String>,
}

fn read_json_dir<T: serde::de::DeserializeOwned>(
    dir: &Path,
    what: &str,
    problems: &mut Vec<String>,
) -> BTreeMap<String, T> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        match fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
            serde_json::from_slice::<T>(&b).map_err(|e| e.to_string())
        }) {
            Ok(value) => {
                out.insert(stem, value);
            }
            Err(e) => problems.push(format!("{what} '{stem}': {e}")),
        }
    }
    out
}

fn load_scene_library(dir: &str) -> CmdResult<SceneLibrary> {
    let root = PathBuf::from(dir);
    if !root.is_dir() {
        return Err(PxlError::usage(format!("{dir} is not a directory"))
            .with_hint("--library takes the directory that holds tileset/ and module/")
            .into());
    }
    let mut problems = Vec::new();
    let tilesets = read_json_dir::<Tileset>(&root.join("tileset"), "tileset", &mut problems);
    let modules = read_json_dir::<Module>(&root.join("module"), "module", &mut problems);
    let kits = read_json_dir::<SceneKit>(&root.join("kit"), "kit", &mut problems);
    for (stem, set) in &tilesets {
        if stem != &set.id {
            problems.push(format!("tileset file '{stem}.json' declares id '{}'", set.id));
        }
        problems.extend(set.validate());
    }
    for (stem, module) in &modules {
        if stem != &module.id {
            problems.push(format!("module file '{stem}.json' declares id '{}'", module.id));
        }
        problems.extend(module.validate());
    }
    for (stem, kit) in &kits {
        if stem != &kit.id {
            problems.push(format!("kit file '{stem}.json' declares id '{}'", kit.id));
        }
        problems.extend(kit.validate());
    }
    // Cross-record references inside the library.
    for module in modules.values() {
        for layer in &module.terrain {
            if !tilesets.contains_key(&layer.tileset) {
                problems.push(format!(
                    "module '{}': terrain layer references missing tileset '{}'",
                    module.id, layer.tileset
                ));
            }
        }
        for cell in &module.tiles {
            if !tilesets.contains_key(&cell.tileset) {
                problems.push(format!(
                    "module '{}': tile placement references missing tileset '{}'",
                    module.id, cell.tileset
                ));
            }
        }
        for decor in &module.decor {
            if let Some(kit_id) = &decor.scene_kit {
                if !kits.contains_key(kit_id) {
                    problems.push(format!(
                        "module '{}': decor '{}' references missing kit '{kit_id}'",
                        module.id, decor.id
                    ));
                }
            }
        }
    }
    for kit in kits.values() {
        problems.extend(kit.validate_items(&BTreeMap::new()).into_iter().map(|e| {
            // Kit prototypes live in the prototype library, not here; only
            // structural problems are actionable from the scene side.
            if e.contains("missing prototype") {
                format!("kit '{}': prototype references are resolved by the asset library at build time", kit.id)
            } else {
                e
            }
        }));
    }
    Ok(SceneLibrary { tilesets, modules, kits, problems })
}

/// A kit resolver that maps a scene-kit to placements resolved from the
/// prototype library when it is available; scenes without kits never call it.
struct NoKits;
impl KitResolver for NoKits {
    fn resolve(&self, kit_id: &str) -> pxl_core::PxlResult<Vec<KitPlacement>> {
        Err(PxlError::semantic(format!(
            "kit '{kit_id}' needs a prototype library to expand; wire the asset build first"
        )))
    }
}

fn parse_bounds(text: &str) -> pxl_core::PxlResult<[u32; 2]> {
    let (w, h) = text
        .split_once('x')
        .ok_or_else(|| PxlError::usage(format!("bounds must be WxH, got '{text}'")))?;
    let (w, h): (u32, u32) = (
        w.trim().parse().map_err(|_| PxlError::usage(format!("bounds width '{w}' is not an integer")))?,
        h.trim().parse().map_err(|_| PxlError::usage(format!("bounds height '{h}' is not an integer")))?,
    );
    if w == 0 || h == 0 {
        return Err(PxlError::usage("scene bounds must be positive"));
    }
    Ok([w, h])
}

fn compile_cmd(ctx: &Ctx, args: SceneCompileArgs) -> CmdResult<()> {
    let library = load_scene_library(&args.library)?;
    if !library.problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "scene library is not valid: {}",
            library.problems.join("; ")
        ))
        .with_hint("fix the listed records, then rerun")
        .into());
    }
    if !args.allow_pending {
        let mut unapproved = Vec::new();
        for set in library.tilesets.values() {
            if set.review != ReviewState::Approved {
                unapproved.push(set.id.clone());
            }
        }
        for module in library.modules.values() {
            if module.review != ReviewState::Approved {
                unapproved.push(module.id.clone());
            }
        }
        if !unapproved.is_empty() {
            return Err(PxlError::semantic(format!(
                "not approved by a human reviewer: {}",
                unapproved.join(", ")
            ))
            .with_hint("pass --allow-pending for a draft scene; approval is never set by this tool")
            .into());
        }
    }

    let root = library
        .modules
        .get(&args.module)
        .ok_or_else(|| PxlError::semantic(format!("module '{}' is not in the library", args.module)))?;
    let bounds = match &args.bounds {
        Some(text) => parse_bounds(text)?,
        None => {
            // Root module size × the shared tile size.
            let tile = library
                .tilesets
                .values()
                .next()
                .map(|s| u32::from(s.tile_size))
                .unwrap_or(16);
            [
                root.size[0].saturating_mul(tile),
                root.size[1].saturating_mul(tile),
            ]
        }
    };

    let module_library = ModuleLibrary {
        tilesets: library.tilesets.clone(),
        modules: library.modules.clone(),
        kits: library.kits.clone(),
    };
    let (scene, report) = compile_scene(
        &module_library,
        &NoKits,
        &args.module,
        &args.scene,
        bounds,
        args.seed,
        SceneBudget::default(),
    )?;

    let mut text = serde_json::to_string_pretty(&scene)
        .map_err(|e| PxlError::io(format!("scene encode failed: {e}")))?;
    text.push('\n');
    util::write_out(&args.out, text.as_bytes(), ctx.overwrite, ctx.json)?;

    let mut body = util::ok_base("scene compile");
    body.insert("out".into(), json!(args.out));
    body.insert("scene".into(), json!(scene.id));
    body.insert("bounds".into(), json!(scene.bounds));
    body.insert("instances".into(), json!(report.instances));
    body.insert("collision_cells".into(), json!(scene.collision.len()));
    body.insert("pixels".into(), json!(report.pixels));
    body.insert("max_depth".into(), json!(report.max_depth_reached));
    body.insert("exact_masks".into(), json!(report.exact_masks.len()));
    body.insert("approximated_masks".into(), json!(report.approximated_masks.len()));
    body.insert("seed".into(), json!(scene.seed));
    body.insert("digest".into(), json!(scene.digest()));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}
