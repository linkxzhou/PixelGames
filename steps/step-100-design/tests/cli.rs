//! step-100 acceptance (plan/v3.0/pxl-step-pipeline-plan.md §8 M3):
//! init locks the pipeline, the fake designer run produces a full package and
//! waits for approval (exit 10), approval binds the manifest digest and
//! tampering trips integrity (exit 15), --until draft exits 0, and gate runs
//! without a model.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn step100(args: &[&str], fake_turns: Option<&Path>) -> (i32, String, String) {
    // Cargo keeps the hyphen in CARGO_BIN_EXE_<name> verbatim.
    let bin = env!("CARGO_BIN_EXE_step-100");
    let mut cmd = Command::new(bin);
    cmd.args(args);
    if let Some(script) = fake_turns {
        cmd.env("STEP_100_FAKE_TURNS", script);
    }
    let out = cmd.output().expect("step-100 binary");
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
    let dir = std::env::temp_dir().join(format!("step-100-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn init_workspace(tag: &str) -> PathBuf {
    let dir = temp(tag);
    let (code, _stdout, stderr) = step100(
        &[
            "init",
            "--brief",
            fixtures().join("brief.json").to_str().unwrap(),
            "--out",
            dir.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 0, "stderr={stderr}");
    dir
}

/// init writes the snapshot, the expanded template, and the pipeline lock
/// (with the slice passed through), and is idempotence-guarded.
#[test]
fn init_locks_the_pipeline() {
    let dir = init_workspace("init");
    for name in [
        "brief.snapshot.json",
        "template.json",
        "pipeline.json",
        "step.json",
    ] {
        assert!(dir.join(name).is_file(), "missing {name}");
    }
    let pipeline = read_json(&dir.join("pipeline.json"));
    assert_eq!(pipeline["game_id"], "fixture-game");
    assert_eq!(
        pipeline["steps"]["step-300-asset-build"]["contract"],
        "asset-build/v4"
    );
    assert_eq!(pipeline["slice"]["assets"][0], "player_tank");
    let step = read_json(&dir.join("step.json"));
    assert_eq!(step["step"], "step-100-design");
    assert_eq!(step["verdict"], "PASS");

    // Second init without --overwrite is exit 6.
    let (code, _o, _e) = step100(
        &[
            "init",
            "--brief",
            fixtures().join("brief.json").to_str().unwrap(),
            "--out",
            dir.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 6);

    // A bad game_id is a schema error; an unknown genre is semantic.
    let bad = temp("init-bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("brief.json"),
        r#"{"game_id":"X","genre":"top_down_tank"}"#,
    )
    .unwrap();
    let (code, _o, _e) = step100(
        &[
            "init",
            "--brief",
            bad.join("brief.json").to_str().unwrap(),
            "--out",
            temp("init-bad-out").to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 3);
    std::fs::write(
        bad.join("brief2.json"),
        r#"{"game_id":"ok-game","genre":"no_such_genre"}"#,
    )
    .unwrap();
    let (code, _o, _e) = step100(
        &[
            "init",
            "--brief",
            bad.join("brief2.json").to_str().unwrap(),
            "--out",
            temp("init-bad-out2").to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 4);
}

/// The scripted designer writes the whole package; the run ends in
/// waiting_approval with exit 10, gates green, and a revision manifest.
#[test]
fn fake_run_reaches_waiting_approval() {
    let dir = init_workspace("run");
    let (code, stdout, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 10, "stdout={stdout}\nstderr={stderr}");
    let out = read_last_json(&stdout);
    assert_eq!(out["status"], "waiting_approval");

    for name in [
        "game-design.json",
        "style-guide.json",
        "visual-intent.json",
        "levels/level_1.json",
        "asset-requirements.json",
        "acceptance-plan.json",
        "design-review.json",
        "revision.manifest.json",
        "reports/design.gates.json",
        "reports/design-review.md",
    ] {
        assert!(dir.join(name).is_file(), "missing {name}");
    }
    let gates = read_json(&dir.join("reports/design.gates.json"));
    assert_eq!(gates["status"], "passed", "gates must pass: {gates}");
    let revision = read_json(&dir.join("revision.manifest.json"));
    assert!(revision["files"].to_string().contains("visual-intent.json"));
    assert!(revision["files"].to_string().contains("knowledge.lock.json"));
    let knowledge = read_json(&dir.join("knowledge.lock.json"));
    assert_eq!(knowledge["contract"], "knowledge.lock/v1");
    assert_eq!(knowledge["entries"][0]["id"], "region-protection");
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("visual.intent"));
    let step = read_json(&dir.join("step.json"));
    assert_eq!(step["status"], "waiting_approval");
    assert_eq!(step["verdict"], "PASS");

    // status reports the same envelope.
    let (code, stdout, _e) = step100(&["status", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("waiting_approval"));
}

/// Approval binds the manifest digest; any later edit trips integrity.
#[test]
fn approve_binds_digest_and_tampering_trips_integrity() {
    let dir = init_workspace("approve");
    let (code, _o, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 10, "stderr={stderr}");

    let manifest = read_json(&dir.join("revision.manifest.json"));
    let digest = manifest["manifest_digest"].as_str().unwrap().to_string();

    // A wrong digest is refused with 15.
    let (code, _o, _e) = step100(
        &[
            "approve",
            "--in",
            dir.to_str().unwrap(),
            "--digest",
            "sha256:nope",
        ],
        None,
    );
    assert_eq!(code, 15);

    // The right digest approves; gate stays green afterwards.
    let (code, _o, _e) = step100(
        &[
            "approve",
            "--in",
            dir.to_str().unwrap(),
            "--digest",
            &digest,
        ],
        None,
    );
    assert_eq!(code, 0);
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0);

    // Tamper with one byte of the approved package: gate must report 15.
    let path = dir.join("game-design.json");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("move, shoot", "move, shoot ")).unwrap();
    let (code, _o, stderr) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 15, "tampering must be integrity, stderr={stderr}");
}

/// --until draft writes the raw package and exits 0 before any gate.
#[test]
fn until_draft_exits_zero() {
    let dir = init_workspace("draft");
    let (code, stdout, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap(), "--until", "draft"],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let dir = dir.join(".rejected");
    let step = read_json(&dir.join("step.json"));
    assert_eq!(step["status"], "draft");
    assert_eq!(step["verdict"], "NOT_ASSESSED");
    // No gates report and no revision manifest in draft mode.
    assert!(!dir.join("reports/design.gates.json").is_file());
    assert!(!dir.join("revision.manifest.json").is_file());
}

/// gate re-runs the gates with zero model involvement.
#[test]
fn gate_runs_without_a_model() {
    let dir = init_workspace("gate");
    let (code, _o, _e) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 10);
    // No STEP_100_FAKE_TURNS set: gate must not need a provider at all.
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0);
}

#[test]
fn gate_accepts_128_svg_and_rejects_fake_acceptance_checks() {
    let dir = init_workspace("svg128");
    let (code, _o, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 10, "stderr={stderr}");
    let path = dir.join("asset-requirements.json");
    let mut requirements = read_json(&path);
    requirements["requirements"][0]["size_px"] = serde_json::json!([128, 128]);
    requirements["requirements"][0]["strategy"] = serde_json::json!("svg");
    requirements["requirements"][0]["acceptance"][0]["expected"] = serde_json::json!([128, 128]);
    std::fs::write(&path, serde_json::to_string_pretty(&requirements).unwrap()).unwrap();
    let visual_path = dir.join("visual-intent.json");
    let mut visual = read_json(&visual_path);
    visual["asset_policies"][0]["native_size"] = serde_json::json!([128, 128]);
    std::fs::write(&visual_path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, stderr) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0, "128px SVG must be expressible: {stderr}");

    requirements["requirements"][0]["acceptance"][0]["check"] =
        serde_json::json!("not.a.real.check");
    std::fs::write(&path, serde_json::to_string_pretty(&requirements).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
}

#[test]
fn visual_reference_and_profile_digest_errors_fail_closed() {
    let dir = designed("visual-invalid");
    let path = dir.join("visual-intent.json");
    let original = read_json(&path);
    let mut visual = original.clone();
    visual["asset_policies"][0]["protected_regions"] = serde_json::json!(["not_a_region"]);
    std::fs::write(&path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("not_a_region"));

    visual = original;
    visual["profile"]["digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    std::fs::write(&path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("digest does not match"));
}

#[test]
fn legacy_contracts_and_migration_commands_are_rejected() {
    let dir = designed("legacy-rejected");
    let path = dir.join("game-design.json");
    let mut value = read_json(&path);
    value["schema_version"] = serde_json::json!(2);
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    let (code, _o, stderr) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12, "stderr={stderr}");
    let gates = read_json(&dir.join("reports/design.gates.json"));
    assert!(gates["checks"].to_string().contains("contract.design_v3"));
    let (code, _o, _e) = step100(&["upgrade", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 2);
    let (code, _o, _e) = step100(&["migrate", "--from", "old", "--out", "new"], None);
    assert_eq!(code, 2);
}

#[test]
fn lessons_analyze_preserves_evidence_and_candidate_status() {
    let run = temp("lesson-run");
    std::fs::create_dir_all(run.join("attempts/attempt-1")).unwrap();
    std::fs::write(run.join("attempts/attempt-1/attempt-manifest.json"), b"{}\n").unwrap();
    std::fs::write(
        run.join("events.jsonl"),
        concat!(
            "{\"seq\":0,\"ts_ms\":0,\"event\":\"tool_started\",\"call_id\":\"call-1\"}\n",
            "{\"seq\":1,\"ts_ms\":1,\"event\":\"tool_finished\",\"call_id\":\"call-1\",\"error_code\":\"E_QA\"}\n"
        ),
    )
    .unwrap();
    let out = temp("lesson-output");
    let (code, _o, stderr) = step100(&[
        "lessons",
        "--runs",
        run.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ], None);
    assert_eq!(code, 0, "stderr={stderr}");
    let report = read_json(&out.join("lesson-candidates.json"));
    assert_eq!(report["candidates"][0]["status"], "candidate");
    assert_eq!(report["promotion"], "manual_review_required");
}

#[test]
fn lessons_compare_reports_deltas_without_claiming_significance() {
    let root = temp("lesson-compare");
    std::fs::create_dir_all(&root).unwrap();
    let baseline = root.join("baseline.json");
    let candidate = root.join("candidate.json");
    std::fs::write(&baseline, r#"{"condition":"baseline","lock":"lock-a","lint_first_pass":4,"repairs":3,"structural_failures":1,"cost_tokens":100,"review_score":3}"#).unwrap();
    std::fs::write(&candidate, r#"{"condition":"candidate","lock":"lock-b","lint_first_pass":5,"repairs":2,"structural_failures":0,"cost_tokens":110,"review_score":4}"#).unwrap();
    let (code, _o, stderr) = step100(&[
        "lessons", "compare",
        "--baseline", baseline.to_str().unwrap(),
        "--candidate", candidate.to_str().unwrap(),
        "--runs", root.to_str().unwrap(),
        "--out", root.to_str().unwrap(),
    ], None);
    assert_eq!(code, 0, "stderr={stderr}");
    let report = read_json(&root.join("experiment-report.json"));
    assert_eq!(report["deltas"]["lint_first_pass"].as_f64(), Some(1.0));
    assert_eq!(report["deltas"]["review_score"].as_f64(), Some(1.0));
    assert_eq!(report["statistically_significant"], false);
    assert_eq!(report["holdout"], "NOT_ASSESSED");
}

#[test]
fn holdout_audit_separates_engineering_from_visual_review() {
    let out = temp("holdout-audit");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../libs/pxl-cli/tests/fixtures/visual-benchmark/manifest.json",
    );
    let (code, _o, stderr) = step100(&[
        "lessons", "audit",
        "--runs", manifest.to_str().unwrap(),
        "--out", out.to_str().unwrap(),
    ], None);
    assert_eq!(code, 0, "stderr={stderr}");
    let report = read_json(&out.join("holdout-report.json"));
    assert_eq!(report["cases"].as_array().unwrap().len(), 16);
    assert_eq!(report["engineering"], "PASS");
    assert_eq!(report["human_review"], "NOT_ASSESSED");
    assert_eq!(report["visual_improvement"], "NOT_ASSESSED");
}

fn read_last_json(stdout: &str) -> Value {
    let line = stdout.trim().lines().last().unwrap_or("");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
}

fn designed(tag: &str) -> PathBuf {
    let dir = init_workspace(tag);
    let (code, _o, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns.json")),
    );
    assert_eq!(code, 10, "stderr={stderr}");
    dir
}

fn write_audio(dir: &Path, value: Value) {
    std::fs::write(
        dir.join("audio-requirements.json"),
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();
}

/// A design without audio-requirements.json has no audio requirement: the gate
/// stays green and the envelope does not gain a NOT_ASSESSED row for it.
#[test]
fn design_without_audio_requirements_is_unaffected() {
    let dir = designed("audio-absent");
    assert!(!dir.join("audio-requirements.json").exists());
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0);
    let step = read_json(&dir.join("step.json"));
    assert_eq!(step["verdict"], "PASS");
    let rows = step["gates"]["required_artifacts"].to_string();
    assert!(!rows.contains("audio-requirements"), "{rows}");
}

#[test]
fn audio_requirements_are_part_of_the_approved_revision() {
    // The designer writes audio-requirements.json through design.write_candidate,
    // exactly as a model would.
    let dir = init_workspace("audio-valid");
    let (code, _o, stderr) = step100(
        &["run", "--in", dir.to_str().unwrap()],
        Some(&fixtures().join("fake_turns_audio.json")),
    );
    assert_eq!(code, 10, "{stderr}");
    assert!(dir.join("audio-requirements.json").is_file());

    let gates = read_json(&dir.join("reports/design.gates.json"));
    let check = gates["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "audio.requirements")
        .expect("audio.requirements check present");
    assert_eq!(check["passed"], true);
    assert_eq!(check["value"], "2 sounds");

    let manifest = read_json(&dir.join("revision.manifest.json"));
    let digest = manifest["manifest_digest"].as_str().unwrap().to_string();
    assert!(manifest["files"]
        .to_string()
        .contains("audio-requirements.json"));

    let (code, _o, stderr) = step100(
        &[
            "approve",
            "--in",
            dir.to_str().unwrap(),
            "--digest",
            &digest,
        ],
        None,
    );
    assert_eq!(code, 0, "{stderr}");
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0);

    // Editing the audio requirements after approval must invalidate it.
    write_audio(
        &dir,
        serde_json::json!({
            "schema_version": 1,
            "requirements": [
                {"audio_id": "tank_fire", "kind": "sfx", "required": true, "trigger": "player_fire"}
            ]
        }),
    );
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 15, "approved digest must reject an edited audio file");
}

#[test]
fn invalid_audio_requirements_fail_the_gate() {
    let dir = designed("audio-bad");
    let asset_id = read_json(&dir.join("asset-requirements.json"))["requirements"][0]["asset_id"]
        .as_str()
        .unwrap()
        .to_string();
    let cases = [
        // sfx that loops
        serde_json::json!({"schema_version": 1, "requirements": [
            {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "x", "loop": true}]}),
        // duplicate ids
        serde_json::json!({"schema_version": 1, "requirements": [
            {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "x"},
            {"audio_id": "beep", "kind": "sfx", "required": false, "trigger": "y"}]}),
        // too long for an sfx
        serde_json::json!({"schema_version": 1, "requirements": [
            {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "x", "max_seconds": 5}]}),
        // collides with an asset id
        serde_json::json!({"schema_version": 1, "requirements": [
            {"audio_id": asset_id, "kind": "sfx", "required": true, "trigger": "x"}]}),
        // unknown field
        serde_json::json!({"schema_version": 1, "requirements": [
            {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "x", "typo": 1}]}),
        // wrong version
        serde_json::json!({"schema_version": 2, "requirements": [
            {"audio_id": "beep", "kind": "sfx", "required": true, "trigger": "x"}]}),
    ];
    for (i, case) in cases.iter().enumerate() {
        write_audio(&dir, case.clone());
        let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
        assert_eq!(code, 12, "case {i} must fail the gate: {case}");
    }
}

/// A visual-intent/v2 with the modern profile passes the gate when its
/// logical_canvas and tile_size match the style guide, and fails when they
/// disagree. v1 documents keep their old behavior untouched.
#[test]
fn gate_accepts_modern_v2_and_enforces_canvas_and_tile_consistency() {
    let dir = designed("modern-v2");
    let visual_path = dir.join("visual-intent.json");
    let mut visual = read_json(&visual_path);
    visual["contract"] = serde_json::json!("visual-intent/v2");
    visual["modern"] = serde_json::json!({
        "profile_contract": "modern-pixel/v1",
        "logical_canvas": [320, 240],
        "tile_size": [16, 16],
        "target_runtime": "runtime-js",
        "sampling": "nearest_integer",
        "budgets": { "palette_entries": 48, "frame_opaque_colors": 16, "ramp_depth": 5, "scene_rgb": 128 }
    });
    visual["scene_previews"][0]["size"] = serde_json::json!([320, 240]);
    std::fs::write(&visual_path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, stderr) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 0, "v2 with matching canvas must pass: {stderr}");

    let mut visual = read_json(&visual_path);
    visual["modern"]["logical_canvas"] = serde_json::json!([640, 360]);
    std::fs::write(&visual_path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("does not match style-guide logical_resolution"));

    let mut visual = read_json(&visual_path);
    visual["modern"]["logical_canvas"] = serde_json::json!([320, 240]);
    visual["modern"]["tile_size"] = serde_json::json!([32, 16]);
    std::fs::write(&visual_path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("must be square and equal style-guide tile_size_px"));
}

/// v1 documents that grow a scene beyond 256 still fail; only v2 may use the
/// 4096 logical canvas.
#[test]
fn gate_keeps_v1_scene_limit() {
    let dir = designed("v1-scene-limit");
    let visual_path = dir.join("visual-intent.json");
    let mut visual = read_json(&visual_path);
    visual["scene_previews"][0]["size"] = serde_json::json!([300, 300]);
    std::fs::write(&visual_path, serde_json::to_string_pretty(&visual).unwrap()).unwrap();
    let (code, _o, _e) = step100(&["gate", "--in", dir.to_str().unwrap()], None);
    assert_eq!(code, 12);
    assert!(read_json(&dir.join("reports/design.gates.json"))["checks"]
        .to_string()
        .contains("1..=256"));
}
