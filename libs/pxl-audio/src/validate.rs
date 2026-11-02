//! Semantic validation for `audio-spec/v1` and the rules that turn a spec into
//! backend requests. Nothing here touches Node.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{AudioError, AudioResult};
use crate::limits::*;
use crate::sandbox;
use crate::song::{validate_song, SongFacts};
use crate::spec::{parse_spec, require_finite, AudioSpec, BgmSpec, SfxParams, SfxSpec};

pub const WAVEFORMS: &[&str] = &["sine", "triangle", "saw", "tan", "noise", "square"];

/// A spec that passed every check, with songs loaded.
#[derive(Debug, Clone)]
pub struct ValidatedSpec {
    pub spec: AudioSpec,
    pub spec_digest_input: Vec<u8>,
    pub songs: Vec<ValidatedSong>,
    pub dependency_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ValidatedSong {
    pub id: String,
    pub rel_path: String,
    pub bytes: Vec<u8>,
    pub json: Value,
    pub facts: SongFacts,
}

fn id_ok(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && id.len() <= 64
}

pub fn validate_spec(spec_path: &Path) -> AudioResult<ValidatedSpec> {
    let bytes = sandbox::read_bounded(spec_path, MAX_JSON_BYTES)?;
    let spec = parse_spec(&bytes)?;
    let root = spec_path.parent().unwrap_or_else(|| Path::new("."));
    let mut dependency_bytes = bytes.len();

    if spec.contract != SPEC_CONTRACT {
        return Err(AudioError::schema(format!(
            "contract '{}' is not supported; expected '{SPEC_CONTRACT}'",
            spec.contract
        )));
    }
    if !id_ok(&spec.asset) {
        return Err(AudioError::semantic(format!(
            "asset id '{}' is invalid",
            spec.asset
        )));
    }
    if spec.sample_rate != SAMPLE_RATE {
        return Err(AudioError::semantic(format!(
            "sample_rate {} is unsupported; v1 renders {SAMPLE_RATE} Hz only",
            spec.sample_rate
        )));
    }
    if spec.seed > u32::MAX as u64 {
        return Err(AudioError::semantic("seed must fit in 32 bits"));
    }
    let total = spec.sfx.len() + spec.bgm.len();
    if total == 0 {
        return Err(AudioError::semantic("spec defines no sfx or bgm"));
    }
    if total > MAX_ASSETS {
        return Err(AudioError::limit(format!(
            "{total} assets exceeds the limit of {MAX_ASSETS}"
        )));
    }

    let mut ids = BTreeSet::new();
    for sfx in &spec.sfx {
        check_id(&sfx.id, &mut ids)?;
        validate_sfx(sfx)?;
    }
    let mut songs = Vec::new();
    for bgm in &spec.bgm {
        check_id(&bgm.id, &mut ids)?;
        let song = validate_bgm(bgm, root)?;
        dependency_bytes += song.bytes.len();
        if dependency_bytes > MAX_JSON_BYTES {
            return Err(AudioError::limit(format!(
                "spec and songs total {dependency_bytes} bytes; the limit is {MAX_JSON_BYTES}"
            )));
        }
        songs.push(song);
    }
    Ok(ValidatedSpec {
        spec,
        spec_digest_input: bytes,
        songs,
        dependency_bytes,
    })
}

fn check_id(id: &str, seen: &mut BTreeSet<String>) -> AudioResult<()> {
    if !id_ok(id) {
        return Err(AudioError::semantic(format!(
            "id '{id}' must match [A-Za-z_][A-Za-z0-9_-]* and be at most 64 characters"
        )));
    }
    if !seen.insert(id.to_string()) {
        return Err(AudioError::semantic(format!("duplicate id '{id}'")));
    }
    Ok(())
}

fn range(name: &str, value: Option<f64>, lo: f64, hi: f64) -> AudioResult<()> {
    if let Some(v) = value {
        require_finite(name, v)?;
        if v < lo || v > hi {
            return Err(AudioError::semantic(format!(
                "{name} {v} is outside {lo}..={hi}"
            )));
        }
    }
    Ok(())
}

fn validate_sfx(sfx: &SfxSpec) -> AudioResult<()> {
    if sfx.engine != "zzfx" {
        return Err(AudioError::semantic(format!(
            "sfx '{}' engine '{}' is unsupported; v1 supports zzfx",
            sfx.id, sfx.engine
        )));
    }
    require_finite("gain", sfx.gain)?;
    if !(0.0..=MAX_GAIN).contains(&sfx.gain) {
        return Err(AudioError::semantic(format!(
            "sfx '{}' gain must be 0..={MAX_GAIN}",
            sfx.id
        )));
    }
    let p = &sfx.params;
    if let Some(w) = &p.waveform {
        if !WAVEFORMS.contains(&w.as_str()) {
            return Err(AudioError::semantic(format!(
                "sfx '{}' waveform '{w}' is unknown",
                sfx.id
            )));
        }
    }
    range("volume", p.volume, 0.0, 2.0)?;
    range("randomness", p.randomness, 0.0, 1.0)?;
    range("frequency_hz", p.frequency_hz, 20.0, 20_000.0)?;
    range("attack_sec", p.attack_sec, 0.0, 2.0)?;
    range("decay_sec", p.decay_sec, 0.0, 2.0)?;
    range("sustain_sec", p.sustain_sec, 0.0, 2.0)?;
    range("release_sec", p.release_sec, 0.0, 2.0)?;
    range("shape_curve", p.shape_curve, 0.0, 10.0)?;
    range("slide", p.slide, -20.0, 20.0)?;
    range("delta_slide", p.delta_slide, -20.0, 20.0)?;
    range("pitch_jump_hz", p.pitch_jump_hz, -2000.0, 2000.0)?;
    range("pitch_jump_time_sec", p.pitch_jump_time_sec, 0.0, 2.0)?;
    range("repeat_time_sec", p.repeat_time_sec, 0.0, 2.0)?;
    range("noise", p.noise, 0.0, 10.0)?;
    range("modulation", p.modulation, -1000.0, 1000.0)?;
    range("bit_crush", p.bit_crush, 0.0, 10.0)?;
    range("delay_sec", p.delay_sec, 0.0, 2.0)?;
    range("sustain_volume", p.sustain_volume, 0.0, 2.0)?;
    range("tremolo", p.tremolo, 0.0, 1.0)?;
    range("filter_hz", p.filter_hz, -20_000.0, 20_000.0)?;
    let frames = sfx_frames(p);
    if frames == 0 {
        return Err(AudioError::semantic(format!(
            "sfx '{}' has no samples",
            sfx.id
        )));
    }
    if frames > MAX_SFX_FRAMES {
        return Err(AudioError::limit(format!(
            "sfx '{}' is {frames} frames; the limit is {MAX_SFX_FRAMES} (2 seconds)",
            sfx.id
        )));
    }
    Ok(())
}

/// Mirrors the frame count ZzFX computes: `attack + decay + sustain + release +
/// delay | 0`, with a nine-sample minimum attack. The adapter re-checks this.
pub fn sfx_frames(p: &SfxParams) -> u64 {
    let rate = SAMPLE_RATE as f64;
    let attack = p.attack_sec.unwrap_or(0.0) * rate;
    let attack = if attack == 0.0 { 9.0 } else { attack };
    let total = attack
        + p.decay_sec.unwrap_or(0.0) * rate
        + p.sustain_sec.unwrap_or(0.0) * rate
        + p.release_sec.unwrap_or(0.1) * rate
        + p.delay_sec.unwrap_or(0.0) * rate;
    if total <= 0.0 {
        0
    } else {
        total.trunc() as u64
    }
}

fn validate_bgm(bgm: &BgmSpec, root: &Path) -> AudioResult<ValidatedSong> {
    if bgm.engine != "beepbox" {
        return Err(AudioError::semantic(format!(
            "bgm '{}' engine '{}' is unsupported; v1 supports beepbox",
            bgm.id, bgm.engine
        )));
    }
    require_finite("gain", bgm.gain)?;
    if !(0.0..=MAX_GAIN).contains(&bgm.gain) {
        return Err(AudioError::semantic(format!(
            "bgm '{}' gain must be 0..={MAX_GAIN}",
            bgm.id
        )));
    }
    let path = sandbox::resolve(root, &bgm.song_file)?;
    let bytes = sandbox::read_bounded(&path, MAX_JSON_BYTES)?;
    let json: Value = serde_json::from_slice(&bytes).map_err(|e| {
        AudioError::schema(format!("{}: {e}", bgm.song_file)).with_path(bgm.song_file.clone())
    })?;
    let facts = validate_song(
        &json,
        bgm.loop_range.start_bar as u64,
        bgm.loop_range.end_bar_exclusive as u64,
    )
    .map_err(|mut e| {
        e.path = Some(bgm.song_file.clone());
        e
    })?;
    Ok(ValidatedSong {
        id: bgm.id.clone(),
        rel_path: bgm.song_file.clone(),
        bytes,
        json,
        facts,
    })
}

/// 32-bit FNV-1a over the root seed (8 bytes, little endian) then the id bytes.
pub fn derive_seed(root_seed: u64, id: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for byte in root_seed.to_le_bytes() {
        h ^= byte as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    for byte in id.as_bytes() {
        h ^= *byte as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub fn spec_root(spec_path: &Path) -> PathBuf {
    spec_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl-audio-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("audio.spec.json");
        fs::write(&path, body).unwrap();
        path
    }

    const GOOD: &str = r#"{"contract":"audio-spec/v1","asset":"pack","seed":7,
        "sfx":[{"id":"fire","engine":"zzfx","params":{"waveform":"square","frequency_hz":220,"sustain_sec":0.03,"release_sec":0.12},"gain":0.5}]}"#;

    #[test]
    fn accepts_a_valid_sfx_spec() {
        let dir = tmp("good");
        let v = validate_spec(&write(&dir, GOOD)).unwrap();
        assert_eq!(v.spec.sfx.len(), 1);
    }

    #[test]
    fn rejects_unknown_field_unknown_version_and_empty() {
        let dir = tmp("schema");
        let bad = GOOD.replace("\"gain\":0.5", "\"gain\":0.5,\"typo\":1");
        assert_eq!(
            validate_spec(&write(&dir, &bad)).unwrap_err().kind,
            crate::error::AudioErrorKind::Schema
        );
        let ver = GOOD.replace("audio-spec/v1", "audio-spec/v9");
        assert!(validate_spec(&write(&dir, &ver))
            .unwrap_err()
            .message
            .contains("not supported"));
        let empty = r#"{"contract":"audio-spec/v1","asset":"pack","seed":1}"#;
        assert!(validate_spec(&write(&dir, empty))
            .unwrap_err()
            .message
            .contains("no sfx or bgm"));
    }

    #[test]
    fn rejects_out_of_range_duplicate_and_long_sfx() {
        let dir = tmp("range");
        let hz = GOOD.replace("\"frequency_hz\":220", "\"frequency_hz\":5");
        assert!(validate_spec(&write(&dir, &hz))
            .unwrap_err()
            .message
            .contains("frequency_hz"));
        let long = GOOD.replace("\"sustain_sec\":0.03", "\"sustain_sec\":2.0");
        assert_eq!(
            validate_spec(&write(&dir, &long)).unwrap_err().kind,
            crate::error::AudioErrorKind::Limit
        );
        let dup = r#"{"contract":"audio-spec/v1","asset":"pack","seed":1,"sfx":[
            {"id":"a","engine":"zzfx","params":{}},{"id":"a","engine":"zzfx","params":{}}]}"#;
        assert!(validate_spec(&write(&dir, dup))
            .unwrap_err()
            .message
            .contains("duplicate"));
    }

    #[test]
    fn rejects_paths_that_escape_the_spec_directory() {
        let dir = tmp("escape");
        for bad in ["../x.json", "/etc/passwd", "http://x/y.json", "a\\\\b.json"] {
            let body = format!(
                r#"{{"contract":"audio-spec/v1","asset":"pack","seed":1,"bgm":[
                {{"id":"t","engine":"beepbox","song_file":"{bad}","loop":{{"start_bar":0,"end_bar_exclusive":2}}}}]}}"#
            );
            assert!(validate_spec(&write(&dir, &body)).is_err(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let dir = tmp("symlink");
        let outside = tmp("symlink-outside");
        fs::write(outside.join("s.json"), "{}").unwrap();
        std::os::unix::fs::symlink(outside.join("s.json"), dir.join("s.json")).unwrap();
        let body = r#"{"contract":"audio-spec/v1","asset":"pack","seed":1,"bgm":[
            {"id":"t","engine":"beepbox","song_file":"s.json","loop":{"start_bar":0,"end_bar_exclusive":2}}]}"#;
        assert!(validate_spec(&write(&dir, body))
            .unwrap_err()
            .message
            .contains("outside"));
    }

    #[test]
    fn derived_seeds_are_stable_and_distinct() {
        assert_eq!(derive_seed(7, "fire"), derive_seed(7, "fire"));
        assert_ne!(derive_seed(7, "fire"), derive_seed(7, "coin"));
        assert_ne!(derive_seed(7, "fire"), derive_seed(8, "fire"));
    }

    /// Vectors computed by an independent Python implementation of 32-bit
    /// FNV-1a over the little-endian root seed followed by the id bytes.
    /// Changing the derivation changes every rendered sample, so it is a
    /// contract break and needs a new `audio-spec` version.
    #[test]
    fn seed_derivation_is_frozen() {
        assert_eq!(derive_seed(7, "fire"), 0x3838_f0b6);
        assert_eq!(derive_seed(7, "coin"), 0x32bc_cb4b);
        assert_eq!(derive_seed(8, "fire"), 0x32a4_5391);
        assert_eq!(derive_seed(0, ""), 0x9be1_7165);
    }
}
