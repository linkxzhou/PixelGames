//! step-400 acceptance (plan/v3.0/pxl-step-pipeline-plan.md §8 M4):
//! coverage over required assets, content addressing against the build
//! manifests, missing files block (11), a changed byte is integrity (15),
//! run and check are idempotent, and approval binds the manifest digest.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn step400(args: &[&str]) -> (i32, String, String) {
    let bin = env!("CARGO_BIN_EXE_step-400");
    let out = Command::new(bin)
        .args(args)
        .output()
        .expect("step-400 binary");
    (
        out.status.code().unwrap_or(1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("step-400-{tag}-{}", std::process::id()));
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

fn write_test_review(out: &Path, verdict: &str) {
    let package = read_json(&out.join("asset-package.json"));
    let revision = read_json(&out.join("revision.manifest.json"));
    let dimension = |status: &str| {
        if status == "pass" {
            json!({"status": "pass", "score": 4, "evidence": ["preview-manifest.json#fixture"]})
        } else {
            json!({"status": "concerns", "score": 3, "evidence": ["preview-manifest.json#fixture"]})
        }
    };
    let status = if verdict == "CONCERNS" { "concerns" } else { "pass" };
    let review = json!({
        "contract": "visual-review/v1",
        "scale_version": "test-rubric-v1",
        "revision_digest": revision["manifest_digest"],
        "subject_digest": package["visual"]["report"]["digest"],
        "reviewers": [{"id": "test-reviewer-a"}, {"id": "test-reviewer-b"}],
        "dimensions": {
            "silhouette": dimension(status),
            "colour_value": dimension(status),
            "material_form": dimension(status),
            "pixel_clusters": dimension(status),
            "animation": dimension(status),
        },
        "verdict": verdict,
        "exceptions": [],
    });
    fs::write(
        out.join("visual-review.json"),
        serde_json::to_string_pretty(&review).unwrap(),
    )
    .unwrap();
}

/// Build a step-300-style tree by hand: index.json, one recipe directory,
/// one published sprite plus a sheet and the palette, each file digested the
/// way build_cmd writes them.
fn fixture(root: &Path) -> (PathBuf, PathBuf) {
    let build = root.join("300-build");
    let out = build.join("units");
    fs::create_dir_all(&out).unwrap();
    let files = [
        (
            "player_tank.pxl.json",
            br#"{"v":3,"w":16,"h":16}"#.as_slice(),
        ),
        ("player_tank.sheet.png", b"PNGFAKE".as_slice()),
        ("player_tank.sheet.json", br#"{"frames":{}}"#.as_slice()),
        ("effective-style.json", br#"{"contract":"effective-style/v1","assets":[]}"#.as_slice()),
        ("visual-report.json", br#"{"contract":"visual-report/v1","verdict":"PASS","visual_review":"not_run","assets":[]}"#.as_slice()),
        ("visual-metadata.json", br#"{"contract":"visual-metadata/v1","assets":[{"asset_id":"player_tank","role":"actor","native_size":[16,16],"pivot":[8,15],"channels":[],"animations":{}}]}"#.as_slice()),
        ("preview-manifest.json", br#"{"contract":"preview-manifest/v1","previews":[]}"#.as_slice()),
        (
            "palette.json",
            br##"{"v":1,"id":"p16","colors":["#00000000","#ffffffff"]}"##.as_slice(),
        ),
    ];
    let mut outputs = serde_json::Map::new();
    for (name, bytes) in files {
        fs::write(out.join(name), bytes).unwrap();
        outputs.insert(name.to_string(), json!(sha(bytes)));
    }
    let manifest = json!({
        "schema_version": 1,
        "pxl_version": "0.1.0",
        "gen_version": 1,
        "seed": 5,
        "inputs": { "recipe": sha(b"recipe"), "palette": sha(b"palette"), "specs": {} },
        "outputs": outputs,
        "qa": { "rules": "default", "config": {}, "ok": true, "checks": 5, "failed": 0 },
        "visual_review": "not_run",
    });
    fs::write(
        out.join("units.build.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let index = json!({
        "schema_version": 4,
        "scope": "all",
        "recipes": [{
            "id": "units",
            "recipe": "units/units.recipe.json",
            "status": "ok",
            "out": "units/",
            "manifest": "units.build.json",
        }],
    });
    fs::write(
        build.join("index.json"),
        serde_json::to_string_pretty(&index).unwrap(),
    )
    .unwrap();

    let requirements = root.join("requirements.json");
    fs::write(
        &requirements,
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "requirements": [{
                "asset_id": "player_tank",
                "kind": "sprite",
                "required": true,
                "consumer_refs": ["level_1"],
                "size_px": [16, 16],
                "directions": ["N"],
                "anims": [{ "name": "move", "fps": 8, "looped": true, "frames": 4 }],
                "strategy": "shapes",
            }],
        }))
        .unwrap(),
    )
    .unwrap();
    (build, requirements)
}

fn run_package(root: &Path, build: &Path, requirements: &Path) -> (i32, String, PathBuf) {
    let out = root.join("400-package");
    let (code, stdout, stderr) = step400(&[
        "run",
        "--in",
        build.to_str().unwrap(),
        "--requirements",
        requirements.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--game-id",
        "fixture",
    ]);
    if code != 0 && code != 11 && code != 15 {
        panic!("unexpected exit {code}\nstdout={stdout}\nstderr={stderr}");
    }
    (code, stdout, out)
}

/// The happy path: coverage 1/1, a package the engine can read, a pending
/// revision, and no recorded failures.
#[test]
fn run_packages_a_valid_build() {
    let root = temp("valid");
    let (build, requirements) = fixture(&root);
    let (code, stdout, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0, "stdout={stdout}");

    let coverage = read_json(&out.join("coverage.json"));
    assert_eq!(coverage["required"], 1);
    assert_eq!(coverage["delivered"], 1);
    assert!(coverage["missing"].as_array().unwrap().is_empty());

    let package = read_json(&out.join("asset-package.json"));
    assert_eq!(package["schema_version"], 4);
    assert_eq!(package["engine"]["contract"], "engine/v4");
    assert_eq!(package["release_status"], "experimental");
    assert_eq!(package["resources"].as_array().unwrap().len(), 0);
    assert!(package["required_capabilities"].as_array().unwrap().contains(&json!("duration")));
    assert!(package["package_id"]
        .as_str()
        .unwrap()
        .starts_with("fixture@"));
    assert_eq!(package["sheets"].as_array().unwrap().len(), 1);
    assert_eq!(package["assets"][0]["id"], "player_tank");
    assert_eq!(package["assets"][0]["kind"], "animated");
    assert_eq!(package["assets"][0]["anims"]["move"]["frames"], 4);
    assert!(package["palette"]["digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    let revision = read_json(&out.join("revision.manifest.json"));
    assert_eq!(revision["status"], "pending");
    assert!(revision["manifest_digest"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));

    let step = read_json(&out.join("step.json"));
    assert_eq!(step["verdict"], "PASS");
    assert_eq!(step["status"], "ok");
    // attempts.jsonl exists and is empty on a clean run.
    assert_eq!(fs::read_to_string(out.join("attempts.jsonl")).unwrap(), "");
}

/// A required asset with no build output blocks with 11 and is named.
#[test]
fn missing_required_asset_blocks() {
    let root = temp("missing-asset");
    let (build, requirements) = fixture(&root);
    let mut req: Value = read_json(&requirements);
    req["requirements"].as_array_mut().unwrap().push(json!({
        "asset_id": "gem",
        "kind": "sprite",
        "required": true,
        "consumer_refs": ["level_1"],
        "size_px": [8, 8],
        "anims": [],
        "strategy": "shapes",
    }));
    fs::write(&requirements, serde_json::to_string_pretty(&req).unwrap()).unwrap();

    let (code, stdout, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 11, "stdout={stdout}");
    let coverage = read_json(&out.join("coverage.json"));
    assert_eq!(coverage["missing"], json!(["gem"]));
    let step = read_json(&out.join("step.json"));
    assert_eq!(step["verdict"], "NOT_ASSESSED");
    let attempts = fs::read_to_string(out.join("attempts.jsonl")).unwrap();
    assert!(attempts.contains("\"gem\""));
}

/// A file the build manifest claims but the disk does not have is a
/// delivery gap: 11, not tampering.
#[test]
fn deleted_build_output_blocks() {
    let root = temp("deleted");
    let (build, requirements) = fixture(&root);
    fs::remove_file(build.join("units/player_tank.sheet.png")).unwrap();
    let (code, stdout, _out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 11, "stdout={stdout}");
}

/// A changed byte breaks the manifest digest: 15.
#[test]
fn tampered_build_output_is_integrity() {
    let root = temp("tampered");
    let (build, requirements) = fixture(&root);
    fs::write(build.join("units/player_tank.sheet.png"), b"PNGFake").unwrap();
    let (code, stdout, _out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 15, "stdout={stdout}");
    let _ = stdout;
}

/// run is idempotent: the same inputs produce the same package and the same
/// revision digest, and check passes afterwards.
#[test]
fn run_is_idempotent_and_check_passes() {
    let root = temp("idem");
    let (build, requirements) = fixture(&root);
    let (code, _o, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0);
    let first = read_json(&out.join("asset-package.json"));
    let first_rev = read_json(&out.join("revision.manifest.json"));

    let (code, _o, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0);
    let second = read_json(&out.join("asset-package.json"));
    let second_rev = read_json(&out.join("revision.manifest.json"));
    assert_eq!(first, second, "package must be byte-stable");
    assert_eq!(
        first_rev["manifest_digest"], second_rev["manifest_digest"],
        "revision digest must be stable"
    );

    let (code, stdout, stderr) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
}

/// check recomputes the digests of everything the package references and
/// catches edits made after packaging.
#[test]
fn check_detects_post_package_tampering() {
    let root = temp("check-tamper");
    let (build, requirements) = fixture(&root);
    let (code, _o, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0);
    let (code, _o, _e) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0);

    // Editing a file the package points at moves its digest.
    fs::write(
        build.join("units/palette.json"),
        br##"{"v":1,"colors":[]}"##,
    )
    .unwrap();
    let (code, stdout, _e) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 15, "stdout={stdout}");

    // Deleting it is a delivery gap.
    fs::remove_file(build.join("units/palette.json")).unwrap();
    let (code, _o, _e) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 11);
}

#[test]
fn levels_use_directory_digests_and_detect_tampering() {
    let root = temp("levels");
    let (build, requirements) = fixture(&root);
    let levels = root.join("input-levels");
    fs::create_dir_all(&levels).unwrap();
    fs::write(levels.join("level_1.json"), r#"{"tile_semantics":[]}"#).unwrap();
    let out = root.join("400-package");
    let (code, stdout, stderr) = step400(&[
        "run",
        "--in",
        build.to_str().unwrap(),
        "--requirements",
        requirements.to_str().unwrap(),
        "--levels",
        levels.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let (code, stdout, stderr) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    fs::write(
        out.join("levels/level_1.json"),
        r#"{"tile_semantics":[],"changed":true}"#,
    )
    .unwrap();
    let (code, _o, _e) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 15);
}

/// approve binds the revision digest; the wrong digest is refused with 15.
#[test]
fn approve_binds_the_revision_digest() {
    let root = temp("approve");
    let (build, requirements) = fixture(&root);
    let (code, _o, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0);
    let digest = read_json(&out.join("revision.manifest.json"))["manifest_digest"]
        .as_str()
        .unwrap()
        .to_string();

    let (code, _o, _e) = step400(&[
        "approve",
        "--in",
        out.to_str().unwrap(),
        "--digest",
        "sha256:nope",
    ]);
    assert_eq!(code, 15);

    let (code, _o, _e) = step400(&[
        "approve",
        "--in",
        out.to_str().unwrap(),
        "--digest",
        &digest,
    ]);
    assert_eq!(code, 11, "missing visual review must block approval");
    write_test_review(&out, "PASS");
    let (code, _o, stderr) = step400(&[
        "approve",
        "--in",
        out.to_str().unwrap(),
        "--digest",
        &digest,
    ]);
    assert_eq!(code, 0, "stderr={stderr}");
    let revision = read_json(&out.join("revision.manifest.json"));
    assert_eq!(revision["status"], "approved");
    assert!(out.join("approval.json").is_file());
}

#[test]
fn accepted_concerns_remain_concerns_after_approval() {
    let root = temp("concerns");
    let (build, requirements) = fixture(&root);
    let (code, _o, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0);
    write_test_review(&out, "CONCERNS");
    let digest = read_json(&out.join("revision.manifest.json"))["manifest_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, _o, stderr) = step400(&[
        "approve",
        "--in",
        out.to_str().unwrap(),
        "--digest",
        &digest,
    ]);
    assert_eq!(code, 0, "stderr={stderr}");
    let revision = read_json(&out.join("revision.manifest.json"));
    assert_eq!(revision["visual_verdict"], "CONCERNS");
    let approval = read_json(&out.join("approval.json"));
    assert_eq!(approval["visual_verdict"], "CONCERNS");
}

fn add_audio_fixture(root: &Path, build: &Path) {
    let audio = build.join("audio");
    fs::create_dir_all(audio.join("sfx")).unwrap();
    fs::create_dir_all(audio.join("bgm")).unwrap();
    let mono: Vec<i16> = (0..1000)
        .map(|i| {
            ((i as f64 * 441.0 * std::f64::consts::TAU / 44100.0).sin() * 12000.0).round() as i16
        })
        .collect();
    let stereo: Vec<i16> = mono.iter().flat_map(|sample| [*sample, *sample]).collect();
    let mono_wav = pxl_audio::wav::encode(&mono, 1).unwrap();
    let stereo_wav = pxl_audio::wav::encode(&stereo, 2).unwrap();
    fs::write(audio.join("sfx/beep.wav"), &mono_wav).unwrap();
    fs::write(audio.join("bgm/theme.wav"), &stereo_wav).unwrap();
    let pcm = |samples: &[i16]| {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let entry = |id: &str,
                 kind: &str,
                 path: &str,
                 wav: &[u8],
                 pcm: &[u8],
                 channels: u16,
                 looped: bool| {
        json!({
            "id": id,
            "kind": kind,
            "path": path,
            "engine": if kind == "sfx" { "zzfx" } else { "beepbox" },
            "seed": 7,
            "channels": channels,
            "frames": 1000,
            "file_sha256": sha(wav),
            "pcm_sha256": sha(pcm),
            "deps_sha256": sha(id.as_bytes()),
            "loop": looped.then_some(json!({"start_frame": 0, "end_frame_exclusive": 1000})),
            "qa": {"status": "pass", "peak_dbfs": -8.7, "rms_dbfs": -11.7, "clipped_samples": 0, "failed": []},
        })
    };
    let manifest = json!({
        "schema_version": 1,
        "contract": "audio-manifest/v1",
        "asset": "fixture_audio",
        "root_seed": 7,
        "sample_rate": 44100,
        "pcm_format": "pcm_s16le",
        "spec_sha256": sha(b"spec"),
        "toolchain": {
            "node": "v25.8.0", "adapter_version": "1.0.0", "adapter_sha256": sha(b"adapter"),
            "beepbox": "4.2.0", "zzfx": "1.4.0", "os": "test", "arch": "test",
        },
        "qa_profile": "audio-qa/v1",
        "entries": [
            entry("beep", "sfx", "sfx/beep.wav", &mono_wav, &pcm(&mono), 1, false),
            entry("theme", "bgm", "bgm/theme.wav", &stereo_wav, &pcm(&stereo), 2, true),
        ],
    });
    fs::write(
        audio.join("audio-manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let mut index = read_json(&build.join("index.json"));
    index["schema_version"] = json!(4);
    index["audio"] = json!({
        "contract": "audio-spec/v1", "status": "ok", "out": "audio/",
        "manifest": "audio-manifest.json", "entries": 2, "warnings": [],
    });
    fs::write(
        build.join("index.json"),
        serde_json::to_string_pretty(&index).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("audio-requirements.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "requirements": [
                {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "fire"},
                {"audio_id": "theme", "kind": "bgm", "required": true, "trigger": "stage", "loop": true},
            ],
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn audio_is_covered_packaged_and_bound_into_revision_integrity() {
    let root = temp("audio-valid");
    let (build, requirements) = fixture(&root);
    add_audio_fixture(&root, &build);
    let (code, stdout, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 0, "stdout={stdout}");
    let coverage = read_json(&out.join("coverage.json"));
    assert_eq!(coverage["audio"]["required"], 2);
    assert_eq!(coverage["audio"]["delivered"], 2);
    let package = read_json(&out.join("asset-package.json"));
    assert_eq!(package["audio"].as_array().unwrap().len(), 2);
    assert!(out.join("assets/audio/sfx/beep.wav").is_file());
    assert!(out.join("assets/audio/bgm/theme.wav").is_file());
    assert_eq!(package["audio"][1]["loop"]["end_frame_exclusive"], 1000);
    let revision = read_json(&out.join("revision.manifest.json"));
    assert!(revision["files"]
        .to_string()
        .contains("assets/audio/audio-manifest.json"));
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");
    fs::write(out.join("assets/audio/sfx/beep.wav"), b"tampered").unwrap();
    let (code, _, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 15);
}

#[test]
fn missing_required_audio_blocks_coverage() {
    let root = temp("audio-missing");
    let (build, requirements) = fixture(&root);
    add_audio_fixture(&root, &build);
    let manifest_path = build.join("audio/audio-manifest.json");
    let mut manifest = read_json(&manifest_path);
    manifest["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["id"] != "theme");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::remove_file(build.join("audio/bgm/theme.wav")).unwrap();
    let (code, stdout, out) = run_package(&root, &build, &requirements);
    assert_eq!(code, 11, "stdout={stdout}");
    assert_eq!(
        read_json(&out.join("coverage.json"))["audio"]["missing"],
        json!(["theme"])
    );
}

// ---- P06: scenes, resource map, closure, self-containment, release states ----

/// Write a compiled scene file plus the manifest resources entry that
/// realises its instance, so the scene->resource->sheet->PNG chain closes.
fn add_scene_fixture(root: &Path, build: &Path, review: &str) -> PathBuf {
    let scenes = root.join("scenes-src");
    fs::create_dir_all(&scenes).unwrap();
    let resource_id = "res-environment-player-tank";
    let scene = json!({
        "contract": "compiled-scene/v1",
        "id": "scene_plaza",
        "version": 1,
        "bounds": [128, 128],
        "instances": [{
            "id": "inst-0001",
            "resource_id": resource_id,
            "layer": "back",
            "sort_y": 32,
            "at": [0, 0],
        }],
        "collision": { "2,2": "solid" },
        "seed": 7,
        "depends": { "terrain_grass": sha(b"tileset") },
        "license": "original",
        "review": review,
    });
    let scene_path = scenes.join("scene_plaza.scene.json");
    fs::write(&scene_path, serde_json::to_string_pretty(&scene).unwrap()).unwrap();
    // The closure walks scene -> resource -> sheet json -> PNG, so the
    // fixture sheet must name its image.
    let sheet_path = build.join("units/player_tank.sheet.json");
    fs::write(
        &sheet_path,
        br#"{"frames":{"player_tank":{}},"meta":{"image":"player_tank.sheet.png"}}"#,
    )
    .unwrap();
    let manifest_path = build.join("units/units.build.json");
    let mut manifest = read_json(&manifest_path);
    let sheet_digest = sha(br#"{"frames":{"player_tank":{}},"meta":{"image":"player_tank.sheet.png"}}"#);
    manifest["outputs"]["player_tank.sheet.json"] = json!(sheet_digest);
    manifest["resources"] = json!([{
        "asset_id": "player_tank",
        "resource_id": resource_id,
        "palette_family": "environment",
        "frames": [{"name": "player_tank", "index": 0}],
        "sheet": "player_tank.sheet.png",
        "sheet_json": "player_tank.sheet.json",
        "document": "player_tank.pxl.json",
        "source_digest": sha(b"player_tank.pxl.json"),
    }]);
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    scenes
}

fn run_package_with_scenes(root: &Path, build: &Path, requirements: &Path, scenes: &Path) -> (i32, String, PathBuf) {
    let out = root.join("400-package");
    let (code, stdout, stderr) = step400(&[
        "run",
        "--in", build.to_str().unwrap(),
        "--requirements", requirements.to_str().unwrap(),
        "--scenes", scenes.to_str().unwrap(),
        "--out", out.to_str().unwrap(),
        "--game-id", "fixture",
    ]);
    (code, format!("{stdout}\n{stderr}"), out)
}

/// Scenes are copied in, the resource map is published, the closure closes
/// and the package stays readable after it is moved to another directory.
#[test]
fn scenes_are_packaged_self_contained_and_readable_after_a_move() {
    let root = temp("scenes-ok");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 0, "stdout={stdout}");

    let package = read_json(&out.join("asset-package.json"));
    assert_eq!(package["scenes"].as_array().unwrap().len(), 1);
    assert_eq!(package["scenes"][0]["id"], "scene_plaza");
    assert_eq!(package["scenes"][0]["path"], "scenes/scene_plaza.scene.json");
    assert_eq!(package["scenes"][0]["review"], "approved");
    assert!(out.join("scenes/scene_plaza.scene.json").is_file());
    assert!(package["required_capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("compiled-scene/v1")));
    let resources = package["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["resource_id"], "res-environment-player-tank");
    assert_eq!(resources[0]["palette_family"], "environment");

    // check passes in place; moving package and build tree together into
    // another workspace directory keeps every relative reference readable.
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");
    let workspace = root.join("another-workspace");
    fs::create_dir_all(&workspace).unwrap();
    let moved = workspace.join("400-package");
    let moved_build = workspace.join("300-build");
    fs::rename(&out, &moved).unwrap();
    fs::rename(&build, &moved_build).unwrap();
    let (code, stdout, _) = step400(&["check", "--in", moved.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");
}

/// A scene instance naming a resource no build produced is integrity (15).
#[test]
fn scene_resource_gap_is_integrity_not_silent() {
    let root = temp("scenes-gap");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    // Drop the resources block: the scene now references nothing.
    let manifest_path = build.join("units/units.build.json");
    let mut manifest = read_json(&manifest_path);
    manifest["resources"] = json!([]);
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 15, "stdout={stdout}");
    let step = read_json(&out.join("step.json"));
    let detail: String = step["gates"]["quality_checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|gate| gate["id"] == "integrity")
        .filter_map(|gate| gate["detail"].as_str().map(String::from))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        detail.contains("res-environment-player-tank"),
        "the gap must name the missing resource:\n{detail}"
    );
}

/// Tampering a packaged scene after the run fails check with 15.
#[test]
fn tampered_scene_fails_check() {
    let root = temp("scenes-tamper");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 0, "stdout={stdout}");
    let scene_file = out.join("scenes/scene_plaza.scene.json");
    let mut scene = read_json(&scene_file);
    scene["seed"] = json!(999);
    fs::write(&scene_file, serde_json::to_string_pretty(&scene).unwrap()).unwrap();
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 15, "stdout={stdout}");
}

/// A packaged scene path escaping the package root fails check with 15.
#[test]
fn escaping_scene_path_fails_check() {
    let root = temp("scenes-escape");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 0, "stdout={stdout}");
    let package_path = out.join("asset-package.json");
    let mut package = read_json(&package_path);
    package["scenes"][0]["path"] = json!("../../outside/scene.json");
    fs::write(&package_path, serde_json::to_string_pretty(&package).unwrap()).unwrap();
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 15, "stdout={stdout}");
    assert!(stdout.contains("package-internal"));
}

/// approve refuses a package whose scenes are not human-approved, and a
/// successful approval flips release_status to publishable.
#[test]
fn approval_requires_approved_scenes_and_flips_release_status() {
    let root = temp("scenes-approve");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 0, "stdout={stdout}");
    write_test_review(&out, "PASS");
    let digest = read_json(&out.join("revision.manifest.json"))["manifest_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, stdout, stderr) = step400(&[
        "approve", "--in", out.to_str().unwrap(), "--digest", &digest,
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let package = read_json(&out.join("asset-package.json"));
    assert_eq!(package["release_status"], "publishable");
    // The revision aggregate is rebound after the status flip.
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");

    // A pending scene blocks approval entirely.
    let root2 = temp("scenes-approve-pending");
    let (build2, requirements2) = fixture(&root2);
    let scenes2 = add_scene_fixture(&root2, &build2, "pending");
    let (code, stdout, out2) = run_package_with_scenes(&root2, &build2, &requirements2, &scenes2);
    assert_eq!(code, 0, "stdout={stdout}");
    write_test_review(&out2, "PASS");
    let current = read_json(&out2.join("revision.manifest.json"))["manifest_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, stdout, stderr) = step400(&[
        "approve", "--in", out2.to_str().unwrap(), "--digest", &current,
    ]);
    assert_eq!(code, 4, "stdout={stdout}\nstderr={stderr}");
    let package2 = read_json(&out2.join("asset-package.json"));
    assert_eq!(package2["release_status"], "experimental");
}

/// A failing rerun does not overwrite the last publishable package.
#[test]
fn failing_rerun_keeps_the_last_good_package() {
    let root = temp("scenes-fail-keeps");
    let (build, requirements) = fixture(&root);
    let scenes = add_scene_fixture(&root, &build, "approved");
    let (code, stdout, out) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 0, "stdout={stdout}");
    let good_package = fs::read_to_string(out.join("asset-package.json")).unwrap();

    // Break the build output; the rerun fails with 15 and must leave the
    // previous package untouched.
    fs::write(build.join("units/player_tank.sheet.png"), b"tampered").unwrap();
    let (code, _stdout, _stderr) = run_package_with_scenes(&root, &build, &requirements, &scenes);
    assert_eq!(code, 15);
    let after = fs::read_to_string(out.join("asset-package.json")).unwrap();
    assert_eq!(after, good_package, "a failed run must not touch the published package");
    let (code, stdout, _) = step400(&["check", "--in", out.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout={stdout}");
    assert!(!out.join(".staging").exists(), "staging must be cleaned up");
}
