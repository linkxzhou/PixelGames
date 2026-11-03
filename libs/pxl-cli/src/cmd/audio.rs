//! `step-300 audio`: validate, render and inspect audio specs.
//!
//! Exit codes stay inside step-300's frozen 0-6 range. An audio error maps onto
//! the nearest existing code, and the JSON error carries `audio_error` with the
//! precise kind (`blocked`, `limit`, `backend`, ...) so callers can tell them
//! apart without a new exit code.

use std::path::{Path, PathBuf};

use pxl_audio::error::{AudioError, AudioErrorKind};
use pxl_audio::inspect::{inspect_dir, inspect_wav};
use pxl_audio::render::{render_spec, RenderOptions};
use pxl_audio::validate::validate_spec;
use pxl_core::PxlError;
use serde_json::{json, Value};

use super::args::{AudioCmd, AudioInspectArgs, AudioRenderArgs, AudioValidateArgs};
use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

pub(super) fn audio_cmd(ctx: &Ctx, action: AudioCmd) -> CmdResult<()> {
    util::check_lock(ctx, None)?;
    match action {
        AudioCmd::Validate(a) => validate(ctx, a),
        AudioCmd::Render(a) => render(ctx, a),
        AudioCmd::Inspect(a) => inspect(ctx, a),
    }
}

fn kind_name(kind: AudioErrorKind) -> &'static str {
    match kind {
        AudioErrorKind::Schema => "schema",
        AudioErrorKind::Semantic => "semantic",
        AudioErrorKind::Limit => "limit",
        AudioErrorKind::Io => "io",
        AudioErrorKind::Blocked => "blocked",
        AudioErrorKind::Backend => "backend",
        AudioErrorKind::Qa => "qa",
        AudioErrorKind::Exists => "exists",
    }
}

pub(super) fn to_cmd_error(err: AudioError) -> CmdError {
    let kind = err.kind;
    let mut pxl = match kind {
        AudioErrorKind::Schema => PxlError::schema(err.message.clone()),
        AudioErrorKind::Semantic | AudioErrorKind::Limit => PxlError::semantic(err.message.clone()),
        AudioErrorKind::Qa => PxlError::qa(err.message.clone()),
        AudioErrorKind::Exists => PxlError::exists(&err.message),
        AudioErrorKind::Io | AudioErrorKind::Blocked | AudioErrorKind::Backend => {
            PxlError::io(err.message.clone())
        }
    };
    if let Some(path) = &err.path {
        pxl = pxl.with_path(path.clone());
    }
    if let Some(hint) = &err.hint {
        pxl = pxl.with_hint(hint.clone());
    }
    CmdError::new(pxl).with_extra(json!({ "audio_error": kind_name(kind) }))
}

fn validate(ctx: &Ctx, args: AudioValidateArgs) -> CmdResult<()> {
    let validated = validate_spec(Path::new(&args.spec)).map_err(to_cmd_error)?;
    let sfx: Vec<Value> = validated
        .spec
        .sfx
        .iter()
        .map(|s| {
            json!({
                "id": s.id, "kind": "sfx", "engine": s.engine,
                "frames": pxl_audio::validate::sfx_frames(&s.params),
            })
        })
        .collect();
    let bgm: Vec<Value> = validated
        .songs
        .iter()
        .map(|s| {
            json!({
                "id": s.id, "kind": "bgm", "engine": "beepbox", "song_file": s.rel_path,
                "frames": s.facts.loop_frames, "bars": s.facts.bars, "bpm": s.facts.bpm,
                "voices": s.facts.voices,
            })
        })
        .collect();
    let mut body = util::ok_base("audio validate");
    body.insert("contract".into(), json!(validated.spec.contract));
    body.insert("asset".into(), json!(validated.spec.asset));
    body.insert("dependency_bytes".into(), json!(validated.dependency_bytes));
    body.insert("sfx".into(), json!(sfx));
    body.insert("bgm".into(), json!(bgm));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn render(ctx: &Ctx, args: AudioRenderArgs) -> CmdResult<()> {
    let options = RenderOptions {
        out: PathBuf::from(&args.out),
        overwrite: ctx.overwrite,
        strict: ctx.strict,
        seed_override: ctx.seed,
        only: args.only,
        adapter: args.adapter.map(PathBuf::from),
    };
    let outcome = render_spec(Path::new(&args.spec), &options).map_err(to_cmd_error)?;
    let entries: Vec<Value> = outcome
        .manifest
        .entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id, "kind": e.kind, "path": e.path, "frames": e.frames,
                "channels": e.channels, "pcm_sha256": e.pcm_sha256,
                "file_sha256": e.file_sha256, "qa": e.qa.status,
            })
        })
        .collect();
    let mut body = util::ok_base("audio render");
    body.insert("out".into(), json!(outcome.out.to_string_lossy()));
    body.insert("manifest".into(), json!(pxl_audio::render::MANIFEST_FILE));
    body.insert("entries".into(), json!(entries));
    body.insert("warnings".into(), json!(outcome.warnings));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn inspect(ctx: &Ctx, args: AudioInspectArgs) -> CmdResult<()> {
    let path = Path::new(&args.path);
    if path.is_dir() {
        let report = inspect_dir(path).map_err(to_cmd_error)?;
        if !report.ok {
            return Err(CmdError::new(
                PxlError::qa(format!(
                    "audio inspect found {} problem(s)",
                    report.problems.len()
                ))
                .with_hint(report.problems.join("; ")),
            )
            .with_extra(json!({
                "cmd": "audio inspect",
                "audio_error": "integrity",
                "entries": report.entries,
                "problems": report.problems,
            })));
        }
        let mut body = util::ok_base("audio inspect");
        body.insert("entries".into(), json!(report.entries));
        body.insert("problems".into(), json!(report.problems));
        util::emit_ok(ctx, &Value::Object(body));
        Ok(())
    } else {
        let facts = inspect_wav(path).map_err(to_cmd_error)?;
        let mut body = util::ok_base("audio inspect");
        if let Value::Object(map) = facts {
            body.extend(map);
        }
        util::emit_ok(ctx, &Value::Object(body));
        Ok(())
    }
}
