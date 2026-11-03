//! step-200 acceptance (plan/v3.0/pxl-step-pipeline-plan.md §8 M2):
//! authored specs land on disk, a missing asset blocks (never aborts), slice
//! scope filters, lint validates through step-300's dry run, and no build
//! tool exists anywhere.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn step200(args: &[&str], fake_turns: Option<&Path>) -> (i32, String, String) {
    // Cargo keeps the hyphen in CARGO_BIN_EXE_<name> verbatim.
    let bin = env!("CARGO_BIN_EXE_step-200");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    if let Some(script) = fake_turns {
        cmd.env("STEP_200_FAKE_TURNS", script);
    }
    let out = cmd.output().expect("step-200 binary");
    (
        out.status.code().unwrap_or(1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("step-200-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The fake provider authors every asset: exit 0, manifest all ok, envelope
/// PASS, palette at the root.
#[test]
fn run_authors_all_specs() {
    let out = temp("all");
    let (code, stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&fixtures().join("fake_turns_full.json")),
    );
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");

    let manifest = read_json(&out.join("spec-manifest.json"));
    let assets = manifest["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 2);
    assert!(assets.iter().all(|a| a["status"] == "ok"));
    assert_eq!(manifest["scope"], "all");
    assert_eq!(manifest["palette"], "palette.json");
    assert!(out.join("palette.json").is_file());
    assert!(out.join("visual-intent.snapshot.json").is_file());
    assert!(out.join("visual-plan.json").is_file());
    assert!(out.join("bullet/bullet.recipe.json").is_file());
    assert!(out.join("gem/gem.shapes.json").is_file());
    assert!(manifest["visual_plan"]["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    let model = read_json(&out.join("model-run.json"));
    let run_dir = out.parent().unwrap().join("_runs").join(format!(
        "{}-{}",
        out.file_name().unwrap().to_string_lossy(),
        model["run_id"].as_str().unwrap()
    ));
    assert!(run_dir.join("attempts/attempt-1/visual-plan.json").is_file());
    assert!(run_dir.join("attempts/attempt-1/attempt-manifest.json").is_file());

    let step = read_json(&out.join("step.json"));
    assert_eq!(step["step"], "step-200-art-spec");
    assert_eq!(step["verdict"], "PASS");
    assert_eq!(step["status"], "ok");
    assert_eq!(step["gates"]["not_assessed"].as_array().unwrap().len(), 0);
}

/// Missing or invalid assets block with exit 11 and FAIL. Their evidence stays
/// in .rejected instead of replacing the last approved candidate.
#[test]
fn missing_asset_blocks_but_never_aborts() {
    let out = temp("partial");
    let (code, _stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&fixtures().join("fake_turns_partial.json")),
    );
    assert_eq!(code, 11, "stderr={stderr}");
    let out = out.join(".rejected");

    let manifest = read_json(&out.join("spec-manifest.json"));
    let assets = manifest["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0]["id"], "bullet");
    assert_eq!(assets[0]["status"], "invalid");
    assert_eq!(assets[1]["id"], "gem");
    assert_eq!(assets[1]["status"], "missing");

    let step = read_json(&out.join("step.json"));
    assert_eq!(step["verdict"], "FAIL");
    assert_eq!(step["status"], "blocked");
    assert_eq!(step["gates"]["not_assessed"].as_array().unwrap().len(), 0);
}

/// --scope slice reads pipeline.json and keeps only slice.assets[].
#[test]
fn scope_slice_filters_requirements() {
    let out = temp("slice");
    let (code, _stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--scope",
            "slice",
        ],
        Some(&fixtures().join("fake_turns_full.json")),
    );
    assert_eq!(code, 0, "stderr={stderr}");

    let manifest = read_json(&out.join("spec-manifest.json"));
    assert_eq!(manifest["scope"], "slice");
    let assets = manifest["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1, "only the slice asset may appear");
    assert_eq!(assets[0]["id"], "bullet");
}

/// lint re-digests the manifest and pushes the whole tree through
/// `step-300 plan --dry-run`; nothing is built and nothing is written outside
/// the reports directory.
#[test]
fn lint_validates_through_step300_dry_run() {
    let out = temp("lint");
    let (code, _stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&fixtures().join("fake_turns_full.json")),
    );
    assert_eq!(code, 0, "stderr={stderr}");

    let pxl = pxl_bin();
    let (code, stdout, stderr) = step200(
        &[
            "lint",
            "--in",
            out.to_str().unwrap(),
            "--pxl-bin",
            pxl.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = read_json(&out.join("reports/art-spec.lint.json"));
    assert_eq!(report["ok"], true);
    let checks = report["checks"].as_array().unwrap();
    assert!(checks.len() >= 5, "spec+recipe per asset plus the dry run");
    assert!(checks.iter().all(|c| c["result"] == "pass"));
    // The dry run must not have produced a build directory.
    assert!(!out.join("build").exists());
}

/// Calling a build tool is an unknown-tool error the loop survives; the
/// source-level guarantee lives in tools.rs::no_build_tool_is_registered.
#[test]
fn calling_a_build_tool_is_rejected_not_silently_added() {
    let turns = temp("build-tool-turns");
    std::fs::write(
        &turns,
        r##"[[
            ["assets.build_candidate", { "recipe": "bullet/bullet.recipe.json" }]
        ],[
            ["assets.write_candidate", { "path": "palette.json", "content": "{\"v\":1,\"id\":\"p16\",\"colors\":[\"#00000000\",\"#140c1c\",\"#442434\",\"#30346d\",\"#4e4a4e\",\"#854c30\",\"#346524\",\"#d04648\",\"#757161\",\"#597dce\",\"#d27d2c\",\"#8595a1\",\"#6daa2c\",\"#d2aa99\",\"#6dc2ca\",\"#deeed6\"],\"ramps\":{},\"cycles\":{}}" }]
        ]]"##,
    )
    .unwrap();
    let out = temp("build-tool");
    let (code, _stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&turns),
    );
    // The loop finished (the palette landed, both assets are missing), the
    // unknown tool never became a build, and the exit code is the blocked
    // code from the missing assets — not a crash.
    assert_eq!(code, 11, "stderr={stderr}");
    let out = out.join(".rejected");
    assert!(out.join("palette.json").is_file());
    let manifest = read_json(&out.join("spec-manifest.json"));
    assert!(manifest["assets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["status"] == "missing"));
    // And nothing was built: no PNG anywhere under the spec output.
    let mut pngs = Vec::new();
    collect_pngs(&out, &mut pngs);
    assert!(pngs.is_empty(), "step-200 must never produce pixels");
}

fn collect_pngs(dir: &Path, acc: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_pngs(&path, acc);
        } else if path.extension().and_then(|e| e.to_str()) == Some("png") {
            acc.push(path);
        }
    }
}

fn run_with_turns(tag: &str, turns: &str) -> (i32, PathBuf) {
    let out = temp(tag);
    let (code, _stdout, stderr) = step200(
        &[
            "run",
            "--in",
            fixtures().join("design").to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        Some(&fixtures().join(turns)),
    );
    eprintln!("{tag}: code={code} stderr={stderr}");
    (code, out)
}

/// Without audio the manifest has no `audio` key and the lint checks are the
/// same ones as before: audio is purely additive.
#[test]
fn specs_without_audio_are_unchanged() {
    let (code, out) = run_with_turns("no-audio", "fake_turns_full.json");
    assert_eq!(code, 0);
    let manifest = read_json(&out.join("spec-manifest.json"));
    assert!(manifest.get("audio").is_none());
    let lint = read_json(&out.join("reports/art-spec.lint.json"));
    assert!(!lint["checks"].to_string().contains("audio."));
}

/// A valid audio spec and its song land in the manifest with digests, lint
/// passes without Node or any render, and no wav is produced.
#[test]
fn audio_spec_is_digested_linted_and_never_rendered() {
    let (code, out) = run_with_turns("audio-ok", "fake_turns_audio.json");
    assert_eq!(code, 0);
    let manifest = read_json(&out.join("spec-manifest.json"));
    let audio = &manifest["audio"];
    assert_eq!(audio["contract"], "audio-spec/v1");
    assert_eq!(audio["status"], "ok");
    let deps: Vec<&str> = audio["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        deps,
        ["audio/audio.spec.json", "audio/songs/theme.beepbox.json"]
    );
    assert!(audio["dependencies"][0]["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    let lint = read_json(&out.join("reports/art-spec.lint.json"));
    assert_eq!(lint["ok"], true);
    let ids: Vec<&str> = lint["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .filter(|id| id.starts_with("audio."))
        .collect();
    assert!(ids.contains(&"audio.spec.valid"), "{ids:?}");
    assert!(
        ids.contains(&"audio.dependency.audio/songs/theme.beepbox.json"),
        "{ids:?}"
    );

    let mut wavs = Vec::new();
    collect_ext(&out, "wav", &mut wavs);
    assert!(wavs.is_empty(), "step-200 must never render audio");
}

/// A spec that breaks a limit blocks the step and the evidence goes to
/// .rejected, exactly like an invalid sprite spec.
#[test]
fn invalid_audio_spec_blocks_the_step() {
    let (code, out) = run_with_turns("audio-bad", "fake_turns_audio_bad.json");
    assert_eq!(code, 11);
    let rejected = out.join(".rejected");
    let manifest = read_json(&rejected.join("spec-manifest.json"));
    assert_eq!(manifest["audio"]["status"], "invalid");
    assert!(manifest["audio"]["error"]
        .as_str()
        .unwrap()
        .contains("frequency_hz"));
    let lint = read_json(&rejected.join("reports/art-spec.lint.json"));
    let failed: Vec<&str> = lint["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["result"] == "fail")
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(failed.contains(&"audio.spec.valid"), "{failed:?}");
}

/// Editing a song after the step finished breaks its recorded digest.
#[test]
fn lint_detects_an_edited_song() {
    let (code, out) = run_with_turns("audio-edit", "fake_turns_audio.json");
    assert_eq!(code, 0);
    let song = out.join("audio/songs/theme.beepbox.json");
    let text = std::fs::read_to_string(&song).unwrap();
    std::fs::write(
        &song,
        text.replace("\"key\": \"C\"", "\"key\": \"D\"")
            .replace("\"key\":\"C\"", "\"key\":\"D\""),
    )
    .unwrap();
    let pxl = pxl_bin();
    let (code, _stdout, _stderr) = step200(
        &[
            "lint",
            "--in",
            out.to_str().unwrap(),
            "--pxl-bin",
            pxl.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 11);
    let report = read_json(&out.join("reports/art-spec.lint.json"));
    assert!(report["checks"].as_array().unwrap().iter().any(|c| c["id"]
        == "audio.dependency.audio/songs/theme.beepbox.json"
        && c["result"] == "fail"));
}

fn collect_ext(dir: &Path, ext: &str, acc: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ext(&path, ext, acc);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            acc.push(path);
        }
    }
}

#[test]
fn unapproved_or_tampered_design_is_rejected() {
    let design = temp("unapproved-design");
    std::fs::create_dir_all(&design).unwrap();
    for entry in std::fs::read_dir(fixtures().join("design"))
        .unwrap()
        .flatten()
    {
        std::fs::copy(entry.path(), design.join(entry.file_name())).unwrap();
    }
    std::fs::remove_file(design.join("approval.json")).unwrap();
    let (code, _o, _e) = step200(
        &[
            "run",
            "--in",
            design.to_str().unwrap(),
            "--out",
            temp("unapproved-out").to_str().unwrap(),
        ],
        Some(&fixtures().join("fake_turns_full.json")),
    );
    assert_eq!(code, 15);

    std::fs::copy(
        fixtures().join("design/approval.json"),
        design.join("approval.json"),
    )
    .unwrap();
    let path = design.join("asset-requirements.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("projectile", "changed after approval")).unwrap();
    let (code, _o, _e) = step200(
        &[
            "run",
            "--in",
            design.to_str().unwrap(),
            "--out",
            temp("tampered-out").to_str().unwrap(),
        ],
        Some(&fixtures().join("fake_turns_full.json")),
    );
    assert_eq!(code, 15);
}

#[test]
fn visual_plan_builds_effective_style_and_region_protection() {
    let (code, out) = run_with_turns("visual-build", "fake_turns_full.json");
    assert_eq!(code, 0);
    let build = out.parent().unwrap().join("visual-build-output");
    let output = Command::new(pxl_bin())
        .arg("--json")
        .arg("plan")
        .arg("--in")
        .arg(&out)
        .arg("--out")
        .arg(&build)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for id in ["bullet", "gem"] {
        let effective = read_json(&build.join(id).join("effective-style.json"));
        assert_eq!(effective["contract"], "effective-style/v1");
        assert_eq!(effective["assets"][0]["palette"]["mode"], "strict");
        assert_eq!(effective["assets"][0]["palette"]["extension"], 0);
        assert!(effective["assets"][0]["frames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|frame| frame["protect_pixels"].as_u64().unwrap_or(0) > 0));
    }
}

/// Locate the sibling `step-300` binary.
///
/// Order: an explicit `PXL_STEP_300_BIN` file, then the directory that holds
/// this test's own `step-200` (same target dir and profile, so a relative or
/// custom `CARGO_TARGET_DIR` resolves the same way Cargo resolved it). An
/// explicit path that is missing is an error, never a silent fallback.
fn resolve_sibling(explicit: Option<&str>, own_bin: &Path, name: &str) -> Result<PathBuf, String> {
    if let Some(raw) = explicit.filter(|raw| !raw.trim().is_empty()) {
        let path = PathBuf::from(raw);
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir().map_err(|e| e.to_string())?.join(path)
        };
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("explicit {name} binary {} does not exist", path.display()))
        };
    }
    let dir = own_bin
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", own_bin.display()))?;
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} is missing; run `cargo build --locked -p pxl-cli --bin {name}` with the same CARGO_TARGET_DIR, or set PXL_STEP_300_BIN",
            path.display()
        ))
    }
}

fn pxl_bin() -> PathBuf {
    let explicit = std::env::var("PXL_STEP_300_BIN").ok();
    resolve_sibling(
        explicit.as_deref(),
        Path::new(env!("CARGO_BIN_EXE_step-200")),
        "step-300",
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn sibling_binary_resolution_rejects_missing_paths() {
    let own = Path::new(env!("CARGO_BIN_EXE_step-200"));
    let missing = temp("no-such-bin").join("step-300");
    let error = resolve_sibling(Some(missing.to_str().unwrap()), own, "step-300").unwrap_err();
    assert!(error.contains("does not exist"), "{error}");
    let error = resolve_sibling(None, &temp("empty-target").join("step-200"), "step-300").unwrap_err();
    assert!(error.contains("is missing"), "{error}");
    let found = resolve_sibling(None, own, "step-200").unwrap();
    assert_eq!(found, own);
}
