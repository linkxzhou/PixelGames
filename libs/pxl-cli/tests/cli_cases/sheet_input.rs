use crate::support::{build_to, repo_root, validate_sheet};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn mutated_sheet(tag: &str, edit: impl FnOnce(&mut Value)) -> (PathBuf, PathBuf) {
    let out = build_to(&repo_root().join("examples/farmer/farmer.recipe.json"), tag);
    let path = out.join("farmer.sheet.json");
    let mut sheet: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut sheet);
    fs::write(&path, serde_json::to_string(&sheet).unwrap()).unwrap();
    (out, path)
}

fn first_frame_mut(sheet: &mut Value) -> &mut Value {
    let frames = sheet["frames"].as_object_mut().unwrap();
    let key = frames.keys().next().unwrap().clone();
    &mut frames.get_mut(&key).unwrap()["frame"]
}

#[test]
fn sheet_validate_rejects_a_frame_rect_past_the_image() {
    let (out, path) = mutated_sheet("rect-oob", |s| {
        first_frame_mut(s)["x"] = serde_json::json!(100000);
    });
    let (code, report) = validate_sheet(&path);
    assert_eq!(
        code, 4,
        "an out-of-image rect is a semantic error: {report}"
    );
    assert_eq!(report["error"]["code"], "E_SEMANTIC", "{report}");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn sheet_validate_rejects_a_coordinate_that_would_wrap_to_u32() {
    let (out, path) = mutated_sheet("rect-wrap", |s| {
        first_frame_mut(s)["x"] = serde_json::json!(4294967296u64);
    });
    let (code, report) = validate_sheet(&path);
    assert_ne!(code, 0, "{report}");
    assert!(
        report["error"]["message"].as_str().unwrap().contains("x"),
        "the error names the bad field: {report}"
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn sheet_validate_rejects_negative_and_fractional_coordinates() {
    for (tag, bad) in [
        ("rect-neg", serde_json::json!(-1)),
        ("rect-frac", serde_json::json!(1.5)),
    ] {
        let (out, path) = mutated_sheet(tag, |s| first_frame_mut(s)["y"] = bad.clone());
        let (code, report) = validate_sheet(&path);
        assert_ne!(code, 0, "{tag}: {report}");
        let _ = fs::remove_dir_all(&out);
    }
}

#[test]
fn sheet_validate_rejects_a_zero_sized_frame() {
    let (out, path) = mutated_sheet("rect-zero", |s| {
        first_frame_mut(s)["w"] = serde_json::json!(0);
    });
    let (code, report) = validate_sheet(&path);
    assert_ne!(code, 0, "{report}");
    let _ = fs::remove_dir_all(&out);
}

/// Same indices, same JSON, but the PNG palette now makes every colour
/// transparent: the file no longer shows what the metadata claims.
#[test]
fn sheet_validate_rejects_a_png_palette_that_disagrees_with_the_json() {
    let (out, path) = mutated_sheet("palette-skew", |_| {});
    let png_path = out.join("farmer.sheet.png");
    let bytes = fs::read(&png_path).unwrap();
    let (w, h, pixels, colors) = pxl_export::decode_indexed(&bytes).unwrap();
    let mut frame = pxl_core::IndexFrame::canvas(w as u16, h as u16).unwrap();
    frame.pixels = pixels;
    let mut skewed = pxl_core::Palette {
        id: "skew".into(),
        colors,
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    for c in skewed.colors.iter_mut() {
        c.a = 0;
    }
    fs::write(
        &png_path,
        pxl_export::encode_indexed_png(&frame, &skewed, 1).unwrap(),
    )
    .unwrap();
    let (code, report) = validate_sheet(&path);
    assert_ne!(code, 0, "a skewed PNG palette must not pass: {report}");
    assert!(
        report["error"]["message"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("palette"),
        "{report}"
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn sheet_validate_rejects_a_corrupt_palette_instead_of_falling_back() {
    let (out, path) = mutated_sheet("palette-corrupt", |s| {
        s["meta"]["pxl"]["colors"] = serde_json::json!(["#00000000", "not-a-colour"]);
    });
    let (code, report) = validate_sheet(&path);
    assert_ne!(
        code, 0,
        "a corrupt palette must not be papered over: {report}"
    );
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn sheet_validate_still_accepts_a_sheet_without_pxl_metadata() {
    let (out, path) = mutated_sheet("legacy", |s| {
        s["meta"].as_object_mut().unwrap().remove("pxl");
    });
    let (code, report) = validate_sheet(&path);
    assert!(
        code == 0 || code == 5,
        "legacy sheet must still load: {report}"
    );
    assert_ne!(report["error"]["code"], "E_SEMANTIC", "{report}");
    let _ = fs::remove_dir_all(&out);
}
