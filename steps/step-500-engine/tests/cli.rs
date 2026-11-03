//! step-500 acceptance (plan/v3.0/pxl-step-pipeline-plan.md §8 M5):
//! the five engine-contract assertions, and the four-value slice verdict —
//! missing evidence is never a pass.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn step500(args: &[&str]) -> (i32, String, String) {
    let bin = env!("CARGO_BIN_EXE_step-500");
    let out = Command::new(bin)
        .args(args)
        .output()
        .expect("step-500 binary");
    (
        out.status.code().unwrap_or(1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("step-500-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn sha(bytes: &[u8]) -> String {
    format!("sha256:{}", pxl_store::digest::sha256_hex(bytes))
}

/// A minimal but complete package: one sheet with one tagged animation, a
/// palette, one level, and matching digests.
fn package(root: &Path) -> PathBuf {
    let dir = root.join("pkg");
    fs::create_dir_all(dir.join("sheets")).unwrap();
    fs::create_dir_all(dir.join("levels")).unwrap();

    let palette = br##"{"v":1,"id":"p16","colors":["#00000000","#ffffffff"]}"##.as_slice();
    fs::write(dir.join("palette.json"), palette).unwrap();

    let sheet = br#"{"v":1,"w":32,"h":16,"frames":{"move_0":{"x":0,"y":0,"w":16,"h":16}},"meta":{"image":"units.sheet.png","frameTags":{"move":{"from":0,"to":0}}}}"#.as_slice();
    fs::write(dir.join("sheets/units.sheet.json"), sheet).unwrap();
    fs::write(dir.join("sheets/units.sheet.png"), b"fixture-png").unwrap();

    let level = br#"{"schema_version":1,"id":"level_1","grid":["ff"],"tile_semantics":[{"tile_id":"player_tank","behaviour":"floor","blocks":false}]}"#.as_slice();
    fs::write(dir.join("levels/level_1.json"), level).unwrap();
    let visual_report = br#"{"contract":"visual-report/v1","verdict":"PASS","assets":[]}"#;
    let visual_metadata = br#"{"contract":"visual-metadata/v1","assets":[]}"#;
    let preview_manifest = br#"{"contract":"preview-manifest/v1","previews":[]}"#;
    let effective_style = br#"{"contract":"effective-style/v1","scene_previews":[{"id":"main","size":[64,64],"background":"dark","scale":2,"tick":0,"instances":[{"asset_id":"player_tank","position":[8,8],"layer":"world","frame":null}],"rois":[]}]}"#;
    for (name, bytes) in [
        ("visual-report.json", visual_report.as_slice()),
        ("visual-metadata.json", visual_metadata.as_slice()),
        ("preview-manifest.json", preview_manifest.as_slice()),
        ("effective-style.json", effective_style.as_slice()),
    ] {
        fs::write(dir.join(name), bytes).unwrap();
    }

    let pkg = json!({
        "schema_version": 3,
        "package_id": "fixture@abc123",
        "engine": { "name": "pxl-web", "contract": "engine/v4", "frame_max": 256, "palette_max": 62 },
        "palette": { "path": "palette.json", "digest": sha(palette) },
        "palettes": [{ "path": "palette.json", "digest": sha(palette) }],
        "sheets": [{
            "id": "units",
            "path": "sheets/units.sheet.json",
            "digest": sha(sheet),
            "frames": 1,
            "frame_tags": ["move"],
            "palette": { "path": "palette.json", "digest": sha(palette) },
        }],
        "assets": [{
            "id": "player_tank",
            "kind": "animated",
            "sheet": "units",
            "role": "actor",
            "pivot": [8, 15],
            "channels": [],
            "anims": { "move": { "frames": 1, "fps": 8, "loop": true } },
        }],
        "visual": {
            "report": { "path": "visual-report.json", "digest": sha(visual_report) },
            "metadata": { "path": "visual-metadata.json", "digest": sha(visual_metadata) },
            "preview_manifest": { "path": "preview-manifest.json", "digest": sha(preview_manifest) },
            "effective_style": { "path": "effective-style.json", "digest": sha(effective_style) },
            "required_capabilities": ["pivot", "duration"],
            "builds": [{
                "recipe": "units",
                "report": { "path": "visual-report.json", "digest": sha(visual_report) },
                "metadata": { "path": "visual-metadata.json", "digest": sha(visual_metadata) },
                "preview_manifest": { "path": "preview-manifest.json", "digest": sha(preview_manifest) },
                "effective_style": { "path": "effective-style.json", "digest": sha(effective_style) },
            }],
        },
        "levels": { "path": "levels", "digest": sha(level) },
    });
    fs::write(
        dir.join("asset-package.json"),
        serde_json::to_string_pretty(&pkg).unwrap(),
    )
    .unwrap();
    dir
}

fn add_audio(package: &Path, required: bool) {
    fs::create_dir_all(package.join("assets/audio/bgm")).unwrap();
    let wav = b"fixture-wav";
    let manifest = b"{\"contract\":\"audio-manifest/v1\"}";
    fs::write(package.join("assets/audio/bgm/theme.wav"), wav).unwrap();
    fs::write(package.join("assets/audio/audio-manifest.json"), manifest).unwrap();
    let path = package.join("asset-package.json");
    let mut document = read_json(&path);
    document["schema_version"] = json!(3);
    document["audio_manifest"] = json!({
        "path": "assets/audio/audio-manifest.json", "digest": sha(manifest),
    });
    document["audio"] = json!([{
        "id": "theme", "kind": "bgm", "required": required,
        "path": "assets/audio/bgm/theme.wav", "digest": sha(wav),
        "pcm_sha256": sha(b"pcm"), "deps_sha256": sha(b"deps"),
        "sample_rate": 44100, "channels": 2, "frames": 1000,
        "loop": {"start_frame": 0, "end_frame_exclusive": 1000},
        "qa": {"status": "pass"},
    }]);
    fs::write(path, serde_json::to_string_pretty(&document).unwrap()).unwrap();
}

/// All seven assertions pass on a well-formed package.
#[test]
fn check_passes_on_a_valid_package() {
    let root = temp("check-ok");
    let pkg = package(&root);
    let (code, stdout, stderr) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = read_json(&pkg.join("engine-check.json"));
    assert_eq!(report["ok"], true);
    let checks = report["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 7);
    assert!(checks.iter().all(|c| c["result"] == "pass"), "{checks:?}");
}

/// No level reference means the tile assertion cannot be evaluated: 11,
/// not a silent pass.
#[test]
fn check_blocks_when_levels_cannot_be_assessed() {
    let root = temp("check-no-levels");
    let pkg = package(&root);
    let mut value = read_json(&pkg.join("asset-package.json"));
    value.as_object_mut().unwrap().remove("levels");
    fs::write(
        pkg.join("asset-package.json"),
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();
    let (code, stdout, _e) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 11, "stdout={stdout}");
    let report = read_json(&pkg.join("engine-check.json"));
    assert_eq!(report["ok"], false);
}

/// A changed sheet byte fails the file assertion with 5.
#[test]
fn check_fails_on_tampered_sheet() {
    let root = temp("check-tamper");
    let pkg = package(&root);
    fs::write(pkg.join("sheets/units.sheet.json"), b"{}").unwrap();
    let (code, stdout, _e) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 5, "stdout={stdout}");
}

/// An animation the sheet cannot satisfy fails the anim assertion.
#[test]
fn check_fails_on_unsatisfied_animation() {
    let root = temp("check-anim");
    let pkg = package(&root);
    let mut value = read_json(&pkg.join("asset-package.json"));
    value["assets"][0]["anims"]["dance"] = json!({"frames": 2, "fps": 8, "loop": true});
    fs::write(
        pkg.join("asset-package.json"),
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();
    let (code, stdout, _e) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 5, "stdout={stdout}");
}

/// The slice gate: all four artifacts plus green checks -> PROCEED.
#[test]
fn slice_proceeds_when_evidence_is_complete() {
    let root = temp("slice-ok");
    let work = slice_work(&root, 0);
    let out = root.join("650-slice-gate");
    let (code, stdout, stderr) = step500(&[
        "slice",
        "--work",
        work.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = read_json(&out.join("slice-report.json"));
    assert_eq!(report["verdict"], "PROCEED");
    assert!(report["missing"].as_array().unwrap().is_empty());
    assert_eq!(report["next"], "step-200 run --scope all");
}

/// One missing artifact forces NOT_ASSESSED (11) — never PROCEED.
#[test]
fn slice_not_assessed_when_evidence_is_missing() {
    let root = temp("slice-missing");
    let work = slice_work(&root, 0);
    fs::remove_file(work.join("500-engine/.slice/engine-check.json")).unwrap();
    let out = root.join("650-slice-gate");
    let (code, stdout, _e) = step500(&[
        "slice",
        "--work",
        work.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 11, "stdout={stdout}");
    let report = read_json(&out.join("slice-report.json"));
    assert_eq!(report["verdict"], "NOT_ASSESSED");
    assert_eq!(report["missing"].as_array().unwrap().len(), 1);
}

/// More than three repair rounds means the spec is wrong: PIVOT.
#[test]
fn slice_pivots_after_too_many_repairs() {
    let root = temp("slice-pivot");
    let work = slice_work(&root, 4);
    let out = root.join("650-slice-gate");
    let (code, stdout, _e) = step500(&[
        "slice",
        "--work",
        work.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "stdout={stdout}");
    let report = read_json(&out.join("slice-report.json"));
    assert_eq!(report["verdict"], "PIVOT");
    assert_eq!(report["questions"][2]["answer"], 4);
}

/// A .slice/ workspace with the four required artifacts; `attempts` failed
/// lines are dropped in.
fn slice_work(root: &Path, attempts: usize) -> PathBuf {
    let work = root.join("work");
    let specs = work.join("200-art-spec/.slice");
    let builds = work.join("300-build/.slice");
    let pkgs = work.join("400-package/.slice");
    let checks = work.join("500-engine/.slice");
    for dir in [&specs, &builds, &pkgs, &checks] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(
        specs.join("spec-manifest.json"),
        serde_json::to_string(&json!({
            "schema_version": 1, "scope": "slice",
            "assets": [{ "id": "player_tank", "kind": "shapes", "status": "ok" }],
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        builds.join("index.json"),
        serde_json::to_string(&json!({
            "schema_version": 1, "scope": "slice",
            "recipes": [{ "id": "player_tank", "status": "ok", "out": "player_tank/" }],
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        pkgs.join("asset-package.json"),
        serde_json::to_string(&json!({
            "schema_version": 1, "package_id": "fixture@abc",
            "engine": { "contract": "engine/v1" },
            "palette": {}, "sheets": [], "assets": [],
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        checks.join("engine-check.json"),
        serde_json::to_string(&json!({
            "schema_version": 1, "package_id": "fixture@abc", "ok": true, "checks": [],
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(checks.join("engine-build.json"), b"{}").unwrap();
    fs::create_dir_all(checks.join("playtest")).unwrap();
    fs::write(
        checks.join("playtest/report.json"),
        serde_json::to_string(&json!({"status": "PASS", "build_digest": sha(b"{}")})).unwrap(),
    )
    .unwrap();
    let mut text = String::new();
    for i in 0..attempts {
        text.push_str(&format!(
            "{{\"asset\":\"player_tank\",\"issue\":\"round {i}\",\"result\":\"failed\"}}\n"
        ));
    }
    fs::write(pkgs.join("attempts.jsonl"), text).unwrap();
    work
}

fn runtime(root: &Path) -> PathBuf {
    let dir = root.join("runtime");
    fs::create_dir_all(dir.join("template")).unwrap();
    fs::write(dir.join("pxl_web.wasm"), b"wasm-fixture").unwrap();
    fs::write(dir.join("mq_js_bundle.js"), b"").unwrap();
    fs::write(dir.join("index.js"), b"").unwrap();
    fs::write(dir.join("pxl_runtime.js"), b"").unwrap();
    fs::write(dir.join("assets.js"), b"").unwrap();
    fs::write(dir.join("showcase.js"), b"").unwrap();
    fs::write(
        dir.join("template/index.html"),
        b"<canvas id=glcanvas></canvas>",
    )
    .unwrap();
    dir
}

fn game(root: &Path) -> PathBuf {
    let dir = root.join("game");
    fs::create_dir_all(dir.join("replay")).unwrap();
    fs::write(dir.join("main.js"), b"// game").unwrap();
    fs::write(dir.join("replay/playtest.json"), b"{}").unwrap();
    fs::write(
        dir.join("headless.js"),
        br#"import fs from 'node:fs';
const a=process.argv; const out=a[a.indexOf('--out')+1]; const report=a[a.indexOf('--report')+1];
fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(report, JSON.stringify({panicked:false,frames:1,expected_frames:1,palette_mismatches:0,player_moved:true,shots:1,bricks_destroyed:1,enemies_destroyed:0,state_hash:'fixed',ok:true}));
"#,
    )
    .unwrap();
    dir
}

#[test]
fn legacy_engine_contract_is_rejected() {
    let root = temp("engine-v1");
    let pkg = package(&root);
    let engine = root.join("engine.json");
    fs::write(
        &engine,
        br#"{"contract":"engine/v1","limits":{"frame_max":256,"palette_max":62}}"#,
    )
    .unwrap();
    let (code, _out, _err) = step500(&[
        "check",
        "--in",
        pkg.to_str().unwrap(),
        "--engine",
        engine.to_str().unwrap(),
    ]);
    assert_eq!(code, 5);
    let report = read_json(&pkg.join("engine-check.json"));
    let check = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "engine_contract")
        .unwrap();
    assert_eq!(check["result"], "fail");
    assert!(check["detail"].as_str().unwrap().contains("engine/v4"));
}

#[test]
fn build_and_headless_playtest_verify_manifest_and_determinism() {
    let root = temp("build-playtest");
    let pkg = package(&root);
    add_audio(&pkg, true);
    let runtime = runtime(&root);
    let game = game(&root);
    let out = root.join("500-engine");
    let (code, stdout, stderr) = step500(&[
        "build",
        "--in",
        pkg.to_str().unwrap(),
        "--game",
        game.to_str().unwrap(),
        "--runtime-dir",
        runtime.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let manifest = read_json(&out.join("engine-build.json"));
    assert_eq!(manifest["contract"], "engine/v4");
    assert_eq!(manifest["mode_was_default"], true);
    assert_eq!(manifest["audio"]["entries"], 1);
    assert_eq!(manifest["audio"]["decoded_bytes"], 8000);
    assert!(out.join("web/assets/audio/bgm/theme.wav").is_file());
    let packaged = read_json(&out.join("web/assets/asset-package.json"));
    assert_eq!(packaged["audio"][0]["path"], "audio/bgm/theme.wav");
    for file in manifest["files"].as_array().unwrap() {
        let path = out.join("web").join(file["path"].as_str().unwrap());
        assert_eq!(
            file["digest"].as_str().unwrap(),
            sha(&fs::read(path).unwrap())
        );
    }
    let (code, stdout, stderr) = step500(&[
        "playtest",
        "--in",
        out.to_str().unwrap(),
        "--mode",
        "headless",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = read_json(&out.join("playtest/report.json"));
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["deterministic"], true);
    assert_eq!(report["build_digest"], manifest_digest(&out));
}

fn manifest_digest(out: &Path) -> String {
    sha(&fs::read(out.join("engine-build.json")).unwrap())
}

#[test]
fn build_without_game_creates_visual_showcase() {
    let root = temp("showcase");
    let pkg = package(&root);
    let runtime = runtime(&root);
    let out = root.join("500-engine");
    let (code, stdout, stderr) = step500(&[
        "build",
        "--in",
        pkg.to_str().unwrap(),
        "--runtime-dir",
        runtime.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let manifest = read_json(&out.join("engine-build.json"));
    assert_eq!(manifest["visual"], true);
    assert!(out.join("web/showcase.js").is_file());
    assert!(out.join("web/assets/visual/units/effective-style.json").is_file());
    let packaged = read_json(&out.join("web/assets/asset-package.json"));
    assert_eq!(
        packaged["visual"]["effective_style"]["path"],
        "visual/units/effective-style.json"
    );
    let style = read_json(&out.join("web/assets/visual/units/effective-style.json"));
    assert_eq!(style["scene_previews"][0]["instances"][0]["asset_id"], "player_tank");
}

#[test]
fn build_refuses_overwrite_and_missing_kernel() {
    let root = temp("build-negative");
    let pkg = package(&root);
    let runtime = runtime(&root);
    let game = game(&root);
    let out = root.join("500-engine");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("keep"), b"keep").unwrap();
    let (code, _stdout, _stderr) = step500(&[
        "build",
        "--in",
        pkg.to_str().unwrap(),
        "--game",
        game.to_str().unwrap(),
        "--runtime-dir",
        runtime.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 4);
    assert_eq!(fs::read(out.join("keep")).unwrap(), b"keep");
    fs::remove_dir_all(&out).unwrap();
    fs::remove_file(runtime.join("pxl_web.wasm")).unwrap();
    let (code, _stdout, stderr) = step500(&[
        "build",
        "--in",
        pkg.to_str().unwrap(),
        "--game",
        game.to_str().unwrap(),
        "--runtime-dir",
        runtime.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 4);
    assert!(stderr.contains("pxl_web.wasm"));
}

#[test]
fn slice_rejects_stale_playtest_evidence() {
    let root = temp("slice-stale");
    let work = slice_work(&root, 0);
    fs::write(work.join("500-engine/.slice/engine-build.json"), b"changed").unwrap();
    let out = root.join("650-slice-gate");
    let (code, _stdout, _stderr) = step500(&[
        "slice",
        "--work",
        work.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 11);
    let report = read_json(&out.join("slice-report.json"));
    assert_eq!(report["verdict"], "NOT_ASSESSED");
    assert!(report["missing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str().unwrap().contains("stale")));
}

#[test]
fn audio_capability_checks_js_rust_and_optional_audio() {
    let root = temp("audio-capability");
    let pkg = package(&root);
    add_audio(&pkg, true);
    let (code, stdout, _) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");
    let report = read_json(&pkg.join("engine-check.json"));
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| { check["id"] == "audio_capability" && check["result"] == "pass" }));

    let engine = root.join("rust-engine.json");
    fs::write(
        &engine,
        serde_json::to_string_pretty(&json!({
            "contract": "engine/v4", "runtime": {"mode": "rust"},
            "capabilities": {
                "audio": {"js": true, "rust": false, "formats": ["wav"]},
                "visual": {"pivot": true, "duration": true}
            },
            "limits": {"frame_max": 256, "palette_max": 62},
        }))
        .unwrap(),
    )
    .unwrap();
    let (code, _, _) = step500(&[
        "check",
        "--in",
        pkg.to_str().unwrap(),
        "--engine",
        engine.to_str().unwrap(),
    ]);
    assert_eq!(code, 5);

    add_audio(&pkg, false);
    let (code, stdout, _) = step500(&[
        "check",
        "--in",
        pkg.to_str().unwrap(),
        "--engine",
        engine.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}");
    let report = read_json(&pkg.join("engine-check.json"));
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| { check["id"] == "audio_capability" && check["result"] == "warning" }));
}

#[test]
fn audio_tampering_and_missing_files_are_not_silent() {
    let root = temp("audio-integrity");
    let pkg = package(&root);
    add_audio(&pkg, true);
    fs::write(pkg.join("assets/audio/bgm/theme.wav"), b"tampered").unwrap();
    let (code, _, _) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 5);
    add_audio(&pkg, true);
    fs::remove_file(pkg.join("assets/audio/bgm/theme.wav")).unwrap();
    let (code, _, _) = step500(&["check", "--in", pkg.to_str().unwrap()]);
    assert_eq!(code, 11);
}
