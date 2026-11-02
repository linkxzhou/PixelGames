//! Full-pipeline acceptance (plan/v3.0/pxl-step-pipeline-plan.md §8 M6).
//!
//! Phase A — step-100 alone: init -> fake designer run -> approve.
//! Phase B — the slice loop: 200 -> 300 -> 400 -> 500 in `.slice/`, then
//!           `step-500 slice` must rule PROCEED.
//! Phase C — production: 200 -> 300 -> 400 -> 500 check, all green.
//!
//! Runs entirely on fake providers: no network, no model. Every step talks
//! to the next only through files, and no step ever reads a downstream
//! directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn root() -> PathBuf {
    // This crate lives two levels below the workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Sibling step binaries live next to this test's own `step-500`, so a custom
/// or relative `CARGO_TARGET_DIR` resolves exactly as Cargo resolved it.
/// `PXL_STEP_BIN_DIR` overrides the directory; a missing binary is an error.
fn bin(name: &str) -> PathBuf {
    let dir = match std::env::var("PXL_STEP_BIN_DIR") {
        Ok(raw) if !raw.trim().is_empty() => {
            let dir = PathBuf::from(raw);
            if dir.is_absolute() {
                dir
            } else {
                std::env::current_dir().unwrap().join(dir)
            }
        }
        _ => Path::new(env!("CARGO_BIN_EXE_step-500"))
            .parent()
            .expect("step-500 binary has a parent directory")
            .to_path_buf(),
    };
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing; run `cargo build --locked --workspace` with the same CARGO_TARGET_DIR, or set PXL_STEP_BIN_DIR",
        path.display()
    );
    path
}

fn run(bin_name: &str, args: &[&str], env: &[(&str, &Path)]) -> (i32, String, String) {
    let mut cmd = Command::new(bin(bin_name));
    cmd.args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("spawn step binary");
    (
        out.status.code().unwrap_or(1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-pipeline-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// A one-tile floor level: it references no asset, so both the slice and the
/// production runs can hold it without pretending a tile was delivered.
fn minimal_levels(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("level_0.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "id": "level_0",
            "name": "e2e",
            "tile_size_px": 16,
            "grid_w": 4,
            "grid_h": 4,
            "grid": ["ffff", "ffff", "ffff", "ffff"],
            "tile_semantics": [
                { "tile_id": "f", "behaviour": "floor", "blocks": false }
            ],
            "placements": [],
            "spawn": [0, 0],
            "exit": [3, 3],
            "objective": "walk",
            "difficulty": 1,
            "seed": 1,
            "notes": ""
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn step100_then_slice_then_production() {
    let work = temp();
    let fx100 = root().join("steps/step-100-design/tests/fixtures");
    let fx200 = root().join("steps/step-200-art-spec/tests/fixtures");
    let levels = work.join("levels");
    minimal_levels(&levels);

    // ---- Phase A: step-100 alone ------------------------------------------
    let w100 = work.join("100-design");
    let (code, _o, e) = run(
        "step-100",
        &[
            "init",
            "--brief",
            fx100.join("brief.json").to_str().unwrap(),
            "--out",
            w100.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "100 init: {e}");

    let (code, _o, e) = run(
        "step-100",
        &["run", "--in", w100.to_str().unwrap()],
        &[("STEP_100_FAKE_TURNS", &fx100.join("fake_turns.json"))],
    );
    assert_eq!(code, 10, "100 run must wait for approval: {e}");
    let digest = read_json(&w100.join("revision.manifest.json"))["manifest_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, _o, e) = run(
        "step-100",
        &[
            "approve",
            "--in",
            w100.to_str().unwrap(),
            "--digest",
            &digest,
        ],
        &[],
    );
    assert_eq!(code, 0, "100 approve: {e}");
    assert!(w100.join("asset-requirements.json").is_file());

    // ---- Phase B: the slice loop ------------------------------------------
    let design = fx200.join("design");
    let w200_slice = work.join("200-art-spec/.slice");
    let (code, _o, e) = run(
        "step-200",
        &[
            "run",
            "--in",
            design.to_str().unwrap(),
            "--out",
            w200_slice.to_str().unwrap(),
            "--scope",
            "slice",
        ],
        &[("STEP_200_FAKE_TURNS", &fx200.join("fake_turns_full.json"))],
    );
    assert_eq!(code, 0, "200 slice run: {e}");
    let manifest = read_json(&w200_slice.join("spec-manifest.json"));
    assert_eq!(manifest["scope"], "slice");
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 1);

    let w300_slice = work.join("300-build/.slice");
    let (code, _o, e) = run(
        "step-300",
        &[
            "plan",
            "--in",
            w200_slice.to_str().unwrap(),
            "--out",
            w300_slice.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "300 slice plan: {e}");

    let w400_slice = work.join("400-package/.slice");
    let (code, _o, e) = run(
        "step-400",
        &[
            "run",
            "--in",
            w300_slice.to_str().unwrap(),
            "--requirements",
            design.join("asset-requirements.json").to_str().unwrap(),
            "--out",
            w400_slice.to_str().unwrap(),
            "--game-id",
            "e2e",
            "--levels",
            levels.to_str().unwrap(),
            "--scope",
            "slice",
            "--pipeline",
            design.join("pipeline.json").to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "400 slice package: {e}");

    let w500_slice = work.join("500-engine/.slice");
    let (code, _o, e) = run(
        "step-500",
        &[
            "check",
            "--in",
            w400_slice.to_str().unwrap(),
            "--out",
            w500_slice.to_str().unwrap(),
            "--scope",
            "slice",
        ],
        &[],
    );
    assert_eq!(code, 0, "500 slice check: {e}");
    fs::write(w500_slice.join("engine-build.json"), b"{}").unwrap();
    fs::create_dir_all(w500_slice.join("playtest")).unwrap();
    fs::write(
        w500_slice.join("playtest/report.json"),
        serde_json::to_string(&json!({
            "status": "PASS",
            "build_digest": format!("sha256:{}", pxl_store::digest::sha256_hex(b"{}")),
        }))
        .unwrap(),
    )
    .unwrap();

    let gate = work.join("650-slice-gate");
    let (code, o, e) = run(
        "step-500",
        &[
            "slice",
            "--work",
            work.to_str().unwrap(),
            "--out",
            gate.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "slice gate must proceed: stdout={o} stderr={e}");
    let report = read_json(&gate.join("slice-report.json"));
    assert_eq!(report["verdict"], "PROCEED");
    assert_eq!(report["next"], "step-200 run --scope all");

    // ---- Phase C: production ----------------------------------------------
    let w200 = work.join("200-art-spec");
    let (code, _o, e) = run(
        "step-200",
        &[
            "run",
            "--in",
            design.to_str().unwrap(),
            "--out",
            w200.to_str().unwrap(),
        ],
        &[("STEP_200_FAKE_TURNS", &fx200.join("fake_turns_full.json"))],
    );
    assert_eq!(code, 0, "200 production run: {e}");
    assert_eq!(
        read_json(&w200.join("spec-manifest.json"))["assets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let w300 = work.join("300-build");
    let (code, _o, e) = run(
        "step-300",
        &[
            "plan",
            "--in",
            w200.to_str().unwrap(),
            "--out",
            w300.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "300 production plan: {e}");

    let w400 = work.join("400-package");
    let (code, _o, e) = run(
        "step-400",
        &[
            "run",
            "--in",
            w300.to_str().unwrap(),
            "--requirements",
            design.join("asset-requirements.json").to_str().unwrap(),
            "--out",
            w400.to_str().unwrap(),
            "--game-id",
            "e2e",
            "--levels",
            levels.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(code, 0, "400 production package: {e}");
    let coverage = read_json(&w400.join("coverage.json"));
    assert_eq!(coverage["delivered"], coverage["required"]);
    assert_eq!(coverage["required"], 2);

    let (code, o, e) = run("step-500", &["check", "--in", w400.to_str().unwrap()], &[]);
    assert_eq!(code, 0, "500 production check: stdout={o} stderr={e}");
    let report = read_json(&w400.join("engine-check.json"));
    assert_eq!(report["ok"], true);
    assert_eq!(report["checks"].as_array().unwrap().len(), 7);
}
