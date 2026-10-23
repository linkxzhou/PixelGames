//! Build stage. Compiles the macroquad game for the host and for wasm32.
//! The macroquad skill has to load before either cargo invocation; a missing
//! skill is a failed gate, same as a compiler error.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::adapters::process::{self, ProcSpec};
use crate::error::Result;
use crate::pipeline::state::set_stage;
use crate::qa::{GateCheck, GateReport, GateStatus, GATE_REPORT_VERSION};
use crate::skills;
use crate::storage::{Event, EventLog, RunState, RunStore, StageStatus};

const PACKAGE: &str = "battle-city";

pub fn run_build_stage(
    project: &Path,
    run: &RunStore,
    state: &mut RunState,
    events: &mut EventLog,
) -> Result<StageStatus> {
    set_stage(state, "build", StageStatus::Running)?;
    run.save(state)?;
    let started = Instant::now();
    let workspace = find_workspace(project).unwrap_or_else(|| project.to_path_buf());
    let manifest = workspace.join("Cargo.toml");
    let mut checks = Vec::new();

    let skill = skills::require("macroquad");
    checks.push(match &skill {
        Ok(skill) => GateCheck {
            id: "skill.macroquad".into(),
            passed: skill.body.contains("FilterMode::Nearest") && skill.body.contains("--headless"),
            value: skill.path.display().to_string(),
            limit: "harness/skills/macroquad/SKILL.md loads and covers nearest filtering plus headless replay".into(),
            failures: vec![],
        },
        Err(err) => GateCheck {
            id: "skill.macroquad".into(),
            passed: false,
            value: "missing".into(),
            limit: "skill file loads".into(),
            failures: vec![err.message.clone()],
        },
    });

    let _ = process::run(&ProcSpec {
        program: "rustup".into(),
        args: vec!["target".into(), "add".into(), "wasm32-unknown-unknown".into()],
        cwd: Some(workspace.clone()),
        timeout_secs: 180,
        max_output_bytes: 256 * 1024,
    });

    checks.push(compile_check(
        "compile.native",
        &workspace,
        &manifest,
        &["build", "--manifest-path", manifest.to_str().unwrap_or("Cargo.toml"), "-p", PACKAGE],
        "native cargo build -p battle-city exits 0",
    )?);
    checks.push(compile_check(
        "compile.wasm32",
        &workspace,
        &manifest,
        &[
            "build",
            "--manifest-path",
            manifest.to_str().unwrap_or("Cargo.toml"),
            "-p",
            PACKAGE,
            "--target",
            "wasm32-unknown-unknown",
        ],
        "wasm32-unknown-unknown cargo build -p battle-city exits 0",
    )?);

    let passed = checks.iter().all(|c| c.passed);
    let report = GateReport {
        report_version: GATE_REPORT_VERSION,
        gate_version: 1,
        stage: "build".into(),
        gate_id: "build.gates".into(),
        status: if passed { GateStatus::Passed } else { GateStatus::Failed },
        input_digest: crate::digest::digest_bytes(manifest.to_string_lossy().as_bytes()),
        checks,
        metrics: [("elapsed_ms".into(), started.elapsed().as_millis() as u64)].into_iter().collect(),
        evidence: vec![format!("workspace {}", workspace.display())],
        tool_versions: [("harness".into(), crate::VERSION.into()), ("cargo".into(), "workspace".into())]
            .into_iter()
            .collect(),
        executed_ms: started.elapsed().as_millis() as u64,
    };
    let digest = report.digest()?;
    let reports = run.reports_dir()?;
    std::fs::write(reports.join("build.gates.json"), serde_json::to_string_pretty(&report)?)?;
    events.append(Event::GateFinished {
        stage: "build".into(),
        task_id: "game".into(),
        passed,
        failures: report.failed_checks().len() as u32,
        report_digest: digest,
    })?;
    set_stage(state, "build", StageStatus::Validating)?;
    let status = if passed { StageStatus::Succeeded } else { StageStatus::Failed };
    set_stage(state, "build", status)?;
    run.save(state)?;
    Ok(status)
}

fn compile_check(
    id: &str,
    workspace: &Path,
    manifest: &Path,
    args: &[&str],
    limit: &str,
) -> Result<GateCheck> {
    if !manifest.is_file() {
        return Ok(GateCheck {
            id: id.into(),
            passed: false,
            value: "no manifest".into(),
            limit: limit.into(),
            failures: vec![format!("missing {}", manifest.display())],
        });
    }
    let output = process::run(&ProcSpec {
        program: "cargo".into(),
        args: args.iter().map(|s| (*s).to_string()).collect(),
        cwd: Some(workspace.to_path_buf()),
        timeout_secs: 600,
        max_output_bytes: 400 * 1024,
    })?;
    let mut failures = Vec::new();
    if !output.ok() {
        let tail = tail_text(&output.stderr, &output.stdout);
        failures.push(format!(
            "exit {:?} timeout {} {}",
            output.exit_code, output.timed_out, tail
        ));
    }
    Ok(GateCheck {
        id: id.into(),
        passed: failures.is_empty(),
        value: format!("exit {:?}", output.exit_code),
        limit: limit.into(),
        failures,
    })
}

fn tail_text(stderr: &str, stdout: &str) -> String {
    let src = if stderr.trim().is_empty() { stdout } else { stderr };
    let lines: Vec<&str> = src.lines().rev().take(12).collect();
    let text = lines.into_iter().rev().collect::<Vec<_>>().join(" | ");
    if text.len() > 700 {
        text.chars().rev().take(700).collect::<String>().chars().rev().collect()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_is_absolute() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../games/battle-city");
        let root = find_workspace(&project).expect("workspace");
        assert!(root.is_absolute(), "{}", root.display());
        assert!(root.join("Cargo.toml").is_file());
    }
}

pub fn find_workspace(start: &Path) -> Option<PathBuf> {
    let mut cur = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        let manifest = cur.join("Cargo.toml");
        if manifest.is_file() {
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                if text.contains("[workspace]") {
                    return Some(std::fs::canonicalize(&cur).unwrap_or(cur));
                }
            }
        }
        if !cur.pop() {
            return None;
        }
    }
}
