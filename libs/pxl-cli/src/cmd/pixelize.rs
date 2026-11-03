//! `pxl pixelize`: turn any PNG into a palette-indexed frame. With no `--size`
//! the picture's own pixel grid is recovered, which is what a "fake" pixel-art
//! image (upscaled, or AI-drawn at 4x) needs before it can be edited.

use std::path::Path;

use pxl_art::{Downscale, PixelizeOptions, Rgba8};
use pxl_core::{Palette, PxlError, PxlResult, SizeProfile};
use pxl_export::{decode_png_rgba, render_asset_png, render_preview_png};
use serde_json::{json, Value};

use super::args::PixelizeArgs;
use crate::error::CmdResult;
use crate::util::{self, Ctx, DirResolver};

pub(super) fn pixelize_cmd(ctx: &Ctx, args: PixelizeArgs) -> CmdResult<()> {
    let bytes =
        std::fs::read(&args.input).map_err(|e| PxlError::io(format!("{}: {e}", args.input)))?;
    let (w, h, data) = decode_png_rgba(&bytes)?;
    let img = Rgba8 {
        w: w as usize,
        h: h as usize,
        data,
    };
    let method = Downscale::parse(&args.method).ok_or_else(|| {
        PxlError::usage(format!("unknown method '{}'", args.method))
            .with_hint("method is nearest, mode, kcentroid, or contrast")
    })?;
    let base = Path::new(&args.input)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let palette = match &args.palette {
        Some(name) => Some(pxl_engine::load_palette(name, &DirResolver::new(&base))?),
        None => None,
    };
    let opts = PixelizeOptions {
        size: args.size.map(|s| (s, s)),
        method,
        outline_radius: args.outline,
        palette: palette.clone(),
        colors: args.colors,
        dither: args.dither,
    };
    let done = pxl_art::pixelize(&img, &opts)?;
    let profile = SizeProfile::for_size(done.frame.w, done.frame.h)?;
    let mut palette = done.palette.clone();
    let mut frame = done.frame.clone();
    let mut stages: Vec<String> = Vec::new();
    if !args.no_finish {
        let fin = pxl_art::FinishOptions::for_profile(&profile);
        let finished = pxl_art::finish(&frame, &palette, &fin)?;
        stages = finished
            .report
            .stages
            .iter()
            .map(|s| s.to_string())
            .collect();
        frame = finished.frame;
        palette = finished.palette;
    }
    if palette.colors.len() > profile.max_colors {
        return Err(PxlError::semantic(format!(
            "pixelize.palette_over_tier: {} colours for the {} tier (cap {})",
            palette.colors.len(),
            profile.tier.name(),
            profile.max_colors
        ))
        .with_hint("pass --colors with a smaller value, or --palette for a fixed one")
        .into());
    }
    let png = if args.asset {
        render_asset_png(&frame, &palette, args.scale.max(1))?
    } else {
        render_preview_png(&frame, &palette, args.scale.max(1), false)?
    };
    util::write_out(&args.out, &png, ctx.overwrite, ctx.json)?;
    let doc_path = args.doc.clone();
    if let Some(path) = &doc_path {
        let doc = doc_from(&frame, &palette, &args.out)?;
        util::save_doc(&doc, path, ctx.overwrite, ctx.json)?;
    }
    let mut body = util::ok_base("pixelize");
    body.insert("out".into(), json!(args.out));
    body.insert("size".into(), json!([frame.w, frame.h]));
    body.insert("source".into(), json!([w, h]));
    body.insert("grid".into(), json!({"cell": done.grid.cell, "phase": [done.grid.phase_x, done.grid.phase_y], "score": done.grid.score}));
    body.insert("outline_radius".into(), json!(done.outline_radius));
    body.insert("method".into(), json!(method.name()));
    body.insert("colors".into(), json!(palette.colors.len()));
    body.insert("dither".into(), json!(args.dither));
    body.insert("profile".into(), json!(profile.tier.name()));
    body.insert("finish".into(), json!(stages));
    body.insert("hash".into(), json!(util::hash_of(&png)));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn doc_from(
    frame: &pxl_core::IndexFrame,
    palette: &Palette,
    out: &str,
) -> PxlResult<pxl_format::PxlDoc> {
    let stem = Path::new(out)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pixelized");
    let mut doc = pxl_format::PxlDoc {
        v: 1,
        palette: format!("{stem}.palette.json"),
        w: frame.w,
        h: frame.h,
        frames: Default::default(),
        anims: Default::default(),
        meta: pxl_format::Meta {
            pxl_version: Some(pxl_core::PXL_VERSION.into()),
            gen_version: Some(pxl_core::GEN_VERSION),
            kind: Some("sprite".into()),
            ..Default::default()
        },
    };
    doc.insert_frame("base", frame)?;
    palette.check_text_encodable()?;
    Ok(doc)
}
