//! P08: the whole-atlas RGBA8 budget. `build` and `sheet` report it, the
//! sheet json embeds it under `meta.pxl.budget`, and the estimate is
//! width × height × 4 bytes — never the PNG size on disk.

use crate::support::{build_to, json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

fn budget_of(value: &Value) -> Value {
    value["budget"].clone()
}

/// `pxl build` reports the budget and the sheet json carries the same
/// numbers under meta.pxl.budget.
#[test]
fn build_reports_the_atlas_budget() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let out = build_to(&recipe, "budget-build");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(out.join("static_bullet.build.json")).unwrap())
            .unwrap();

    let sheet: Value = serde_json::from_str(
        &fs::read_to_string(out.join("static_bullet.sheet.json")).unwrap(),
    )
    .unwrap();
    let embedded = &sheet["meta"]["pxl"]["budget"];
    assert!(embedded.is_object(), "sheet json embeds meta.pxl.budget");
    let w = embedded["width"].as_u64().unwrap();
    let h = embedded["height"].as_u64().unwrap();
    // RGBA8 estimate: w * h * 4, one page, matching the meta size.
    assert_eq!(embedded["rgba8_bytes"].as_u64().unwrap(), w * h * 4);
    assert_eq!(embedded["pages"].as_u64().unwrap(), 1);
    assert_eq!(embedded["page_limit"].as_u64().unwrap(), 1);
    assert_eq!(
        embedded["rgba8_mib"].as_f64().unwrap(),
        (w * h * 4) as f64 / (1024.0 * 1024.0)
    );
    // The frame count agrees with the frames map.
    let frames = sheet["frames"].as_object().unwrap().len() as u64;
    assert_eq!(embedded["frames"].as_u64().unwrap(), frames);
    // The manifest is untouched by P08 (schema_version stays 3); the budget
    // lives in the sheet json and the CLI body instead.
    assert!(manifest.get("budget").is_none() || manifest["budget"].is_null());
    let _ = fs::remove_dir_all(&out);
}

/// `pxl sheet` reports the same budget on stdout and never confuses the
/// RGBA8 estimate with the PNG file size.
#[test]
fn sheet_reports_the_rgba8_estimate_not_the_png_size() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let built = build_to(&recipe, "budget-sheet");
    let doc = built.join("bullet.pxl.json");
    assert!(doc.is_file(), "build must publish bullet.pxl.json: {:?}", fs::read_dir(&built).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>());
    let target = std::env::temp_dir().join(format!("pxl-budget-target-{}", std::process::id()));
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(&target).unwrap();
    let png = target.join("s.png");
    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "sheet",
        doc.to_str().unwrap(),
        "-o",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let body = json_line(&stdout);
    let budget = budget_of(&body);
    assert!(budget.is_object(), "sheet body carries a budget: {body}");
    let w = budget["width"].as_u64().unwrap();
    let h = budget["height"].as_u64().unwrap();
    assert_eq!(budget["rgba8_bytes"].as_u64().unwrap(), w * h * 4);
    let png_size = fs::metadata(&png).unwrap().len();
    // An indexed PNG is smaller than its RGBA8 expansion; the budget must
    // be the memory estimate, not the file size.
    assert!(
        budget["rgba8_bytes"].as_u64().unwrap() >= png_size,
        "rgba8 estimate {} must not be the png size {png_size}",
        budget["rgba8_bytes"]
    );
    let _ = fs::remove_dir_all(&target);
    let _ = fs::remove_dir_all(&built);
}
