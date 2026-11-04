use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_store::digest::digest_file;

use crate::RunArgs;

pub struct AudioBuild {
    source_dir: PathBuf,
    manifest: Value,
}

pub struct PackagedAudio {
    pub manifest: Value,
    pub entries: Vec<Value>,
}

pub fn requirements(args: &RunArgs) -> Result<Vec<(String, Value)>> {
    let sibling = args.requirements.with_file_name("audio-requirements.json");
    let path = args
        .audio_requirements
        .as_ref()
        .or_else(|| sibling.is_file().then_some(&sibling));
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let text = fs::read_to_string(path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    let document: Value = serde_json::from_str(&text)
        .map_err(|e| HarnessError::schema(format!("audio-requirements.json: {e}")))?;
    if document["schema_version"] != 1 {
        return Err(HarnessError::schema(format!(
            "unsupported audio-requirements schema_version {}; expected 1",
            document["schema_version"]
        )));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for requirement in document["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let id = requirement["audio_id"]
            .as_str()
            .ok_or_else(|| HarnessError::schema("audio requirement has no audio_id"))?
            .to_string();
        if !seen.insert(id.clone()) {
            return Err(HarnessError::semantic(format!(
                "duplicate audio requirement '{id}'"
            )));
        }
        out.push((id, requirement));
    }
    Ok(out)
}

pub fn load(
    index: &Value,
    input: &Path,
    attempts: &mut Vec<Value>,
    integrity_errors: &mut Vec<String>,
    missing_files: &mut Vec<String>,
) -> Result<Option<AudioBuild>> {
    let Some(audio) = index.get("audio") else {
        return Ok(None);
    };
    if audio["status"] != "ok" {
        let message = audio["error"]
            .as_str()
            .unwrap_or("audio build failed")
            .to_string();
        attempts.push(json!({"asset": "audio", "issue": message, "result": "failed"}));
        missing_files.push("audio: step-300 did not publish a complete render".into());
        return Ok(None);
    }
    let out_rel = audio["out"].as_str().unwrap_or("audio/");
    let manifest_rel = audio["manifest"].as_str().unwrap_or("audio-manifest.json");
    if !safe_relative(out_rel) || !safe_relative(manifest_rel) {
        integrity_errors.push("audio index contains an unsafe path".into());
        return Ok(None);
    }
    let source_dir = input.join(out_rel);
    let manifest_path = source_dir.join(manifest_rel);
    let manifest: Value = match fs::read_to_string(&manifest_path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| HarnessError::schema(format!("{}: {e}", manifest_path.display())))?,
        Err(error) => {
            missing_files.push(format!("audio: {error}"));
            return Ok(None);
        }
    };
    if manifest["contract"] != "audio-manifest/v1" {
        integrity_errors.push(format!(
            "unsupported audio manifest contract {}",
            manifest["contract"]
        ));
        return Ok(None);
    }
    let entries = manifest["entries"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        integrity_errors.push("audio manifest has no entries".into());
        return Ok(None);
    }
    let mut missing = false;
    for entry in &entries {
        let Some(rel) = entry["path"].as_str().filter(|path| safe_relative(path)) else {
            integrity_errors.push(format!("audio entry {} has an unsafe path", entry["id"]));
            return Ok(None);
        };
        if !source_dir.join(rel).is_file() {
            missing_files.push(format!("audio {}: {rel}", entry["id"]));
            missing = true;
        }
    }
    if missing {
        return Ok(None);
    }
    let report = pxl_audio::inspect::inspect_dir(&source_dir)
        .map_err(|error| HarnessError::integrity(format!("audio inspect: {}", error.message)))?;
    if !report.ok {
        integrity_errors.extend(report.problems);
        return Ok(None);
    }
    Ok(Some(AudioBuild {
        source_dir,
        manifest,
    }))
}

pub fn coverage(
    build: Option<&AudioBuild>,
    requirements: &[(String, Value)],
) -> (Value, Vec<Value>, Vec<String>) {
    let delivered: BTreeMap<String, &Value> = build
        .into_iter()
        .flat_map(|build| build.manifest["entries"].as_array().into_iter().flatten())
        .filter_map(|entry| entry["id"].as_str().map(|id| (id.to_string(), entry)))
        .collect();
    let required: Vec<_> = requirements
        .iter()
        .filter(|(_, requirement)| requirement["required"].as_bool().unwrap_or(true))
        .collect();
    let mut missing = Vec::new();
    let mut manifest_entries = Vec::new();
    for (id, requirement) in requirements {
        let status = if delivered.contains_key(id) {
            "delivered"
        } else {
            if requirement["required"].as_bool().unwrap_or(true) {
                missing.push(id.clone());
            }
            "missing"
        };
        manifest_entries.push(json!({
            "id": id,
            "kind": requirement["kind"],
            "required": requirement["required"].as_bool().unwrap_or(true),
            "trigger": requirement["trigger"],
            "status": status,
        }));
    }
    let extra: Vec<String> = delivered
        .keys()
        .filter(|id| {
            !requirements
                .iter()
                .any(|(required_id, _)| required_id == *id)
        })
        .cloned()
        .collect();
    let delivered_required = required
        .iter()
        .filter(|(id, _)| delivered.contains_key(id))
        .count();
    (
        json!({
            "required": required.len(),
            "delivered": delivered_required,
            "missing": missing,
            "extra": extra,
        }),
        manifest_entries,
        missing,
    )
}

pub fn package(
    out: &Path,
    build: Option<&AudioBuild>,
    requirements: &[(String, Value)],
) -> Result<Option<PackagedAudio>> {
    let Some(build) = build else {
        return Ok(None);
    };
    let destination = out.join("assets/audio");
    fs::create_dir_all(&destination)
        .map_err(|e| HarnessError::io(format!("cannot create audio package: {e}")))?;
    let required: BTreeMap<&str, bool> = requirements
        .iter()
        .map(|(id, requirement)| {
            (
                id.as_str(),
                requirement["required"].as_bool().unwrap_or(true),
            )
        })
        .collect();
    let mut entries = Vec::new();
    for entry in build.manifest["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let id = entry["id"].as_str().unwrap_or_default();
        let rel = entry["path"].as_str().unwrap_or_default();
        let target_rel = format!("assets/audio/{rel}");
        let target = out.join(&target_rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| HarnessError::io(format!("cannot create audio directory: {e}")))?;
        }
        fs::copy(build.source_dir.join(rel), &target)
            .map_err(|e| HarnessError::io(format!("cannot copy audio {id}: {e}")))?;
        let mut packaged = json!({
            "id": id,
            "kind": entry["kind"],
            "required": required.get(id).copied().unwrap_or(false),
            "path": target_rel,
            "digest": entry["file_sha256"],
            "pcm_sha256": entry["pcm_sha256"],
            "deps_sha256": entry["deps_sha256"],
            "sample_rate": build.manifest["sample_rate"],
            "channels": entry["channels"],
            "frames": entry["frames"],
            "qa": entry["qa"],
        });
        if !entry["loop"].is_null() {
            packaged["loop"] = entry["loop"].clone();
        }
        entries.push(packaged);
    }
    entries.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let manifest_target = destination.join("audio-manifest.json");
    fs::copy(
        build.source_dir.join("audio-manifest.json"),
        &manifest_target,
    )
    .map_err(|e| HarnessError::io(format!("cannot copy audio manifest: {e}")))?;
    Ok(Some(PackagedAudio {
        manifest: json!({
            "path": "assets/audio/audio-manifest.json",
            "digest": digest_file(&manifest_target)?,
        }),
        entries,
    }))
}

pub fn revision_files(out: &Path) -> Result<Vec<(String, String)>> {
    let root = out.join("assets/audio");
    let mut files = Vec::new();
    if root.is_dir() {
        collect(out, &root, &mut files)?;
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
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((rel, digest_file(&path)?));
        }
    }
    Ok(())
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}
