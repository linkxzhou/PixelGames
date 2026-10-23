use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

fn dir_digest(dir: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some(".rejected") {
                    continue;
                }
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    walk(dir, dir, &mut out);
    out
}

/// A rebuild that fails QA must leave the previous good output untouched, and
/// the rejected attempt must be kept somewhere that cannot be mistaken for it.
#[test]
fn failed_build_does_not_overwrite_the_previous_good_output() {
    let root = repo_root();
    let src = root.join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-publish-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let good = src.join("static_bullet.recipe.json");
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        good.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let before = dir_digest(&out);

    // Same recipe without the static declaration fails QA.
    let text = fs::read_to_string(&good)
        .unwrap()
        .replace("\"mode\": \"static\",", "");
    let bad = dir.join("bad.recipe.json");
    fs::write(&bad, text).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        bad.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stdout}");
    let report = json_line(&stdout);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["pass"] == false),
        "the failure still reports its checks: {report}"
    );
    assert_eq!(
        dir_digest(&out),
        before,
        "a failed build changed the good output"
    );
    let rejected = out.join(".rejected");
    assert!(
        rejected.join("static_bullet.sheet.json").is_file(),
        "the rejected attempt is kept for diagnosis"
    );
    assert_eq!(
        report["rejected"],
        serde_json::json!(rejected.to_string_lossy())
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A successful rebuild into a populated directory still replaces the outputs:
/// the harness rebuilds the same directory on every repair round and does not
/// pass `--overwrite`, so build keeps that contract.
#[test]
fn successful_rebuild_replaces_the_previous_output_and_cleans_staging() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let out = std::env::temp_dir().join(format!("pxl-rebuild-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let run = || {
        pxl(&[
            "--json",
            "build",
            recipe.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
    };
    assert_eq!(run().0, 0);
    let first = dir_digest(&out);
    let (code, stdout, _) = run();
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(dir_digest(&out), first, "the build is deterministic");
    assert!(
        !out.join(".staging").exists(),
        "staging must not outlive the build"
    );
    assert!(!out.join(".rejected").exists());
    let _ = fs::remove_dir_all(&out);
}

/// A preview that cannot be rendered fails the build without touching the
/// published files.
#[test]
fn failed_preview_leaves_the_published_output_alone() {
    let root = repo_root();
    let src = root.join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-preview-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let good = src.join("static_bullet.recipe.json");
    let out = dir.join("out");
    assert_eq!(
        pxl(&[
            "--json",
            "build",
            good.to_str().unwrap(),
            "-o",
            out.to_str().unwrap()
        ])
        .0,
        0
    );
    let before = dir_digest(&out);
    let mut recipe: Value = serde_json::from_str(&fs::read_to_string(&good).unwrap()).unwrap();
    recipe["preview"] =
        serde_json::json!([{"file": "p.png", "source": "missing_source", "scale": 4}]);
    let bad = dir.join("badprev.recipe.json");
    fs::write(&bad, serde_json::to_string(&recipe).unwrap()).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "build",
        bad.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "{stdout}");
    assert_eq!(
        dir_digest(&out),
        before,
        "preview failure leaked into the output"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A successful build commits `<sheet>.build.json`: input digests, every
/// published file with the digest it has on disk, the QA result, and an
/// explicit "visual review not run". A failed rebuild leaves it unchanged.
#[test]
fn build_manifest_records_inputs_outputs_and_survives_a_failed_rebuild() {
    let root = repo_root();
    let src = root.join("examples/static_bullet");
    let dir = std::env::temp_dir().join(format!("pxl-manifest-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["static_bullet.shapes.json", "static_bullet.palette.json"] {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    let good = dir.join("static_bullet.recipe.json");
    fs::copy(src.join("static_bullet.recipe.json"), &good).unwrap();
    let out = dir.join("out");
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        good.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let body = json_line(&stdout);
    assert_eq!(body["manifest"], "static_bullet.build.json");
    let manifest_path = out.join("static_bullet.build.json");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["qa"]["ok"], true);
    assert_eq!(manifest["visual_review"], "not_run");
    assert!(manifest["inputs"]["specs"]["static_bullet.shapes.json"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    let files = manifest["outputs"].as_object().unwrap();
    assert_eq!(files.len(), body["outputs"].as_array().unwrap().len());
    for (name, digest) in files {
        assert!(out.join(name).is_file(), "{name} listed but not published");
        assert!(digest.as_str().unwrap().starts_with("sha256:"), "{name}");
    }
    let sheet_digest = files["static_bullet.sheet.png"].as_str().unwrap();
    assert_eq!(sheet_digest, body["hash"].as_str().unwrap());
    let before = fs::read(&manifest_path).unwrap();

    let text = fs::read_to_string(&good)
        .unwrap()
        .replace("\"mode\": \"static\",", "");
    let bad = dir.join("bad.recipe.json");
    fs::write(&bad, text).unwrap();
    let (code, _, _) = pxl(&[
        "--json",
        "build",
        bad.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 5);
    assert_eq!(
        fs::read(&manifest_path).unwrap(),
        before,
        "a failed build rewrote the committed manifest"
    );
    assert!(!out.join(".rejected/static_bullet.build.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

/// `pxl sheet` writes a png and a json; without `--overwrite` an existing
/// json must block the write just like an existing png, and nothing is written.
#[test]
fn sheet_refuses_to_replace_either_half_without_overwrite() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let built = crate::support::build_to(&recipe, "sheet-ow");
    let doc = built.join("bullet.pxl.json");
    let target = std::env::temp_dir().join(format!("pxl-sheet-target-{}", std::process::id()));
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(&target).unwrap();
    let png = target.join("s.png");
    let json = target.join("s.json");
    fs::write(&json, "old json").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "sheet",
        doc.to_str().unwrap(),
        "-o",
        png.to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "{stdout}");
    assert!(!png.exists(), "png written although the json blocked the sheet");
    assert_eq!(fs::read_to_string(&json).unwrap(), "old json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "sheet",
        doc.to_str().unwrap(),
        "-o",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert!(png.is_file());
    assert_ne!(fs::read_to_string(&json).unwrap(), "old json");
    let _ = fs::remove_dir_all(&target);
    let _ = fs::remove_dir_all(&built);
}
