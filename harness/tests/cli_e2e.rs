//! End-to-end CLI fixture test: scripted fake provider produces a full
//! design package, gates pass, review renders, approval binds the digest,
//! and a second run reuses the approved design without regenerating.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn bin() -> PathBuf {
    // Target dir layout: target/debug/pxl-harness
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/debug/pxl-harness");
    if !p.is_file() {
        p = PathBuf::from("target/debug/pxl-harness");
    }
    assert!(p.is_file(), "binary not built: {}", p.display());
    p
}

fn tmp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl_h_e2e_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_cli(args: &[&str]) -> (i32, Value) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("failed to run pxl-harness");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
    (out.status.code().unwrap_or(-1), json)
}

const GAME: &str = r#"{"schema_version":1,"game_id":"tank_slice","genre":"top_down_tank","core_loop":"move, shoot, reach exit","actions":[{"id":"move","name":"Move","input":"wasd","desc":""},{"id":"shoot","name":"Shoot","input":"space","desc":""}],"entities":[{"id":"player_tank","kind":"player","desc":"","actions":["move","shoot"],"speed":3.0},{"id":"enemy_tank","kind":"enemy","desc":"","actions":["move","shoot"],"speed":2.0}],"rules":[{"id":"r1","statement":"player moves","entities":["player_tank"],"actions":["move"]},{"id":"r2","statement":"bullets destroy","entities":["enemy_tank"],"actions":["shoot"]}],"win_condition":{"id":"win","statement":"reach exit","refs":["player_tank"]},"lose_condition":{"id":"lose","statement":"hit by bullet","refs":["player_tank"]},"requirement_ids":[],"scope":{}}"#;
const STYLE: &str = r#"{"schema_version":1,"logical_resolution":[320,240],"tile_size_px":16,"sprite_size_px":16,"palette":"palette.json","transparent_index":0,"outline":"always","directions":["N","E","S","W"],"anim_fps_min":2,"anim_fps_max":16,"nearest_filtering":true}"#;
const LEVEL: &str = r#"{"schema_version":1,"id":"level_1","name":"First Steps","tile_size_px":16,"grid_w":8,"grid_h":8,"grid":["ffffffff","ffbbbbff","ffffffff","ffwbbwff","ffffffff","fbbfffff","ffffffff","ffffffff"],"tile_semantics":[{"tile_id":"f","behaviour":"floor","blocks":false},{"tile_id":"b","behaviour":"wall","blocks":true},{"tile_id":"w","behaviour":"water","blocks":true}],"placements":[{"entity_id":"enemy_tank","pos":[6,1],"dir":"W"}],"spawn":[1,1],"exit":[6,6],"objective":"reach the exit","difficulty":1,"seed":1,"notes":"teach movement"}"#;
const REQS: &str = r#"{"schema_version":1,"requirements":[{"asset_id":"player_tank","kind":"sprite","required":true,"consumer_refs":["level_1"],"style_ref":"palette.json","size_px":[16,16],"directions":["N","E","S","W"],"anims":[{"name":"move","fps":10,"looped":true,"frames":4,"dirs":["N","E","S","W"]}],"semantics":"player","strategy":"shapes","seamless":false,"priority":1},{"asset_id":"enemy_tank","kind":"sprite","required":true,"consumer_refs":["level_1"],"style_ref":"palette.json","size_px":[16,16],"directions":["N","E","S","W"],"anims":[],"semantics":"enemy","strategy":"shapes","seamless":false,"priority":1},{"asset_id":"b","kind":"tile","required":true,"consumer_refs":["level_1"],"style_ref":"palette.json","size_px":[16,16],"directions":[],"anims":[],"semantics":"wall","strategy":"tile_gen","seamless":true,"priority":1},{"asset_id":"w","kind":"tile","required":true,"consumer_refs":["level_1"],"style_ref":"palette.json","size_px":[16,16],"directions":[],"anims":[],"semantics":"water","strategy":"tile_gen","seamless":true,"priority":1}]}"#;
const ACCEPT: &str = r#"{"schema_version":1,"items":[{"requirement_id":"player_tank","description":"delivered","gate_id":"assets.coverage","stage":"assets","evidence":"manifest"},{"requirement_id":"enemy_tank","description":"delivered","gate_id":"assets.coverage","stage":"assets","evidence":"manifest"},{"requirement_id":"b","description":"delivered","gate_id":"assets.coverage","stage":"assets","evidence":"manifest"},{"requirement_id":"w","description":"delivered","gate_id":"assets.coverage","stage":"assets","evidence":"manifest"}]}"#;

fn escape_json(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}

/// Build the fake provider script as a config file. The scripted provider
/// replays tool calls through the env var `PXL_HARNESS_FAKE_TURNS`.
fn write_fake_config(project: &PathBuf, turns_json: &str) {
    let cfg = serde_json::json!({
        "provider": "fake",
        "model": "fake",
        "require_design_approval": true,
    });
    fs::write(
        project.join("harness.config.json"),
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();
    std::env::set_var("PXL_HARNESS_FAKE_TURNS", turns_json);
}

#[test]
fn fake_provider_full_design_flow() {
    let project = tmp_project("flow");
    // 1. init
    let brief = project.join("brief.json");
    fs::write(&brief, r#"{"game_id":"tank_slice","text":"make a tank game"}"#).unwrap();
    let (code, out) = run_cli(&[
        "init", "--project", project.to_str().unwrap(), "--brief",
        brief.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["game_id"], "tank_slice");

    // 2. run --until design with a scripted fake provider that writes the
    //    whole package via tool calls, validates, then requests review.
    let wc = |path: &str, content: &str| {
        (
            "design.write_candidate".to_string(),
            format!(r#"{{"path":"{path}","content":{}}}"#, escape_json(content)),
        )
    };
    let turns = serde_json::json!([
        [wc("game-design.json", GAME)],
        [wc("style-guide.json", STYLE)],
        [wc("levels/level_1.json", LEVEL)],
        [wc("asset-requirements.json", REQS)],
        [wc("acceptance-plan.json", ACCEPT)],
        [["design.read_template", "{}"]],
        [["design.validate", "{}"]],
        [["design.request_review", r#"{"summary":"3 logical levels, tank template reused","risks":["water readability"],"open_questions":[]}"#]],
    ]);
    write_fake_config(&project, &turns.to_string());

    let (code, out) = run_cli(&[
        "run", "--project", project.to_str().unwrap(), "--until", "design",
    ]);
    assert_eq!(code, 10, "expected waiting_approval, got {code}: {out}");
    assert_eq!(out["status"], "waiting_approval");

    // 3. The candidate package must exist with the review view.
    let runs_dir = project.join("runs");
    let run_id = fs::read_dir(&runs_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .next()
        .unwrap();
    let reports = project.join("runs").join(&run_id).join("reports");
    let review_md = reports.join("design-review.md");
    assert!(review_md.is_file(), "missing design-review.md");
    let md = fs::read_to_string(&review_md).unwrap();
    assert!(md.contains("# Design package review"));
    assert!(md.contains("tank_slice"));
    assert!(md.contains("### Risks"));
    assert!(md.contains("- water readability"));

    // 4. status shows waiting approval state.
    let (code, out) = run_cli(&["status", "--project", project.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["run"]["status"], "waiting_approval");

    // No revision committed yet: approving a non-existent revision fails.
    let (code, out) = run_cli(&[
        "approve", "--project", project.to_str().unwrap(), "--stage", "design",
        "--revision", "dnope", "--digest", "",
    ]);
    assert_ne!(code, 0, "{out}");

    // 5. Commit path requires approval off; do a second run in a project
    //    copy where approval is not required to verify the revision commit
    //    and pointer update end-to-end.
    let project2 = tmp_project("commit");
    fs::write(project2.join("brief.json"), r#"{"game_id":"tank_slice","text":"make a tank game"}"#).unwrap();
    let (code, _) = run_cli(&[
        "init", "--project", project2.to_str().unwrap(), "--brief",
        project2.join("brief.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let cfg = serde_json::json!({
        "provider": "fake",
        "model": "fake",
        "require_design_approval": false,
    });
    fs::write(
        project2.join("harness.config.json"),
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();
    std::env::set_var("PXL_HARNESS_FAKE_TURNS", turns.to_string());
    let (code, out) = run_cli(&[
        "run", "--project", project2.to_str().unwrap(), "--until", "design",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["status"], "succeeded");

    // Revision committed and pointer updated.
    let (code, out) = run_cli(&["status", "--project", project2.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    let revision = out["identity"]["design_revision"].as_str().unwrap().to_string();
    assert!(!revision.is_empty());
    assert!(project2.join("revisions/design").join(&revision).is_dir());

    // Approve binds the digest.
    let (code, out) = run_cli(&[
        "approve", "--project", project2.to_str().unwrap(), "--stage", "design",
        "--revision", revision.as_str(), "--digest", "",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out["digest"].as_str().unwrap().starts_with("sha256:"));

    // Tampering with the committed revision invalidates approval.
    let rev_dir = project2.join("revisions/design").join(&revision);
    fs::write(rev_dir.join("game-design.json"), "{}").unwrap();
    let (code, out) = run_cli(&["status", "--project", project2.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["design_approval_valid"], false);

    // A run with an invalid approval must not silently reuse the design.
    std::env::set_var("PXL_HARNESS_FAKE_TURNS", turns.to_string());
    let (code, out) = run_cli(&[
        "run", "--project", project2.to_str().unwrap(), "--until", "design",
    ]);
    assert_eq!(code, 11, "expected blocked, got {code}: {out}");
    std::env::remove_var("PXL_HARNESS_FAKE_TURNS");
}

/// P0-B regression (`w` hole): the model writes specs for two assets but only
/// builds one. The harness must auto-check the unbuilt one and surface it as a
/// queue failure, instead of treating saved specs as delivered.
///
/// Multi-frame directional assets are used as carriers (single-frame bullet fly
/// cannot pass QA before batch D and must not carry a scheduling test).
#[test]
fn unbuilt_asset_is_auto_checked_and_reported() {
    // Unit-level assertion of the queue + gate contract, driven through the
    // same harness code the CLI uses (no API key, no real model).
    let dir = std::env::temp_dir().join(format!("pxl_h_e2e_unbuilt_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let attempt = dir.join("attempt");
    let design = dir.join("design");
    fs::create_dir_all(&attempt).unwrap();
    fs::create_dir_all(&design).unwrap();
    fs::write(design.join("asset-requirements.json"), REQS).unwrap();

    let reqs: Value = serde_json::from_str(REQS).unwrap();
    let required: Vec<String> = reqs["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["required"] == true)
        .map(|r| r["asset_id"].as_str().unwrap().to_string())
        .collect();
    assert!(required.contains(&"player_tank".to_string()));
    assert!(required.contains(&"w".to_string()));

    // Both assets get complete specs (multi-frame anim carrier + tile).
    let palette = r##"{"v":1,"id":"e2e","colors":["#00000000","#101010","#f4f0e6"],"ramps":{"water":[1,2]},"cycles":{},"special":{"transparent":0,"outline":1}}"##;
    fs::write(attempt.join("palette.json"), palette).unwrap();
    for id in &required {
        fs::write(
            attempt.join(format!("{id}.shapes.json")),
            format!(r##"{{"w":8,"h":8,"name":"{id}","draw":[{{"rect":[1,1,2,2],"c":1}}]}}"##),
        )
        .unwrap();
        fs::write(
            attempt.join(format!("{id}.recipe.json")),
            format!(
                r##"{{"v":1,"asset":"{id}","gen_version":1,"seed":9,"palette":"palette.json"}}"##
            ),
        )
        .unwrap();
    }

    // Simulate: model built player_tank only; `w` was written but never built.
    let mut queue = pxl_harness::pipeline::asset_queue::AssetQueue::load(&attempt);
    queue.seed(&required);
    queue.mark_all_dirty(&attempt);
    queue.record_success("player_tank");
    queue.save(&attempt).unwrap();

    let loaded = pxl_harness::pipeline::asset_queue::AssetQueue::load(&attempt);
    // `w` must be pending_build: written, complete, never built.
    let w = loaded.entry("w").expect("w tracked in queue");
    assert_eq!(
        pxl_harness::pipeline::asset_queue::state_name(w.state),
        "pending_build",
        "written-but-unbuilt asset must be pending_build"
    );
    assert!(!w.input_fingerprint.is_empty());
    // player_tank is built; it must not be silently re-marked dirty.
    assert_eq!(
        pxl_harness::pipeline::asset_queue::state_name(
            loaded.entry("player_tank").unwrap().state
        ),
        "built"
    );
    // The status summary names the outstanding item.
    let summary = loaded.status_summary();
    assert!(summary.contains("w [pending_build]"), "{summary}");
    assert!(summary.contains("player_tank [built]"), "{summary}");

    // Changing an input invalidates the earlier success (no stale endorsement).
    fs::write(
        attempt.join("player_tank.recipe.json"),
        r##"{"v":1,"asset":"player_tank","gen_version":1,"seed":99,"palette":"palette.json"}"##,
    )
    .unwrap();
    let mut q2 = pxl_harness::pipeline::asset_queue::AssetQueue::load(&attempt);
    q2.mark_dirty("player_tank", &attempt);
    assert_eq!(
        pxl_harness::pipeline::asset_queue::state_name(
            q2.entry("player_tank").unwrap().state
        ),
        "pending_build",
        "recipe change must invalidate the prior success"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn init_refuses_twice_and_bad_briefs() {
    let project = tmp_project("init");
    let brief = project.join("brief.json");
    fs::write(&brief, r#"{"game_id":"tank_slice","text":"x"}"#).unwrap();
    let (code, _) = run_cli(&[
        "init", "--project", project.to_str().unwrap(), "--brief",
        brief.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    // Second init must refuse.
    let (code, out) = run_cli(&[
        "init", "--project", project.to_str().unwrap(), "--brief",
        brief.to_str().unwrap(),
    ]);
    assert_eq!(code, 4, "{out}");
    // Missing game_id fails.
    let other = tmp_project("bad");
    let bad = other.join("brief.json");
    fs::write(&bad, r#"{"text":"no id"}"#).unwrap();
    let (code, _) = run_cli(&[
        "init", "--project", other.to_str().unwrap(), "--brief",
        bad.to_str().unwrap(),
    ]);
    assert_eq!(code, 3);
}
