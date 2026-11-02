//! `audio inspect`: re-read what a render published and check it against its
//! manifest. This is the integrity half of the audio gate; it needs no Node.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::error::{AudioError, AudioResult};
use crate::limits::*;
use crate::manifest::Manifest;
use crate::qa::{self, QaInput};
use crate::render::{sha256_tagged, MANIFEST_FILE};
use crate::wav;

#[derive(Debug)]
pub struct InspectReport {
    pub ok: bool,
    pub entries: Vec<Value>,
    pub problems: Vec<String>,
}

pub fn inspect_dir(dir: &Path) -> AudioResult<InspectReport> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path).map_err(|e| {
        AudioError::io(format!("cannot read {}: {e}", manifest_path.display()))
            .with_hint("point inspect at a directory written by `audio render`")
    })?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| AudioError::schema(format!("{}: {e}", manifest_path.display())))?;
    if manifest.contract != MANIFEST_CONTRACT {
        return Err(AudioError::schema(format!(
            "manifest contract '{}' is not supported; expected '{MANIFEST_CONTRACT}'",
            manifest.contract
        )));
    }
    let mut problems = Vec::new();
    let mut entries = Vec::new();
    for entry in &manifest.entries {
        let path = dir.join(&entry.path);
        let mut entry_problems: Vec<String> = Vec::new();
        let bytes = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                entry_problems.push(format!("cannot read {}: {e}", entry.path));
                None
            }
        };
        let mut facts = json!({"id": entry.id, "kind": entry.kind, "path": entry.path});
        if let Some(bytes) = &bytes {
            if sha256_tagged(bytes) != entry.file_sha256 {
                entry_problems.push("file digest does not match the manifest".into());
            }
            match wav::decode(bytes) {
                Ok(decoded) => {
                    if decoded.sample_rate != manifest.sample_rate {
                        entry_problems.push(format!(
                            "sample rate {} != {}",
                            decoded.sample_rate, manifest.sample_rate
                        ));
                    }
                    if decoded.channels != entry.channels {
                        entry_problems.push(format!(
                            "channels {} != {}",
                            decoded.channels, entry.channels
                        ));
                    }
                    if decoded.frames != entry.frames {
                        entry_problems
                            .push(format!("frames {} != {}", decoded.frames, entry.frames));
                    }
                    if sha256_tagged(&decoded.pcm) != entry.pcm_sha256 {
                        entry_problems.push("pcm digest does not match the manifest".into());
                    }
                    if let Some(range) = &entry.loop_frames {
                        if range.start_frame >= range.end_frame_exclusive
                            || range.end_frame_exclusive > decoded.frames
                        {
                            entry_problems.push("loop range is outside the file".into());
                        }
                    }
                    let samples = decoded.samples();
                    let max_frames = if entry.kind == "bgm" {
                        MAX_BGM_FRAMES
                    } else {
                        MAX_SFX_FRAMES
                    };
                    let report = qa::run(&QaInput {
                        samples: &samples,
                        channels: decoded.channels,
                        clipped: entry.qa.clipped_samples,
                        is_loop: entry.loop_frames.is_some(),
                        expected_frames: Some(entry.frames),
                        max_frames,
                    });
                    for check in report.errors() {
                        entry_problems.push(format!("qa {} failed", check.id));
                    }
                    facts["frames"] = json!(decoded.frames);
                    facts["channels"] = json!(decoded.channels);
                    facts["seconds"] = json!(qa::seconds(decoded.frames));
                    facts["peak_dbfs"] = json!(report.peak_dbfs);
                    facts["rms_dbfs"] = json!(report.rms_dbfs);
                    facts["qa"] = json!(report.status());
                }
                Err(e) => entry_problems.push(e.message),
            }
        }
        facts["ok"] = json!(entry_problems.is_empty());
        if !entry_problems.is_empty() {
            facts["problems"] = json!(entry_problems);
            for p in &entry_problems {
                problems.push(format!("{}: {p}", entry.id));
            }
        }
        entries.push(facts);
    }
    Ok(InspectReport {
        ok: problems.is_empty(),
        entries,
        problems,
    })
}

pub fn inspect_wav(path: &Path) -> AudioResult<Value> {
    let bytes = fs::read(path)
        .map_err(|e| AudioError::io(format!("cannot read {}: {e}", path.display())))?;
    let decoded = wav::decode(&bytes)?;
    let samples = decoded.samples();
    let report = qa::run(&QaInput {
        samples: &samples,
        channels: decoded.channels,
        clipped: 0,
        is_loop: false,
        expected_frames: None,
        max_frames: MAX_BGM_FRAMES,
    });
    Ok(json!({
        "path": path.to_string_lossy(),
        "channels": decoded.channels,
        "sample_rate": decoded.sample_rate,
        "frames": decoded.frames,
        "seconds": qa::seconds(decoded.frames),
        "file_sha256": sha256_tagged(&bytes),
        "pcm_sha256": sha256_tagged(&decoded.pcm),
        "peak_dbfs": report.peak_dbfs,
        "rms_dbfs": report.rms_dbfs,
        "qa": report.status(),
        "checks": report.checks,
    }))
}
