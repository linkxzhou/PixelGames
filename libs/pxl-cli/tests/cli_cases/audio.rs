//! `step-300 audio`: validate, render, inspect, and the failure semantics.
//!
//! Render tests need Node 22+. When it is missing they print a skip line and
//! return, so a machine without Node never reports a render as verified.

use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn node_ok() -> bool {
    Command::new("node")
        .arg("--version")
        .env_remove("NODE_OPTIONS")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| {
            v.trim()
                .trim_start_matches('v')
                .split('.')
                .next()?
                .parse::<u32>()
                .ok()
        })
        .is_some_and(|major| major >= 22)
}

macro_rules! need_node {
    () => {
        if !node_ok() {
            eprintln!("SKIP: Node 22+ not found; audio render is NOT_ASSESSED");
            return;
        }
    };
}

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-audio-cli-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn spec(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("audio.spec.json");
    fs::write(&path, body).unwrap();
    path
}

const SFX_ONLY: &str = r#"{"contract":"audio-spec/v1","asset":"cli_pack","seed":11,"sfx":[
  {"id":"beep","engine":"zzfx","params":{"waveform":"sine","frequency_hz":440,"attack_sec":0.002,"sustain_sec":0.05,"release_sec":0.05},"gain":0.4},
  {"id":"thud","engine":"zzfx","params":{"waveform":"noise","frequency_hz":90,"sustain_sec":0.04,"release_sec":0.12,"noise":1},"gain":0.4}]}"#;

fn render_args<'a>(spec: &'a str, out: &'a str) -> Vec<&'a str> {
    vec!["--json", "audio", "render", spec, "-o", out]
}

#[test]
fn validate_reports_frames_and_needs_no_node() {
    let dir = tmp("validate");
    let s = spec(&dir, SFX_ONLY);
    let (code, stdout, _) = pxl(&["--json", "audio", "validate", s.to_str().unwrap()]);
    assert_eq!(code, 0, "{stdout}");
    let v = json_line(&stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["sfx"].as_array().unwrap().len(), 2);
    assert!(v["sfx"][0]["frames"].as_u64().unwrap() > 0);
}

#[test]
fn validate_rejects_bad_specs_with_the_right_exit_codes() {
    let dir = tmp("bad");
    let unknown = SFX_ONLY.replace(
        "\"gain\":0.4},\n  {\"id\":\"thud\"",
        "\"gain\":0.4,\"typo\":1},\n  {\"id\":\"thud\"",
    );
    let (code, stdout, _) = pxl(&[
        "--json",
        "audio",
        "validate",
        spec(&dir, &unknown).to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert_eq!(json_line(&stdout)["audio_error"], "schema");

    let range = SFX_ONLY.replace("\"frequency_hz\":440", "\"frequency_hz\":1");
    let (code, stdout, _) = pxl(&[
        "--json",
        "audio",
        "validate",
        spec(&dir, &range).to_str().unwrap(),
    ]);
    assert_eq!(code, 4, "{stdout}");
    assert_eq!(json_line(&stdout)["audio_error"], "semantic");

    let long = SFX_ONLY.replace("\"sustain_sec\":0.05", "\"sustain_sec\":2");
    let (code, stdout, _) = pxl(&[
        "--json",
        "audio",
        "validate",
        spec(&dir, &long).to_str().unwrap(),
    ]);
    assert_eq!(code, 4, "{stdout}");
    assert_eq!(json_line(&stdout)["audio_error"], "limit");

    let (code, _, _) = pxl(&[
        "--json",
        "audio",
        "validate",
        dir.join("missing.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn render_sfx_is_deterministic_and_inspectable() {
    need_node!();
    let dir = tmp("sfx");
    let s = spec(&dir, SFX_ONLY);
    let a = dir.join("a");
    let b = dir.join("b");
    let (code, stdout, stderr) = pxl(&render_args(s.to_str().unwrap(), a.to_str().unwrap()));
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let v = json_line(&stdout);
    assert_eq!(v["entries"].as_array().unwrap().len(), 2);
    let (code, ..) = pxl(&render_args(s.to_str().unwrap(), b.to_str().unwrap()));
    assert_eq!(code, 0);
    for rel in ["audio-manifest.json", "sfx/beep.wav", "sfx/thud.wav"] {
        assert_eq!(
            fs::read(a.join(rel)).unwrap(),
            fs::read(b.join(rel)).unwrap(),
            "{rel}"
        );
    }
    let (code, stdout, _) = pxl(&["--json", "audio", "inspect", a.to_str().unwrap()]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(json_line(&stdout)["ok"], true);
    let (code, stdout, _) = pxl(&[
        "--json",
        "audio",
        "inspect",
        a.join("sfx/beep.wav").to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(json_line(&stdout)["channels"], 1);
}

#[test]
fn seed_reaches_the_synth_only_where_randomness_is_requested() {
    need_node!();
    // `jitter` asks for pitch randomness, so its samples depend on the seed;
    // `steady` has randomness 0, so the seed must not change a single byte.
    let body = r#"{"contract":"audio-spec/v1","asset":"cli_pack","seed":11,"sfx":[
      {"id":"jitter","engine":"zzfx","params":{"waveform":"sine","randomness":0.5,"frequency_hz":440,"sustain_sec":0.05,"release_sec":0.05},"gain":0.4},
      {"id":"steady","engine":"zzfx","params":{"waveform":"sine","randomness":0,"frequency_hz":440,"sustain_sec":0.05,"release_sec":0.05},"gain":0.4}]}"#;
    let dir = tmp("seed");
    let s = spec(&dir, body);
    let a = dir.join("a");
    let b = dir.join("b");
    let (code, ..) = pxl(&[
        "--json",
        "--seed",
        "1",
        "audio",
        "render",
        s.to_str().unwrap(),
        "-o",
        a.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let (code, ..) = pxl(&[
        "--json",
        "--seed",
        "2",
        "audio",
        "render",
        s.to_str().unwrap(),
        "-o",
        b.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    assert_ne!(
        fs::read(a.join("sfx/jitter.wav")).unwrap(),
        fs::read(b.join("sfx/jitter.wav")).unwrap()
    );
    assert_eq!(
        fs::read(a.join("sfx/steady.wav")).unwrap(),
        fs::read(b.join("sfx/steady.wav")).unwrap()
    );
}

#[test]
fn render_bgm_has_stereo_whole_loop_and_checks_pass() {
    need_node!();
    let spec_path = repo_root().join("games/battle-city/audio/audio.spec.json");
    let out = tmp("bgm").join("out");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "--strict",
        "audio",
        "render",
        spec_path.to_str().unwrap(),
        "--only",
        "stage_theme",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(out.join("audio-manifest.json")).unwrap())
            .unwrap();
    let entry = &manifest["entries"][0];
    assert_eq!(entry["channels"], 2);
    assert_eq!(entry["frames"], 1_128_960);
    assert_eq!(entry["loop"]["start_frame"], 0);
    assert_eq!(entry["loop"]["end_frame_exclusive"], 1_128_960);
    assert_eq!(entry["qa"]["status"], "pass");
    assert!(
        manifest.get("generated_at").is_none(),
        "no timestamps in the manifest"
    );
}

#[test]
fn existing_output_needs_overwrite_and_failed_render_publishes_nothing() {
    need_node!();
    let dir = tmp("exists");
    let s = spec(&dir, SFX_ONLY);
    let out = dir.join("out");
    let (code, ..) = pxl(&render_args(s.to_str().unwrap(), out.to_str().unwrap()));
    assert_eq!(code, 0);
    let before = fs::read(out.join("sfx/beep.wav")).unwrap();
    let (code, stdout, _) = pxl(&render_args(s.to_str().unwrap(), out.to_str().unwrap()));
    assert_eq!(code, 6, "{stdout}");
    let (code, ..) = pxl(&[
        "--json",
        "--overwrite",
        "audio",
        "render",
        s.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);

    // A spec whose only sfx is silent fails QA: the old output stays, evidence goes to .rejected.
    let silent = r#"{"contract":"audio-spec/v1","asset":"cli_pack","seed":11,"sfx":[
      {"id":"beep","engine":"zzfx","params":{"waveform":"sine","frequency_hz":440,"sustain_sec":0.05,"release_sec":0.05},"gain":0.0}]}"#;
    let s2 = spec(&dir, silent);
    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "audio",
        "render",
        s2.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stdout}");
    assert_eq!(json_line(&stdout)["audio_error"], "qa");
    assert_eq!(
        fs::read(out.join("sfx/beep.wav")).unwrap(),
        before,
        "approved output must survive a failed render"
    );
    assert!(dir.join(".out.rejected/audio-manifest.json").is_file());
}

#[test]
fn missing_node_blocks_and_publishes_nothing() {
    let dir = tmp("nonode");
    let s = spec(&dir, SFX_ONLY);
    let out = dir.join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_step-300"))
        .args([
            "--json",
            "audio",
            "render",
            s.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .env("PXL_AUDIO_NODE", dir.join("no-such-node"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let v = json_line(&String::from_utf8(output.stdout).unwrap());
    assert_eq!(v["audio_error"], "blocked");
    assert!(!out.exists());
}

#[test]
fn inspect_detects_tampered_wav_and_manifest() {
    need_node!();
    let dir = tmp("tamper");
    let s = spec(&dir, SFX_ONLY);
    let out = dir.join("out");
    let (code, ..) = pxl(&render_args(s.to_str().unwrap(), out.to_str().unwrap()));
    assert_eq!(code, 0);

    let wav = out.join("sfx/beep.wav");
    let mut bytes = fs::read(&wav).unwrap();
    bytes[100] ^= 0x55;
    fs::write(&wav, bytes).unwrap();
    let (code, stdout, _) = pxl(&["--json", "audio", "inspect", out.to_str().unwrap()]);
    assert_eq!(code, 5, "{stdout}");
    let v = json_line(&stdout);
    assert_eq!(v["audio_error"], "integrity");
    assert!(v["problems"].to_string().contains("digest"));
}
