//! QA asset-case helpers.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::support::{json_line, pxl, repo_root};

pub(super) fn farmer_with(tag: &str, edit: impl FnOnce(&mut Value)) -> (i32, Value, PathBuf) {
    let src = repo_root().join("examples/farmer");
    let dir = std::env::temp_dir().join(format!("pxl-farmer-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for entry in fs::read_dir(&src).unwrap().flatten() {
        let p = entry.path();
        if p.is_file() {
            fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
    let mut recipe: Value =
        serde_json::from_str(&fs::read_to_string(src.join("farmer.recipe.json")).unwrap()).unwrap();
    edit(&mut recipe);
    let path = dir.join("edited.recipe.json");
    fs::write(&path, recipe.to_string()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        path.to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    (code, json_line(&stdout), dir)
}
