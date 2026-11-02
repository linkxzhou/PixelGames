use std::fs;

use serde_json::Value;

use crate::support::{json_line, pxl, repo_root};

use super::helpers::{bullet_dir, temp_dir};

/// `plan` and `build` produce byte-identical outputs for the same recipe.
#[test]
fn plan_matches_build_bytes() {
    let recipe = bullet_dir().join("static_bullet.recipe.json");
    let build_out = temp_dir("build");
    let plan_in = temp_dir("plan-in");
    let plan_out = temp_dir("plan-out");

    // Assemble a step-200 style tree: <id>/<id>.recipe.json + siblings.
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }

    let (build_code, build_stdout, build_stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        build_out.to_str().unwrap(),
    ]);
    assert_eq!(
        build_code, 0,
        "build\nstdout={build_stdout}\nstderr={build_stderr}"
    );

    let (plan_code, plan_stdout, plan_stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
    ]);
    assert_eq!(
        plan_code, 0,
        "plan\nstdout={plan_stdout}\nstderr={plan_stderr}"
    );

    // Every published file the build wrote must be byte-identical under plan.
    let mut names: Vec<String> = fs::read_dir(&build_out)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json") || n.ends_with(".png"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "build produced no outputs");
    for name in &names {
        let a = fs::read(build_out.join(name)).unwrap();
        let b = fs::read(plan_out.join("static_bullet").join(name)).unwrap();
        assert_eq!(a, b, "plan and build differ on {name}");
    }

    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 5);
    assert_eq!(index["scope"], "all");
    let recipes = index["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0]["id"], "static_bullet");
    assert_eq!(recipes[0]["status"], "ok");
    assert!(recipes[0]["manifest"]
        .as_str()
        .unwrap()
        .ends_with(".build.json"));
}

#[test]
fn baseline_rebuilds_twice_and_separates_review_states() {
    let plan_in = temp_dir("baseline-in");
    let plan_out = temp_dir("baseline-out");
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--baseline",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let baseline: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("baseline.json")).unwrap()).unwrap();
    assert_eq!(baseline["ok"], true);
    assert_eq!(baseline["assets"][0]["deterministic_pass"], true);
    assert_eq!(baseline["assets"][0]["contract_pass"], true);
    assert_eq!(baseline["assets"][0]["visual_review"], "not_run");
    assert_eq!(baseline["model_robustness"], "not_run");
    assert!(
        baseline["assets"][0]["indexed_frames"]
            .as_object()
            .unwrap()
            .len()
            > 0
    );
    assert!(!plan_out.join(".baseline").exists());
}

/// `--dry-run` parses and validates without writing anything.
#[test]
fn plan_dry_run_writes_nothing() {
    let plan_in = temp_dir("dry-in");
    let plan_out = temp_dir("dry-out");
    fs::create_dir_all(plan_in.join("static_bullet")).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), plan_in.join("static_bullet").join(&name)).unwrap();
        }
    }

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = json_line(&stdout);
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["checked"], 1);
    assert_eq!(report["ok_count"], 1);
    assert!(
        !plan_out.exists(),
        "--dry-run must not create the output directory"
    );

    // A recipe whose palette does not resolve fails the dry run, is reported
    // in the summary line, and still writes nothing. In --json mode the
    // summary and the trailing error are two objects on stdout, so parse the
    // first line.
    let broken = plan_in.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(
        broken.join("broken.recipe.json"),
        r#"{ "v": 1, "asset": "broken", "seed": 1, "palette": "no-such.palette.json",
             "sprites": [], "tiles": [], "sheet": { "sources": [], "file": "broken.sheet.png" } }"#,
    )
    .unwrap();
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_ne!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(report["ok"], false);
    assert!(report["ok_count"].as_u64().unwrap() < report["checked"].as_u64().unwrap());
    assert!(
        !plan_out.exists(),
        "--dry-run must not create the output directory, even on failure"
    );
}

/// One failed recipe never aborts the batch: the good one still builds, the
/// bad one is recorded, and the exit code is 5.
#[test]
fn failed_recipe_does_not_abort_the_batch() {
    let plan_in = temp_dir("mixed-in");
    let plan_out = temp_dir("mixed-out");

    // Good recipe.
    let good = plan_in.join("static_bullet");
    fs::create_dir_all(&good).unwrap();
    for entry in fs::read_dir(bullet_dir()).unwrap().flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".json") {
            fs::copy(entry.path(), good.join(&name)).unwrap();
        }
    }

    // Bad recipe: names a spec that does not exist.
    let bad = plan_in.join("broken");
    fs::create_dir_all(&bad).unwrap();
    fs::write(
        bad.join("broken.recipe.json"),
        r#"{ "v": 1, "asset": "broken", "seed": 1, "palette": "p16",
             "sprites": [ { "id": "broken", "base": { "gen": "shapes", "spec": "missing.shapes.json" } } ],
             "tiles": [], "sheet": { "sources": [ "broken" ], "file": "broken.sheet.png" } }"#,
    )
    .unwrap();

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        plan_in.to_str().unwrap(),
        "--out",
        plan_out.to_str().unwrap(),
    ]);
    assert_eq!(
        code, 5,
        "the worst status must win\nstdout={stdout}\nstderr={stderr}"
    );

    let index: Value =
        serde_json::from_str(&fs::read_to_string(plan_out.join("index.json")).unwrap()).unwrap();
    let recipes = index["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 2, "both entries must be recorded");
    let by_id = |id: &str| {
        recipes
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("missing {id} in index.json"))
    };
    assert_eq!(by_id("static_bullet")["status"], "ok");
    assert_eq!(by_id("broken")["status"], "parse_error");
    assert!(by_id("broken")["error"]
        .as_str()
        .unwrap()
        .contains("missing.shapes.json"));
    // The good recipe's outputs were still published.
    assert!(plan_out
        .join("static_bullet/static_bullet.build.json")
        .is_file());
}

#[test]
fn dry_run_compiles_svg_and_strict_covers_variant_warnings() {
    let input = temp_dir("bad-svg-in");
    let output = temp_dir("bad-svg-out");
    fs::create_dir_all(input.join("broken")).unwrap();
    fs::write(input.join("broken/broken.recipe.json"), r#"{"v":1,"asset":"broken","gen_version":1,"palette":"p16","sprites":[{"id":"broken","base":{"gen":"sprite","spec":"broken.sprite.json"}}],"sheet":{"sources":["broken"],"file":"broken.sheet.png"}}"#).unwrap();
    fs::write(
        input.join("broken/broken.sprite.json"),
        r#"{"v":3,"size":32,"palette":"p16","source":{"kind":"svg","file":"broken.svg"}}"#,
    )
    .unwrap();
    fs::write(input.join("broken/broken.svg"), "<svg><").unwrap();
    let (code, stdout, _stderr) = pxl(&[
        "--json",
        "plan",
        "--in",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_ne!(code, 0, "bad SVG must not pass dry-run: {stdout}");
    assert!(!output.exists());

    let out = temp_dir("strict-morden").join("morden.png");
    let input = repo_root().join("examples/v3/morden/morden.sprite.json");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "--strict",
        "sprite",
        "build",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_ne!(
        code, 0,
        "variant warning must fail strict: {stdout}\n{stderr}"
    );
    assert!(format!("{stdout}\n{stderr}").contains("palette_over_tier"));
    assert!(
        !out.exists(),
        "failed bundle must not publish the primary PNG"
    );
}

