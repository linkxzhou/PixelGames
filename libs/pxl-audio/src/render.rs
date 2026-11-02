//! Render an audio spec into WAV files plus `audio-manifest.json`.
//!
//! Output is staged in `.<name>.rejected` next to `--out`. Only a fully
//! successful render replaces `--out`; on failure the staging directory stays
//! as evidence and the previous output is untouched.

use std::fs;
use std::path::{Path, PathBuf};

use pxl_core::sha256_hex;
use serde_json::json;

use crate::backend::{self, Backend};
use crate::error::{AudioError, AudioErrorKind, AudioResult};
use crate::limits::*;
use crate::manifest::{Entry, EntryQa, LoopFrames, Manifest, Toolchain};
use crate::qa::{self, QaInput};
use crate::validate::{derive_seed, sfx_frames, validate_spec, ValidatedSpec};
use crate::wav;

pub const MANIFEST_FILE: &str = "audio-manifest.json";

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    pub out: PathBuf,
    pub overwrite: bool,
    pub strict: bool,
    pub seed_override: Option<u64>,
    pub only: Option<String>,
    pub adapter: Option<PathBuf>,
}

#[derive(Debug)]
pub struct RenderOutcome {
    pub manifest: Manifest,
    pub out: PathBuf,
    pub warnings: Vec<String>,
}

pub fn rejected_path(out: &Path) -> PathBuf {
    let name = out
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    out.with_file_name(format!(".{name}.rejected"))
}

pub fn sha256_tagged(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

pub fn render_spec(spec_path: &Path, options: &RenderOptions) -> AudioResult<RenderOutcome> {
    let validated = validate_spec(spec_path)?;
    if options.out.exists() && !options.overwrite {
        return Err(AudioError::new(
            AudioErrorKind::Exists,
            format!("output {} already exists", options.out.display()),
        )
        .with_hint("pass --overwrite to replace it, or choose a new --out"));
    }
    if let Some(only) = &options.only {
        let known = validated
            .spec
            .sfx
            .iter()
            .map(|s| &s.id)
            .chain(validated.spec.bgm.iter().map(|b| &b.id));
        if !known.into_iter().any(|id| id == only) {
            return Err(AudioError::semantic(format!(
                "no sfx or bgm with id '{only}'"
            )));
        }
    }
    let backend = backend::locate(options.adapter.as_deref())?;
    let toolchain = toolchain_of(&backend)?;

    let staging = rejected_path(&options.out);
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| AudioError::io(format!("cannot clear {}: {e}", staging.display())))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| AudioError::io(format!("cannot create {}: {e}", staging.display())))?;

    let result = render_into(&validated, options, &backend, toolchain, &staging);
    let (manifest, warnings, failure) = match result {
        Ok((manifest, warnings)) => (manifest, warnings, None),
        Err(failed) => {
            let (manifest, warnings, err) = *failed;
            (manifest, warnings, Some(err))
        }
    };
    write_manifest(&staging, &manifest)?;

    if let Some(err) = failure {
        return Err(err.with_hint(format!("inspect {}", staging.display())));
    }
    if options.strict && !warnings.is_empty() {
        return Err(AudioError::qa(format!(
            "{} warning(s) treated as errors because --strict is set: {}",
            warnings.len(),
            warnings.join("; ")
        ))
        .with_hint(format!("inspect {}", staging.display())));
    }
    publish(&staging, &options.out)?;
    Ok(RenderOutcome {
        manifest,
        out: options.out.clone(),
        warnings,
    })
}

type Partial = (Manifest, Vec<String>);
type PartialFail = Box<(Manifest, Vec<String>, AudioError)>;
type EntryFail = Box<(Option<Entry>, AudioError)>;

fn render_into(
    validated: &ValidatedSpec,
    options: &RenderOptions,
    backend: &Backend,
    toolchain: Toolchain,
    staging: &Path,
) -> Result<Partial, PartialFail> {
    let spec = &validated.spec;
    let root_seed = options.seed_override.unwrap_or(spec.seed);
    let spec_sha = sha256_hex(&validated.spec_digest_input);
    let mut manifest = Manifest {
        schema_version: 1,
        contract: MANIFEST_CONTRACT.to_string(),
        asset: spec.asset.clone(),
        root_seed,
        sample_rate: SAMPLE_RATE,
        pcm_format: PCM_FORMAT.to_string(),
        spec_sha256: format!("sha256:{spec_sha}"),
        toolchain,
        qa_profile: QA_PROFILE.to_string(),
        entries: Vec::new(),
    };
    let mut warnings = Vec::new();

    let wanted = |id: &str| options.only.as_deref().is_none_or(|only| only == id);

    let mut sfx_sorted: Vec<_> = spec.sfx.iter().collect();
    sfx_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for sfx in sfx_sorted {
        if !wanted(&sfx.id) {
            continue;
        }
        let seed = derive_seed(root_seed, &sfx.id);
        let request = json!({
            "kind": "sfx", "seed": seed, "gain": sfx.gain,
            "params": sfx.params, "max_frames": MAX_SFX_FRAMES,
        });
        let expected = sfx_frames(&sfx.params);
        let deps = deps_digest(&spec_sha, None);
        let entry = render_entry(
            backend,
            staging,
            "sfx",
            &sfx.id,
            "zzfx",
            seed,
            &request,
            expected,
            false,
            deps,
            None,
            MAX_SFX_FRAMES,
        );
        match entry {
            Ok((entry, mut w)) => {
                warnings.append(&mut w);
                manifest.entries.push(entry);
            }
            Err(failed) => {
                let (entry, err) = *failed;
                if let Some(entry) = entry {
                    manifest.entries.push(entry);
                }
                return Err(Box::new((manifest, warnings, err)));
            }
        }
    }

    let mut bgm_sorted: Vec<_> = spec.bgm.iter().collect();
    bgm_sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for bgm in bgm_sorted {
        if !wanted(&bgm.id) {
            continue;
        }
        let song = validated
            .songs
            .iter()
            .find(|s| s.id == bgm.id)
            .expect("validated song for every bgm");
        let seed = derive_seed(root_seed, &bgm.id);
        let request = json!({
            "kind": "bgm", "seed": seed, "gain": bgm.gain, "song": song.json,
            "loop_bars": song.facts.bars, "warmup": BGM_WARMUP_PASSES,
            "max_frames": MAX_BGM_FRAMES,
        });
        let deps = deps_digest(&spec_sha, Some(&sha256_hex(&song.bytes)));
        let entry = render_entry(
            backend,
            staging,
            "bgm",
            &bgm.id,
            "beepbox",
            seed,
            &request,
            song.facts.loop_frames,
            true,
            deps,
            Some(song.rel_path.clone()),
            MAX_BGM_FRAMES,
        );
        match entry {
            Ok((entry, mut w)) => {
                warnings.append(&mut w);
                manifest.entries.push(entry);
            }
            Err(failed) => {
                let (entry, err) = *failed;
                if let Some(entry) = entry {
                    manifest.entries.push(entry);
                }
                return Err(Box::new((manifest, warnings, err)));
            }
        }
    }
    Ok((manifest, warnings))
}

pub fn deps_digest(spec_sha: &str, song_sha: Option<&str>) -> String {
    let mut text = format!("spec:{spec_sha}\n");
    if let Some(song) = song_sha {
        text.push_str(&format!("song:{song}\n"));
    }
    sha256_tagged(text.as_bytes())
}

#[allow(clippy::too_many_arguments)]
fn render_entry(
    backend: &Backend,
    staging: &Path,
    kind: &str,
    id: &str,
    engine: &str,
    seed: u32,
    request: &serde_json::Value,
    expected_frames: u64,
    is_loop: bool,
    deps_sha256: String,
    song_file: Option<String>,
    max_frames: u64,
) -> Result<(Entry, Vec<String>), EntryFail> {
    let fail = |entry: Option<Entry>, err: AudioError| -> EntryFail { Box::new((entry, err)) };
    let rendered = backend::render(backend, request).map_err(|e| fail(None, annotate(e, id)))?;
    let (samples, clipped) = wav::quantize(&rendered.pcm);
    let report = qa::run(&QaInput {
        samples: &samples,
        channels: rendered.channels,
        clipped,
        is_loop,
        expected_frames: Some(expected_frames),
        max_frames,
    });
    let wav_bytes =
        wav::encode(&samples, rendered.channels).map_err(|e| fail(None, annotate(e, id)))?;
    let rel = format!("{kind}/{id}.wav");
    let path = staging.join(&rel);
    fs::create_dir_all(path.parent().expect("has parent")).map_err(|e| {
        fail(
            None,
            AudioError::io(format!("cannot create {kind} directory: {e}")),
        )
    })?;
    fs::write(&path, &wav_bytes)
        .map_err(|e| fail(None, AudioError::io(format!("cannot write {rel}: {e}"))))?;
    let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let entry = Entry {
        id: id.to_string(),
        kind: kind.to_string(),
        path: rel,
        engine: engine.to_string(),
        seed,
        channels: rendered.channels,
        frames: rendered.frames,
        file_sha256: sha256_tagged(&wav_bytes),
        pcm_sha256: sha256_tagged(&pcm_bytes),
        deps_sha256,
        song_file,
        loop_frames: is_loop.then_some(LoopFrames {
            start_frame: 0,
            end_frame_exclusive: rendered.frames,
        }),
        qa: EntryQa::from_report(&report),
    };
    if !report.ok() {
        let failed: Vec<String> = report.errors().map(|c| c.id.to_string()).collect();
        return Err(fail(
            Some(entry),
            AudioError::qa(format!("'{id}' failed audio QA: {}", failed.join(", "))),
        ));
    }
    let warnings = report
        .warnings()
        .map(|c| format!("{id}: {} ({:.3} vs limit {:.3})", c.id, c.value, c.limit))
        .collect();
    Ok((entry, warnings))
}

fn annotate(mut err: AudioError, id: &str) -> AudioError {
    err.message = format!("'{id}': {}", err.message);
    err
}

fn toolchain_of(backend: &Backend) -> AudioResult<Toolchain> {
    let vendor_path = backend.adapter_dir.join("vendor/VENDOR.json");
    let text = fs::read_to_string(&vendor_path)
        .map_err(|e| AudioError::blocked(format!("cannot read {}: {e}", vendor_path.display())))?;
    let vendor: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        AudioError::blocked(format!("{} is not valid JSON: {e}", vendor_path.display()))
    })?;
    let version_of = |name: &str| -> AudioResult<String> {
        vendor["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name)
            .and_then(|p| p["version"].as_str())
            .map(str::to_string)
            .ok_or_else(|| AudioError::blocked(format!("VENDOR.json does not list {name}")))
    };
    Ok(Toolchain {
        node: backend.node_version.clone(),
        adapter_version: backend.adapter_version.clone(),
        adapter_sha256: adapter_digest(&backend.adapter_dir)?,
        beepbox: version_of("beepbox")?,
        zzfx: version_of("zzfx")?,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    })
}

/// Digest of every adapter source file (not tests or caches), so a changed or
/// patched toolchain cannot render under the same manifest.
pub fn adapter_digest(dir: &Path) -> AudioResult<String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    files.sort();
    let mut text = String::new();
    for (rel, digest) in files {
        text.push_str(&format!("{rel} {digest}\n"));
    }
    Ok(sha256_tagged(text.as_bytes()))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> AudioResult<()> {
    let entries = fs::read_dir(dir)
        .map_err(|e| AudioError::io(format!("cannot read {}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "tests" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let bytes = fs::read(&path)
                .map_err(|e| AudioError::io(format!("cannot read {}: {e}", path.display())))?;
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, sha256_hex(&bytes)));
        }
    }
    Ok(())
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> AudioResult<()> {
    let text = manifest
        .to_json()
        .map_err(|e| AudioError::io(format!("cannot encode the manifest: {e}")))?;
    fs::write(dir.join(MANIFEST_FILE), text)
        .map_err(|e| AudioError::io(format!("cannot write the manifest: {e}")))
}

fn publish(staging: &Path, out: &Path) -> AudioResult<()> {
    if out.exists() {
        fs::remove_dir_all(out)
            .map_err(|e| AudioError::io(format!("cannot replace {}: {e}", out.display())))?;
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| AudioError::io(format!("cannot create {}: {e}", parent.display())))?;
        }
    }
    fs::rename(staging, out)
        .map_err(|e| AudioError::io(format!("cannot publish {}: {e}", out.display())))
}
