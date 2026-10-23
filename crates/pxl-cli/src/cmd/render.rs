use std::fs;
use std::path::Path;

use pxl_core::PxlError;
use pxl_export::{
    decode_png_rgba, encode_gif_timed, encode_rgba_png, gif_delays_cs, pack_sheet,
    render_preview_png, scale_rgba_checked, SheetCell,
};
use serde_json::{json, Value};

use super::args::RenderArgs;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn render_cmd(ctx: &Ctx, args: RenderArgs) -> CmdResult<()> {
    if args.scale == 0 || args.scale > 16 {
        return Err(PxlError::usage("scale must be in 1..=16").into());
    }
    let path = Path::new(&args.input);
    if args.input.ends_with(".png") || looks_like_png(&args.input) {
        if args.out.ends_with(".gif") {
            return Err(PxlError::usage(format!(
                "cannot write {} from a png input: a png has no animation",
                args.out
            ))
            .with_hint("render a .pxl.json document with --anim for a gif, or name the output .png")
            .into());
        }
        let bytes =
            fs::read(&args.input).map_err(|e| PxlError::io(format!("{}: {e}", args.input)))?;
        let (w, h, rgba) = decode_png_rgba(&bytes)?;
        let (ow, oh, scaled) = scale_rgba_checked(w, h, &rgba, args.scale, args.grid)?;
        let png = encode_rgba_png(ow, oh, &scaled)?;
        let hash = util::hash_of(&png);
        util::write_out(&args.out, &png, ctx.overwrite, ctx.json)?;
        let mut body = util::ok_base("render");
        body.insert("out".into(), json!(args.out));
        body.insert("png".into(), json!(args.out));
        body.insert("png_w".into(), json!(ow));
        body.insert("png_h".into(), json!(oh));
        body.insert("frames_count".into(), json!(1));
        body.insert("hash".into(), json!(hash));
        util::emit_ok(ctx, &Value::Object(body));
        return Ok(());
    }
    let (doc, doc_path) = util::load_doc(&args.input)?;
    let _ = path;
    let palette = util::doc_palette(&doc, &doc_path)?;
    let (frames, delays, looped) = if let Some(anim_name) = &args.anim {
        let anim = doc.anims.get(anim_name).ok_or_else(|| {
            PxlError::semantic(format!("unknown anim '{anim_name}'"))
                .with_hint("pxl inspect lists animations")
        })?;
        let names = anim.frames.sequence(args.dir.as_deref())?;
        let mut frames = Vec::new();
        for name in names {
            frames.push(doc.frame(&name)?);
        }
        let delays = gif_delays_cs(anim.fps, frames.len());
        (frames, delays, anim.looped)
    } else {
        let name = args.frame.unwrap_or_else(|| {
            if doc.frames.contains_key("base") {
                "base".into()
            } else {
                doc.frame_names()
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "base".into())
            }
        });
        (vec![doc.frame(&name)?], vec![10u16], true)
    };
    let gif = args.out.ends_with(".gif");
    let (bytes, png_w, png_h) = if gif {
        let bytes = encode_gif_timed(&frames, &palette, args.scale, &delays, looped)?;
        (
            bytes,
            frames[0].w as u32 * args.scale,
            frames[0].h as u32 * args.scale,
        )
    } else if frames.len() == 1 {
        let bytes = render_preview_png(&frames[0], &palette, args.scale, args.grid)?;
        (
            bytes,
            frames[0].w as u32 * args.scale,
            frames[0].h as u32 * args.scale,
        )
    } else {
        // A horizontal strip so a multi-frame preview still has a PNG.
        let mut rows = Vec::new();
        let mut row = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            row.push(SheetCell {
                name: format!("f{i}"),
                frame: frame.clone(),
                duration_ms: 100,
                tag: None,
            });
        }
        rows.push(row);
        let packed = pack_sheet(&rows, "rows-by-anim", 1)?;
        let bytes = render_preview_png(&packed.image, &palette, args.scale, args.grid)?;
        (
            bytes,
            packed.image.w as u32 * args.scale,
            packed.image.h as u32 * args.scale,
        )
    };
    let hash = util::hash_of(&bytes);
    util::write_out(&args.out, &bytes, ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("render");
    body.insert("out".into(), json!(args.out));
    body.insert("png".into(), json!(args.out));
    body.insert("png_w".into(), json!(png_w));
    body.insert("png_h".into(), json!(png_h));
    body.insert("frames_count".into(), json!(frames.len()));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn looks_like_png(path: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"\x89PNG")
}
