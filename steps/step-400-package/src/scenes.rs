//! P06: compiled scenes, the stable resource map and reference closure.
//!
//! The package is the boundary between the build tree and the engine. From
//! v4 on it records every compiled scene it ships, the stable resource ids
//! the scenes draw through, and it verifies the full closure
//! scene -> resource -> sheet -> PNG before anything is published. Scenes
//! and their referenced sheets are copied into the package root so a
//! package moved to another directory stays readable (self-contained);
//! older packages that point at the build tree keep reading through the
//! same code paths without being called self-contained.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::tileset::CompiledScene;
use pxl_store::digest::digest_file;

/// One scene file discovered in the build tree.
pub struct SceneSource {
    pub path: PathBuf,
    pub scene: CompiledScene,
}

/// Scan `dir` for `*.scene.json` files and parse them.
pub fn load(dir: &Path) -> Result<Vec<SceneSource>> {
    if !dir.is_dir() {
        return Err(HarnessError::usage(format!(
            "{} is not a directory",
            dir.display()
        ))
        .with_hint("--scenes takes the directory `step-300 scene compile` wrote"));
    }
    let mut sources = Vec::new();
    for entry in fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        let scene: CompiledScene = serde_json::from_str(&text).map_err(|e| {
            HarnessError::schema(format!("{}: {e}", path.display()))
        })?;
        sources.push(SceneSource { path, scene });
    }
    if sources.is_empty() {
        return Err(HarnessError::semantic(format!(
            "no compiled scenes under {}",
            dir.display()
        ))
        .with_hint("run `step-300 scene compile` first, or drop --scenes for a package without scenes"));
    }
    sources.sort_by(|a, b| a.scene.id.cmp(&b.scene.id));
    Ok(sources)
}

/// Copy every scene into `<out>/scenes/` and return the package entries.
/// The copy is skipped when the source already lives inside the package
/// tree, so re-running on its own output stays idempotent.
pub fn package(out: &Path, sources: &[SceneSource]) -> Result<Vec<Value>> {
    let dest = out.join("scenes");
    fs::create_dir_all(&dest)
        .map_err(|e| HarnessError::io(format!("cannot create {}: {e}", dest.display())))?;
    let mut entries = Vec::new();
    for source in sources {
        let name = source
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}.scene.json", source.scene.id));
        let target_rel = format!("scenes/{name}");
        let target = out.join(&target_rel);
        if !source.path.starts_with(out) {
            fs::copy(&source.path, &target)
                .map_err(|e| HarnessError::io(format!("cannot copy scene {}: {e}", source.scene.id)))?;
        } else if !target.is_file() {
            return Err(HarnessError::semantic(format!(
                "scene '{}' is already inside the package but missing at {target_rel}",
                source.scene.id
            )));
        }
        entries.push(json!({
            "id": source.scene.id,
            "path": target_rel,
            "digest": digest_file(&target)?,
            "bounds": source.scene.bounds,
            "instances": source.scene.instances.len(),
            "depends": source.scene.depends,
            "review": source.scene.review,
        }));
    }
    Ok(entries)
}

/// The stable resource map collected from every build manifest's
/// `resources` block, ordered by resource id so serialization is stable
/// whatever the recipe order was.
pub fn resource_map(
    facts: &BTreeMap<String, (Vec<Value>, PathBuf)>,
) -> Vec<Value> {
    let mut by_id: BTreeMap<String, Value> = BTreeMap::new();
    for (entries, _dir) in facts.values() {
        for entry in entries {
            let Some(id) = entry.get("resource_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(mut bound) = entry.clone().as_object().cloned() else {
                continue;
            };
            bound.remove("asset_id");
            bound.insert(
                "palette_family".into(),
                entry.get("palette_family").cloned().unwrap_or(Value::Null),
            );
            by_id.insert(id.to_string(), Value::Object(bound));
        }
    }
    by_id.into_values().collect()
}

/// Verify scene -> resource -> sheet -> PNG closure. Every instance of
/// every scene must name a resource in the map; every resource must name
/// a sheet whose json exists (relative to the build tree) with the digest
/// the package records. Returns integrity problems (fail 15) and missing
/// files (blocked 11).
pub fn closure(
    scenes: &[SceneSource],
    resources: &BTreeMap<String, Value>,
    sheets: &BTreeMap<String, (PathBuf, String)>,
) -> (Vec<String>, Vec<String>) {
    let mut integrity = Vec::new();
    let mut missing = Vec::new();
    for source in scenes {
        for instance in &source.scene.instances {
            // `res-<id>#variant` addresses a variant frame of a resource.
            let base = instance.resource_id.split('#').next().unwrap_or("");
            if !base.starts_with("res-") {
                integrity.push(format!(
                    "scene '{}': instance '{}' does not address a compiled resource",
                    source.scene.id, instance.id
                ));
                continue;
            }
            if !resources.contains_key(base) {
                integrity.push(format!(
                    "scene '{}': instance '{}' references resource '{base}' that no build produced",
                    source.scene.id, instance.id
                ));
            }
        }
    }
    for (id, resource) in resources {
        // The resource carries both the packed image (`sheet`, .png) and its
        // metadata (`sheet_json`, .json); the closure verifies the metadata
        // file and the image it points at.
        let Some(sheet_json) = resource.get("sheet_json").and_then(Value::as_str) else {
            integrity.push(format!("resource '{id}' has no sheet_json reference"));
            continue;
        };
        let sheet_stem = sheet_json.trim_end_matches(".sheet.json");
        match sheets.get(sheet_stem) {
            Some((path, digest)) => {
                if !path.is_file() {
                    missing.push(format!("resource '{id}': sheet {sheet_json}"));
                    continue;
                }
                let actual = digest_file(path).unwrap_or_default();
                if actual != *digest {
                    integrity.push(format!(
                        "resource '{id}': sheet {sheet_json} digest {actual} != package {digest}"
                    ));
                }
                // The PNG the sheet json points at must exist beside it.
                if let Ok(text) = fs::read_to_string(path) {
                    if let Ok(doc) = serde_json::from_str::<Value>(&text) {
                        let image = doc["meta"]["image"].as_str().unwrap_or_default();
                        if image.is_empty() {
                            integrity.push(format!(
                                "resource '{id}': sheet {sheet_json} has no meta.image"
                            ));
                        } else if !path.parent().unwrap_or(path).join(image).is_file() {
                            missing.push(format!(
                                "resource '{id}': sheet image {image}"
                            ));
                        }
                    } else {
                        integrity.push(format!(
                            "resource '{id}': sheet {sheet_json} is not valid JSON"
                        ));
                    }
                }
            }
            None => integrity.push(format!(
                "resource '{id}': sheet '{sheet_json}' is not in the package"
            )),
        }
    }
    (integrity, missing)
}

/// A path is package-safe when it stays inside the directory it is
/// resolved against: no absolute components, no `..` escapes. Symlinks
/// are resolved so a link pointing outside fails the same way.
pub fn safe_relative(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if Path::new(path).is_absolute() {
        return false;
    }
    Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Reject package paths that can never resolve safely: absolute paths and
/// non-normal components. Older packages legitimately reference the build
/// tree with `../` segments (they are readable but not self-contained), so
/// `..` itself is not an escape here — only absolute paths are. Scenes and
/// audio are copied into the package, so their paths must be strictly
/// package-internal.
pub fn escapes(dir: &Path, package: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let root = dir.canonicalize().ok();
    let mut check = |out: &mut Vec<String>, label: &str, entry: &Value| {
        let Some(rel) = entry.get("path").and_then(Value::as_str) else {
            return;
        };
        if Path::new(rel).is_absolute() {
            out.push(format!("{label}: '{rel}' is an absolute path"));
            return;
        }
        if !safe_relative(rel) && !relative_with_parents(rel) {
            out.push(format!("{label}: '{rel}' is not a relative path"));
            return;
        }
        // Scenes are copied into the package (v4): they must resolve
        // inside it. Everything else may point at the build tree.
        let target = dir.join(rel);
        if let (Some(canonical), Some(root)) = (target.canonicalize().ok(), root.as_ref()) {
            if !canonical.starts_with(root) && !relative_with_parents(rel) {
                out.push(format!("{label}: '{rel}' escapes the package root"));
            }
        }
    };
    if let Some(palette) = package.get("palette") {
        check(&mut out, "palette", palette);
    }
    for sheet in package["sheets"].as_array().cloned().unwrap_or_default() {
        let id = sheet["id"].as_str().unwrap_or_default();
        check(&mut out, &format!("sheet {id}"), &sheet);
    }
    for scene in package["scenes"].as_array().cloned().unwrap_or_default() {
        let id = scene["id"].as_str().unwrap_or_default();
        // Scenes are always package-internal: no `..` tolerated.
        let rel = scene["path"].as_str().unwrap_or_default();
        if !safe_relative(rel) {
            out.push(format!("scene {id}: '{rel}' is not a package-internal path"));
        } else {
            check(&mut out, &format!("scene {id}"), &scene);
        }
    }
    for (key, entry) in package["visual"]
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| {
            ["report", "metadata", "preview_manifest", "effective_style"].contains(&key.as_str())
        })
    {
        check(&mut out, &format!("visual {key}"), &entry);
    }
    for audio in package["audio"].as_array().cloned().unwrap_or_default() {
        let id = audio["id"].as_str().unwrap_or_default();
        // Audio is copied into the package like scenes.
        let rel = audio["path"].as_str().unwrap_or_default();
        if !safe_relative(rel) {
            out.push(format!("audio {id}: '{rel}' is not a package-internal path"));
        } else {
            check(&mut out, &format!("audio {id}"), &audio);
        }
    }
    out
}

/// A relative path that may climb with `..` but never becomes absolute —
/// the legacy build-tree reference pattern (`../300-build/...`).
fn relative_with_parents(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir | Component::ParentDir))
}

/// Digests of every file under `<out>/scenes/`, relative to `out`, for the
/// revision aggregate.
pub fn revision_files(out: &Path) -> Result<Vec<(String, String)>> {
    let root = out.join("scenes");
    let mut files = Vec::new();
    if root.is_dir() {
        collect(&root, &root, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .map_err(|e| HarnessError::io(format!("cannot read {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let rel = path
                .strip_prefix(root.parent().unwrap_or(root))
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((rel, digest_file(&path)?));
        }
    }
    Ok(())
}

/// Union of the capabilities every packaged scene requires beyond the
/// visual channel capabilities: today that is the compiled-scene capability
/// itself.
pub fn scene_capabilities(scenes: &[SceneSource]) -> BTreeSet<String> {
    let mut caps = BTreeSet::new();
    if !scenes.is_empty() {
        caps.insert("compiled-scene/v1".into());
    }
    caps
}
