use std::collections::BTreeMap;
use std::path::Path;

use pxl_core::PxlError;
use pxl_format::{Anim, AnimFrames, Op};
use pxl_ops::expand_macro;
use serde_json::{json, Value};

use super::args::AnimArgs;
use super::document::{apply_with, frames_vec, load_parts};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn anim_cmd(ctx: &Ctx, args: AnimArgs) -> CmdResult<()> {
    let (mut doc, doc_path) = util::load_doc(&args.input)?;
    util::check_lock(ctx, doc.meta.pxl_version.as_deref())?;
    let palette = util::doc_palette(&doc, &doc_path)?;
    let base = doc_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let seed = ctx.seed.or(doc.meta.seed).unwrap_or(1);
    let parts = load_parts(&doc, &base)?;
    let dirs: Vec<String> = args
        .dirs
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let fps = args.fps.unwrap_or(default_fps(&args.name));
    check_anim_timing(&args.name, fps, 1)?;
    let targets: Vec<Option<String>> = if dirs.is_empty() {
        vec![None]
    } else {
        dirs.iter().cloned().map(Some).collect()
    };
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut flat = Vec::new();
    for target in &targets {
        let source_name = target.clone().unwrap_or_else(|| "base".into());
        let source = doc.frame(&source_name).map_err(|e| {
            e.with_hint(format!(
                "create frame '{source_name}' before animating it (noop / rotate90 / flip)"
            ))
        })?;
        let lists = if let Some(mac) = &args.macro_call {
            expand_macro(mac, target.as_deref(), &source, &palette, parts.as_ref())?
        } else if let Some(path) = &args.frames {
            let text = util::read_text(path)?;
            let lists: Vec<Vec<Op>> = pxl_format::parse_json(&text, "anim frames")?;
            lists
        } else {
            return Err(PxlError::usage("anim needs --macro or --frames")
                .with_hint("example: --macro 'tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)'")
                .into());
        };
        if lists.is_empty() || lists.len() > MAX_ANIM_FRAMES {
            return Err(PxlError::schema(format!(
                "animation '{}' has {} frames; expected 1..=16",
                args.name,
                lists.len()
            ))
            .into());
        }
        let mut names = Vec::new();
        for (i, ops) in lists.iter().enumerate() {
            let name = match target {
                Some(dir) => format!("{}_{dir}_{i}", args.name),
                None => format!("{}_{i}", args.name),
            };
            let frame = apply_with(&source, ops, &palette, seed, &name, &base, parts.as_ref())?;
            doc.insert_frame(&name, &frame)?;
            names.push(name);
        }
        if let Some(dir) = target {
            by_dir.insert(dir.clone(), names);
        } else {
            flat = names;
        }
    }
    let frames = if by_dir.is_empty() {
        AnimFrames::Seq(flat)
    } else {
        AnimFrames::ByDir(by_dir)
    };
    doc.anims.insert(
        args.name.clone(),
        Anim {
            fps,
            looped: !args.no_loop,
            dirs: dirs.clone(),
            frames,
            mode: pxl_format::AnimMode::Dynamic,
        },
    );
    let hash = util::save_doc(&doc, &args.out, ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("anim");
    body.insert("out".into(), json!(args.out));
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert(
        "colors_used".into(),
        json!(util::colors_used(&frames_vec(&doc)?)),
    );
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn default_fps(name: &str) -> u32 {
    match name {
        "move" => 10,
        "idle" => 4,
        _ => 8,
    }
}

/// Limits every animation obeys, whether it comes from `pxl anim` or a recipe,
/// so no entry point can describe motion another one refuses to preview.
pub(super) const MAX_ANIM_FPS: u32 = 60;

pub(super) const MAX_ANIM_FRAMES: usize = 16;

pub(super) fn check_anim_timing(name: &str, fps: u32, frame_count: usize) -> CmdResult<()> {
    if fps == 0 || fps > MAX_ANIM_FPS {
        return Err(PxlError::schema(format!(
            "animation '{name}' has fps {fps}; fps must be in 1..={MAX_ANIM_FPS}"
        ))
        .with_path(format!("/anims/{name}/fps"))
        .into());
    }
    if frame_count == 0 || frame_count > MAX_ANIM_FRAMES {
        return Err(PxlError::schema(format!(
            "animation '{name}' has {frame_count} frames; expected 1..={MAX_ANIM_FRAMES}"
        ))
        .with_path(format!("/anims/{name}/frames"))
        .into());
    }
    Ok(())
}
