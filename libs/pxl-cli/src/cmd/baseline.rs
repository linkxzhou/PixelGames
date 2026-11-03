use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use serde_json::{json, Value};

use super::args::PlanArgs;
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

pub(super) fn baseline_cmd(
    ctx: &Ctx,
    args: &PlanArgs,
    input: &Path,
    recipes: &[(String, String)],
) -> CmdResult<()> {
    let out = PathBuf::from(&args.out);
    let work = out.join(".baseline");
    if work.exists() {
        fs::remove_dir_all(&work).map_err(|e| PxlError::io(format!("{}: {e}", work.display())))?;
    }
    fs::create_dir_all(&work).map_err(|e| PxlError::io(format!("{}: {e}", work.display())))?;
    let mut entries = Vec::new();
    for (id, recipe) in recipes {
        let first = super::plan::build_one(ctx, args, input, &work.join("a"), recipe, id);
        let contract_pass = first["status"] == "ok";
        let mut deterministic = false;
        let mut differences = Vec::new();
        let mut second_status = "not_run".to_string();
        let mut frame_hashes = BTreeMap::new();
        if contract_pass {
            let second = super::plan::build_one(ctx, args, input, &work.join("b"), recipe, id);
            second_status = second["status"].as_str().unwrap_or("failed").to_string();
            let a = tree_hashes(&work.join("a").join(id))?;
            let b = tree_hashes(&work.join("b").join(id))?;
            differences = compare_hashes(&a, &b);
            frame_hashes = indexed_frame_hashes(&work.join("a").join(id))?;
            let other_frames = indexed_frame_hashes(&work.join("b").join(id))?;
            if frame_hashes != other_frames {
                differences.push("indexed_frames".into());
            }
            deterministic = second["status"] == "ok" && differences.is_empty();
        }
        let source = input.join(recipe);
        let dependencies = super::dependencies::recipe_dependencies(input, &source)?;
        entries.push(json!({
            "id": id,
            "recipe": recipe,
            "executed": contract_pass,
            "contract_pass": contract_pass && second_status == "ok",
            "deterministic_pass": deterministic,
            "visual_review": "not_run",
            "model_robustness": "not_run",
            "first_status": first["status"],
            "second_status": second_status,
            "differences": differences,
            "dependencies": dependencies,
            "indexed_frames": frame_hashes,
        }));
    }
    let ok = entries
        .iter()
        .all(|entry| entry["deterministic_pass"] == true);
    let baseline = json!({
        "schema_version": 2,
        "check_version": "v4-m0",
        "pxl_version": pxl_core::PXL_VERSION,
        "gen_version": pxl_core::GEN_VERSION,
        "capability_digest": pxl_format::capability_digest(),
        "ok": ok,
        "assets": entries,
        "visual_review": "not_run",
        "model_robustness": "not_run",
        "cross_platform": "not_run",
        "wasm": "not_run",
    });
    let text = pxl_format::to_pretty(&baseline)?;
    fs::write(out.join("baseline.json"), text)
        .map_err(|e| PxlError::io(format!("baseline.json: {e}")))?;
    let _ = fs::remove_dir_all(&work);
    let mut body = util::ok_base("baseline");
    body.insert("ok".into(), json!(ok));
    body.insert("checked".into(), json!(entries.len()));
    body.insert(
        "baseline".into(),
        json!(out.join("baseline.json").to_string_lossy()),
    );
    util::emit_ok(ctx, &Value::Object(body));
    if ok {
        Ok(())
    } else {
        Err(
            CmdError::new(PxlError::qa("one or more baseline checks failed"))
                .with_extra(json!({"cmd": "baseline", "ok": false})),
        )
    }
}

fn tree_hashes(root: &Path) -> CmdResult<BTreeMap<String, String>> {
    tree_hashes_at(root, root)
}

fn tree_hashes_at(root: &Path, current: &Path) -> CmdResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    if !current.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(current)
        .map_err(|e| PxlError::io(format!("{}: {e}", current.display())))?
    {
        let path = entry
            .map_err(|e| PxlError::io(format!("{}: {e}", current.display())))?
            .path();
        if path.is_dir() {
            out.extend(tree_hashes_at(root, &path)?);
        } else if path.is_file() {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let previous = out.insert(
                name.clone(),
                util::hash_of(
                    &fs::read(&path)
                        .map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?,
                ),
            );
            if previous.is_some() {
                return Err(PxlError::io(format!("duplicate baseline path '{name}'")).into());
            }
        }
    }
    Ok(out)
}

fn compare_hashes(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    let mut names = a.keys().chain(b.keys()).collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|name| a.get(*name) != b.get(*name))
        .cloned()
        .collect()
}

fn indexed_frame_hashes(root: &Path) -> CmdResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(root).map_err(|e| PxlError::io(format!("{}: {e}", root.display())))? {
        let path = entry
            .map_err(|e| PxlError::io(format!("{}: {e}", root.display())))?
            .path();
        if path.extension().and_then(|v| v.to_str()) != Some("json")
            || !path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|name| name.ends_with(".pxl.json"))
        {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?;
        let doc: pxl_format::PxlDoc = pxl_format::parse_json(&text, "baseline document")?;
        let asset = path.file_name().unwrap_or_default().to_string_lossy();
        for name in doc.frame_names() {
            let frame = doc.frame(&name)?;
            let mut bytes = Vec::with_capacity(frame.pixels.len() + 5);
            bytes.extend_from_slice(&frame.w.to_le_bytes());
            bytes.extend_from_slice(&frame.h.to_le_bytes());
            bytes.push(name.as_bytes().len() as u8);
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&frame.pixels);
            out.insert(format!("{asset}:{name}"), util::hash_of(&bytes));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pxl-baseline-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn tree_hashes_preserves_nested_paths_and_detects_every_change() {
        let a = fixture("a");
        let b = fixture("b");
        for root in [&a, &b] {
            fs::create_dir_all(root.join("a")).unwrap();
            fs::create_dir_all(root.join("b")).unwrap();
            fs::write(root.join("a/report.json"), b"a").unwrap();
            fs::write(root.join("b/report.json"), b"b").unwrap();
            fs::write(root.join("b/emissive.mask"), b"mask").unwrap();
        }
        let first = tree_hashes(&a).unwrap();
        assert_eq!(
            first.keys().cloned().collect::<Vec<_>>(),
            vec!["a/report.json", "b/emissive.mask", "b/report.json"]
        );
        assert_eq!(compare_hashes(&first, &tree_hashes(&b).unwrap()), Vec::<String>::new());

        fs::write(b.join("b/report.json"), b"changed").unwrap();
        assert_eq!(
            compare_hashes(&first, &tree_hashes(&b).unwrap()),
            vec!["b/report.json"]
        );
        fs::remove_file(b.join("b/emissive.mask")).unwrap();
        let differences = compare_hashes(&first, &tree_hashes(&b).unwrap());
        assert!(differences.contains(&"b/emissive.mask".to_string()));
        assert!(differences.contains(&"b/report.json".to_string()));

        fs::remove_dir_all(a).unwrap();
        fs::remove_dir_all(b).unwrap();
    }
}
