use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use serde_json::{Map, Value};

use crate::error::CmdResult;
use crate::util;

pub(super) fn recipe_dependencies(input_root: &Path, recipe_path: &Path) -> CmdResult<Value> {
    let mut files = Map::new();
    let mut visited = BTreeSet::new();
    let text = fs::read_to_string(recipe_path)
        .map_err(|e| PxlError::io(format!("cannot read {}: {e}", recipe_path.display())))?;
    let value: Value = pxl_format::parse_json(&text, "recipe")?;
    collect_value(
        &value,
        recipe_path.parent().unwrap_or_else(|| Path::new(".")),
        input_root,
        &mut files,
        &mut visited,
    )?;
    if let Some(palette) = value.get("palette").and_then(Value::as_str) {
        collect_file(
            palette,
            recipe_path.parent().unwrap_or_else(|| Path::new(".")),
            input_root,
            &mut files,
            &mut visited,
        )?;
    }
    if let Some(spec_root) = recipe_path.parent().and_then(Path::parent) {
        if spec_root.join("spec-manifest.json").is_file() {
            for name in ["visual-intent.snapshot.json", "visual-plan.json"] {
                collect_file(name, spec_root, input_root, &mut files, &mut visited)?;
            }
        }
    }
    Ok(Value::Object(files))
}

pub(super) fn sprite_dependencies(input_root: &Path, spec_path: &Path) -> CmdResult<Value> {
    let mut files = Map::new();
    let mut visited = BTreeSet::new();
    let text = fs::read_to_string(spec_path)
        .map_err(|e| PxlError::io(format!("cannot read {}: {e}", spec_path.display())))?;
    let value: Value = pxl_format::parse_json(&text, "sprite")?;
    collect_value(
        &value,
        spec_path.parent().unwrap_or_else(|| Path::new(".")),
        input_root,
        &mut files,
        &mut visited,
    )?;
    Ok(Value::Object(files))
}

fn collect_value(
    value: &Value,
    base: &Path,
    input_root: &Path,
    files: &mut Map<String, Value>,
    visited: &mut BTreeSet<PathBuf>,
) -> CmdResult<()> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if matches!(
                    key.as_str(),
                    "spec" | "file" | "palette" | "legend_ref" | "template"
                ) {
                    if let Some(reference) = child.as_str() {
                        collect_file(reference, base, input_root, files, visited)?;
                    }
                }
                collect_value(child, base, input_root, files, visited)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_value(child, base, input_root, files, visited)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_file(
    reference: &str,
    base: &Path,
    input_root: &Path,
    files: &mut Map<String, Value>,
    visited: &mut BTreeSet<PathBuf>,
) -> CmdResult<()> {
    let path = if Path::new(reference).is_absolute() {
        PathBuf::from(reference)
    } else {
        base.join(reference)
    };
    if !path.is_file() {
        if matches!(reference, "p16" | "tank") {
            files.insert(
                format!("builtin:{reference}"),
                Value::String(pxl_core::PXL_VERSION.into()),
            );
        }
        return Ok(());
    }
    let path = path
        .canonicalize()
        .map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?;
    if !visited.insert(path.clone()) {
        return Ok(());
    }
    let bytes = fs::read(&path).map_err(|e| PxlError::io(format!("{}: {e}", path.display())))?;
    let canonical_root = input_root
        .canonicalize()
        .unwrap_or_else(|_| input_root.to_path_buf());
    let relative = path.strip_prefix(&canonical_root).unwrap_or(&path);
    let name = if relative.is_absolute() {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        relative.to_string_lossy().replace('\\', "/")
    };
    files.insert(name, Value::String(util::hash_of(&bytes)));
    if path.extension().and_then(|v| v.to_str()) == Some("json") {
        let text = String::from_utf8(bytes)
            .map_err(|_| PxlError::schema(format!("{} is not UTF-8", path.display())))?;
        let value: Value = pxl_format::parse_json(&text, "dependency")?;
        collect_value(
            &value,
            path.parent().unwrap_or_else(|| Path::new(".")),
            input_root,
            files,
            visited,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_source_and_nested_palette_are_in_the_closure() {
        let root = std::env::temp_dir().join(format!("pxl-deps-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("palette.json"),
            r##"{"v":1,"id":"x","colors":["#00000000","#ffffffff"],"ramps":{},"cycles":{}}"##,
        )
        .unwrap();
        fs::write(root.join("image.svg"), "<svg/>").unwrap();
        fs::write(root.join("sprite.json"), r#"{"v":3,"size":32,"palette":"palette.json","source":{"kind":"svg","file":"image.svg"}}"#).unwrap();
        fs::write(root.join("asset.recipe.json"), r#"{"v":1,"asset":"x","palette":"palette.json","sprites":[{"id":"x","base":{"gen":"sprite","spec":"sprite.json"}}],"sheet":{}}"#).unwrap();
        let deps = recipe_dependencies(&root, &root.join("asset.recipe.json")).unwrap();
        for name in ["sprite.json", "image.svg", "palette.json"] {
            assert!(deps.get(name).is_some(), "missing {name}: {deps}");
        }
        let _ = fs::remove_dir_all(root);
    }
}
