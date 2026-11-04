use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use pxl_contract::error::{HarnessError, Result};
use pxl_contract::{
    DigestEntry, GateCheck, GateResult, Gates, Scope, StepEnvelope, StepStatus, Verdict,
};
use pxl_store::digest::digest_file;

use crate::check::write_json;
use crate::{PlaytestArgs, STEP_ID, VERSION};

pub fn playtest(args: PlaytestArgs) -> Result<i32> {
    if !args.in_dir.join("engine-build.json").is_file() {
        return Err(
            HarnessError::not_found("engine-build.json is missing".to_string())
                .with_hint("run step-500 build first"),
        );
    }
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.in_dir.join("playtest"));
    fs::create_dir_all(out.join("frames")).map_err(io("cannot create playtest output"))?;
    let build_digest = digest_file(&args.in_dir.join("engine-build.json"))?;
    let mode = args.mode.as_str();
    if !["headless", "browser", "visual", "all"].contains(&mode) {
        return Err(HarnessError::usage(format!(
            "unknown playtest mode '{mode}'"
        )));
    }

    let mut headless = None;
    let mut browser = None;
    if mode == "headless" || mode == "all" {
        headless = Some(run_headless(&args.in_dir, &out, &build_digest)?);
    }
    if mode == "browser" || mode == "visual" || mode == "all" {
        browser = Some(run_browser(&args.in_dir, &out, &build_digest)?);
    }
    let status = combine(headless.as_ref(), browser.as_ref());
    let report = if let Some(mut report) = headless.clone() {
        if let Some(browser) = &browser {
            report["browser"] = browser.clone();
            report["status"] = Value::String(status.clone());
            report["ok"] = Value::Bool(status == "PASS");
        }
        report
    } else {
        let mut report = browser
            .clone()
            .unwrap_or_else(|| json!({"status": "NOT_ASSESSED"}));
        report["build_digest"] = Value::String(build_digest);
        report
    };
    write_json(&out.join("report.json"), &report)?;
    write_envelope(&args.in_dir, &out, &status)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": status == "PASS",
            "cmd": "step-500 playtest",
            "status": status,
            "report": "report.json",
        }))?
    );
    Ok(match status.as_str() {
        "PASS" => 0,
        "NOT_ASSESSED" => 11,
        _ => 5,
    })
}

fn run_headless(build: &Path, out: &Path, build_digest: &str) -> Result<Value> {
    let web = build.join("web");
    let runner = web.join("headless.js");
    let replay = web.join("replay/playtest.json");
    if !runner.is_file() || !replay.is_file() {
        return Ok(json!({
            "status": "NOT_ASSESSED", "ok": false, "build_digest": build_digest,
            "reason": "the game has no headless.js or replay/playtest.json",
        }));
    }
    let mut command = Command::new("node");
    command
        .arg(&runner)
        .arg("--package")
        .arg(web.join("assets"))
        .arg("--replay")
        .arg(&replay)
        .arg("--out")
        .arg(out)
        .arg("--report")
        .arg(out.join("report.json"));
    let first = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(json!({
                "status": "NOT_ASSESSED", "ok": false, "build_digest": build_digest,
                "reason": "Node.js is required for headless playtest",
            }));
        }
        Err(error) => return Err(HarnessError::io(format!("cannot run Node.js: {error}"))),
    };
    let report_path = out.join("report.json");
    if !report_path.is_file() {
        return Ok(json!({
            "status": "FAIL", "ok": false, "build_digest": build_digest,
            "reason": format!("headless runner exited {} without report.json", first.status),
        }));
    }
    let mut report: Value = serde_json::from_str(
        &fs::read_to_string(&report_path).map_err(io("cannot read playtest report"))?,
    )
    .map_err(|e| HarnessError::schema(format!("playtest report: {e}")))?;
    let first_hash = report["state_hash"].as_str().unwrap_or("").to_string();
    let first_audio_hash = report["audio_events_hash"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let second = Command::new("node")
        .arg(&runner)
        .arg("--package")
        .arg(web.join("assets"))
        .arg("--replay")
        .arg(&replay)
        .arg("--out")
        .arg(out)
        .arg("--report")
        .arg(out.join("second-report.json"))
        .output()
        .map_err(|e| HarnessError::io(format!("cannot repeat headless replay: {e}")))?;
    let second_report: Value = serde_json::from_str(
        &fs::read_to_string(out.join("second-report.json"))
            .map_err(io("cannot read second playtest report"))?,
    )
    .map_err(|e| HarnessError::schema(format!("second playtest report: {e}")))?;
    let audio_deterministic =
        first_audio_hash == second_report["audio_events_hash"].as_str().unwrap_or("");
    let deterministic = first.status.success()
        && second.status.success()
        && !first_hash.is_empty()
        && first_hash == second_report["state_hash"].as_str().unwrap_or("")
        && audio_deterministic;
    let passed = report["ok"] == true && deterministic;
    report["audio_deterministic"] = Value::Bool(audio_deterministic);
    report["deterministic"] = Value::Bool(deterministic);
    report["build_digest"] = Value::String(build_digest.to_string());
    report["status"] = Value::String(if passed { "PASS" } else { "FAIL" }.to_string());
    report["ok"] = Value::Bool(passed);
    fs::remove_file(out.join("second-report.json")).ok();
    Ok(report)
}

fn run_browser(build: &Path, out: &Path, build_digest: &str) -> Result<Value> {
    let script = build.join("web/browser-playtest.py");
    if !script.is_file() {
        return Ok(json!({
            "status": "NOT_ASSESSED", "ok": false, "build_digest": build_digest,
            "reason": "browser-playtest.py is missing",
        }));
    }
    let output = match Command::new("python3")
        .arg(&script)
        .arg("--dir")
        .arg(build.join("web"))
        .arg("--out")
        .arg(out)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(json!({
                "status": "NOT_ASSESSED", "ok": false, "build_digest": build_digest,
                "reason": "python3 with Playwright is unavailable",
            }));
        }
        Err(error) => {
            return Err(HarnessError::io(format!(
                "cannot run browser playtest: {error}"
            )))
        }
    };
    let path = out.join("browser-report.json");
    if !path.is_file() {
        return Ok(json!({
            "status": "FAIL", "ok": false, "build_digest": build_digest,
            "reason": format!("browser runner exited {} without browser-report.json", output.status),
        }));
    }
    let mut report: Value =
        serde_json::from_str(&fs::read_to_string(path).map_err(io("cannot read browser report"))?)
            .map_err(|e| HarnessError::schema(format!("browser report: {e}")))?;
    report["build_digest"] = Value::String(build_digest.to_string());
    Ok(report)
}

fn combine(headless: Option<&Value>, browser: Option<&Value>) -> String {
    let statuses = [headless, browser]
        .into_iter()
        .flatten()
        .filter_map(|value| value["status"].as_str())
        .collect::<Vec<_>>();
    if statuses.iter().any(|status| *status == "FAIL") {
        "FAIL".to_string()
    } else if statuses.is_empty() || statuses.iter().any(|status| *status == "NOT_ASSESSED") {
        "NOT_ASSESSED".to_string()
    } else {
        "PASS".to_string()
    }
}

fn write_envelope(build: &Path, out: &Path, status: &str) -> Result<()> {
    let verdict = match status {
        "PASS" => Verdict::Pass,
        "NOT_ASSESSED" => Verdict::NotAssessed,
        _ => Verdict::Fail,
    };
    let envelope = StepEnvelope {
        schema_version: 1,
        step: STEP_ID.to_string(),
        step_version: VERSION.to_string(),
        contract: crate::CONTRACT_ID.to_string(),
        status: match verdict {
            Verdict::Pass => StepStatus::Ok,
            Verdict::NotAssessed => StepStatus::Blocked,
            _ => StepStatus::GateFailed,
        },
        scope: Scope::All,
        inputs: vec![DigestEntry {
            path: "engine-build.json".to_string(),
            digest: digest_file(&build.join("engine-build.json"))?,
        }],
        outputs: vec![DigestEntry {
            path: "playtest/report.json".to_string(),
            digest: digest_file(&out.join("report.json"))?,
        }],
        verdict,
        gates: Gates {
            required_artifacts: vec![GateCheck {
                id: "playtest.report".to_string(),
                result: GateResult::Pass,
                detail: None,
            }],
            quality_checks: vec![GateCheck {
                id: "playtest.status".to_string(),
                result: match status {
                    "PASS" => GateResult::Pass,
                    "NOT_ASSESSED" => GateResult::NotAssessed,
                    _ => GateResult::Fail,
                },
                detail: None,
            }],
            not_assessed: if status == "NOT_ASSESSED" {
                vec!["playtest.status".to_string()]
            } else {
                Vec::new()
            },
        },
        error: None,
    };
    write_json(&out.join("step.json"), &serde_json::to_value(envelope)?)?;
    Ok(())
}

fn io(message: &'static str) -> impl FnOnce(std::io::Error) -> HarnessError {
    move |error| HarnessError::io(format!("{message}: {error}"))
}

#[allow(dead_code)]
fn _path(_: PathBuf) {}
