//! Playtest stage. Runs the macroquad game's headless replay and checks the
//! report plus the PNG bytes: N frames without a panic, palette match, and
//! the scripted move / shoot / destroy assertions.

use std::path::Path;
use std::time::Instant;

use serde::Deserialize;

use crate::adapters::process::{self, ProcSpec};
use crate::error::{HarnessError, Result};
use crate::pipeline::build_stage::find_workspace;
use crate::pipeline::state::set_stage;
use crate::qa::{GateCheck, GateReport, GateStatus, GATE_REPORT_VERSION};
use crate::storage::{Event, EventLog, RunState, RunStore, StageStatus};

#[derive(Debug, Deserialize)]
pub(crate) struct PlayReport {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    panicked: bool,
    #[serde(default)]
    frames: u32,
    #[serde(default)]
    player_moved: bool,
    #[serde(default)]
    shots: u32,
    #[serde(default)]
    bricks_destroyed: u32,
    #[serde(default)]
    enemies_destroyed: u32,
    #[serde(default)]
    palette_mismatches: u32,
    #[serde(default)]
    screenshots: Vec<String>,
}

pub fn run_playtest_stage(
    project: &Path,
    run: &RunStore,
    state: &mut RunState,
    events: &mut EventLog,
) -> Result<StageStatus> {
    set_stage(state, "playtest", StageStatus::Running)?;
    run.save(state)?;
    let started = Instant::now();
    let workspace = find_workspace(project).unwrap_or_else(|| project.to_path_buf());
    let manifest = workspace.join("Cargo.toml");
    let replay = project.join("runtime/replay/playtest.json");
    let reports = run.reports_dir()?;
    let out_dir = reports.join("playtest");
    let report_path = reports.join("playtest.json");
    std::fs::create_dir_all(&out_dir)?;

    let mut checks = Vec::new();
    if !replay.is_file() {
        checks.push(fail("playtest.frames", "missing replay", "runtime/replay/playtest.json"));
    } else {
        let output = process::run(&ProcSpec {
            program: "cargo".into(),
            args: vec![
                "run".into(),
                "--manifest-path".into(),
                manifest.display().to_string(),
                "-p".into(),
                "battle-city".into(),
                "--".into(),
                "--headless".into(),
                "--replay".into(),
                replay.display().to_string(),
                "--out".into(),
                out_dir.display().to_string(),
                "--report".into(),
                report_path.display().to_string(),
                "--scale".into(),
                "3".into(),
            ],
            cwd: Some(workspace),
            timeout_secs: 300,
            max_output_bytes: 400 * 1024,
        })?;
        if !output.ok() {
            checks.push(GateCheck {
                id: "playtest.frames".into(),
                passed: false,
                value: format!("exit {:?}", output.exit_code),
                limit: "headless battle-city exits 0 and writes a report".into(),
                failures: vec![clip(&format!("{}\n{}", output.stderr, output.stdout))],
            });
        } else if let Some(report) = read_report(&report_path) {
            let palette = load_palette(&project.join("runtime/data/palette.json"))?;
            checks.extend(evaluate(&report, &out_dir, &palette));
        } else {
            checks.push(fail("playtest.frames", "report missing", "playtest.json parses"));
        }
    }

    let passed = checks.iter().all(|c| c.passed);
    let report = GateReport {
        report_version: GATE_REPORT_VERSION,
        gate_version: 1,
        stage: "playtest".into(),
        gate_id: "playtest.gates".into(),
        status: if passed { GateStatus::Passed } else { GateStatus::Failed },
        input_digest: crate::digest::digest_bytes(replay.to_string_lossy().as_bytes()),
        checks,
        metrics: [("elapsed_ms".into(), started.elapsed().as_millis() as u64)].into_iter().collect(),
        evidence: vec![out_dir.display().to_string()],
        tool_versions: [("harness".into(), crate::VERSION.into())].into_iter().collect(),
        executed_ms: started.elapsed().as_millis() as u64,
    };
    let digest = report.digest()?;
    std::fs::write(reports.join("playtest.gates.json"), serde_json::to_string_pretty(&report)?)?;
    events.append(Event::GateFinished {
        stage: "playtest".into(),
        task_id: "replay".into(),
        passed,
        failures: report.failed_checks().len() as u32,
        report_digest: digest,
    })?;
    set_stage(state, "playtest", StageStatus::Validating)?;
    let status = if passed { StageStatus::Succeeded } else { StageStatus::Failed };
    set_stage(state, "playtest", status)?;
    run.save(state)?;
    Ok(status)
}

pub(crate) fn evaluate(report: &PlayReport, out_dir: &Path, palette: &[[u8; 4]]) -> Vec<GateCheck> {
    let mut checks = Vec::new();
    checks.push(GateCheck {
        id: "playtest.frames".into(),
        passed: report.ok && !report.panicked && report.frames >= 60,
        value: format!("frames={} panicked={}", report.frames, report.panicked),
        limit: "at least 60 fixed steps, process did not panic, report ok".into(),
        failures: vec![],
    });
    let mut png_bad = Vec::new();
    if report.screenshots.is_empty() {
        png_bad.push("no screenshot listed".into());
    }
    for name in &report.screenshots {
        let path = out_dir.join(name);
        if !path.is_file() {
            png_bad.push(format!("{name} missing"));
            continue;
        }
        match png_mismatches(&path, palette) {
            Ok(0) => {}
            Ok(n) => png_bad.push(format!("{name} has {n} off-palette pixels")),
            Err(err) => png_bad.push(format!("{name}: {err}")),
        }
    }
    let reported = report.palette_mismatches;
    checks.push(GateCheck {
        id: "playtest.palette".into(),
        passed: png_bad.is_empty() && reported == 0,
        value: format!("report={reported} files={}", report.screenshots.len()),
        limit: "every screenshot pixel is transparent or an opaque palette color".into(),
        failures: png_bad,
    });
    checks.push(bool_check("playtest.player_moves", report.player_moved, "player_moved", "player tile changes during the replay"));
    checks.push(bool_check("playtest.shoots", report.shots >= 1, &format!("shots={}", report.shots), "at least one shot"));
    let destroyed = report.bricks_destroyed >= 1 || report.enemies_destroyed >= 1;
    checks.push(bool_check(
        "playtest.destroys",
        destroyed,
        &format!("bricks={} enemies={}", report.bricks_destroyed, report.enemies_destroyed),
        "a brick or an enemy is destroyed",
    ));
    checks.push(GateCheck {
        id: "playtest.screenshot".into(),
        passed: report.screenshots.iter().any(|n| out_dir.join(n).is_file()),
        value: report.screenshots.first().cloned().unwrap_or_else(|| "none".into()),
        limit: "at least one PNG written by the headless replay".into(),
        failures: vec![],
    });
    checks
}

fn bool_check(id: &str, passed: bool, value: &str, limit: &str) -> GateCheck {
    GateCheck {
        id: id.into(),
        passed,
        value: value.into(),
        limit: limit.into(),
        failures: vec![],
    }
}

fn fail(id: &str, value: &str, limit: &str) -> GateCheck {
    GateCheck {
        id: id.into(),
        passed: false,
        value: value.into(),
        limit: limit.into(),
        failures: vec![value.into()],
    }
}

fn read_report(path: &Path) -> Option<PlayReport> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_palette(path: &Path) -> Result<Vec<[u8; 4]>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        HarnessError::not_found(format!("palette {}: {e}", path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let colors = value
        .get("colors")
        .and_then(|c| c.as_array())
        .ok_or_else(|| HarnessError::schema("palette.colors missing"))?;
    let mut out = Vec::new();
    for color in colors {
        let hex = color.as_str().ok_or_else(|| HarnessError::schema("palette color"))?;
        out.push(parse_hex(hex).ok_or_else(|| HarnessError::schema(format!("bad color {hex}")))?);
    }
    Ok(out)
}

fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    let h = hex.strip_prefix('#')?;
    match h.len() {
        8 => {
            let n = u32::from_str_radix(h, 16).ok()?;
            Some([
                ((n >> 24) & 0xff) as u8,
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
            ])
        }
        6 => {
            let n = u32::from_str_radix(h, 16).ok()?;
            Some([((n >> 16) & 0xff) as u8, ((n >> 8) & 0xff) as u8, (n & 0xff) as u8, 255])
        }
        _ => None,
    }
}

fn png_mismatches(path: &Path, palette: &[[u8; 4]]) -> std::result::Result<usize, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let bytes = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for px in bytes.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        other => return Err(format!("color type {other:?}")),
    };
    let bad = rgba.chunks_exact(4).filter(|px| !palette_contains(palette, *px)).count();
    Ok(bad)
}

fn palette_contains(palette: &[[u8; 4]], px: &[u8]) -> bool {
    if px.get(3) == Some(&0) {
        return true;
    }
    palette.iter().any(|c| c[0] == px[0] && c[1] == px[1] && c[2] == px[2] && c[3] == 255 && px[3] == 255)
}

fn clip(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= 500 {
        trimmed.to_string()
    } else {
        trimmed[trimmed.len() - 500..].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(path: &Path, rgba: [u8; 4]) {
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }

    #[test]
    fn playtest_gates_accept_a_palette_clean_report() {
        let dir = std::env::temp_dir().join(format!("pxl_pt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        solid(&dir.join("frame_0000.png"), [0x0d, 0x0f, 0x0a, 255]);
        let report = PlayReport {
            ok: true,
            panicked: false,
            frames: 120,
            player_moved: true,
            shots: 2,
            bricks_destroyed: 1,
            enemies_destroyed: 0,
            palette_mismatches: 0,
            screenshots: vec!["frame_0000.png".into()],
        };
        let palette = vec![[0x0d, 0x0f, 0x0a, 255]];
        let checks = evaluate(&report, &dir, &palette);
        assert!(checks.iter().all(|c| c.passed), "{checks:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn playtest_gates_reject_an_off_palette_pixel() {
        let dir = std::env::temp_dir().join(format!("pxl_pt_bad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        solid(&dir.join("frame_0000.png"), [1, 2, 3, 255]);
        let report = PlayReport {
            ok: true,
            panicked: false,
            frames: 120,
            player_moved: true,
            shots: 1,
            bricks_destroyed: 1,
            enemies_destroyed: 0,
            palette_mismatches: 0,
            screenshots: vec!["frame_0000.png".into()],
        };
        let checks = evaluate(&report, &dir, &[[9, 9, 9, 255]]);
        let palette = checks.iter().find(|c| c.id == "playtest.palette").unwrap();
        assert!(!palette.passed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
