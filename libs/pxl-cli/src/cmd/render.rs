use std::fs;
use std::path::Path;

use pxl_core::{Palette, PxlError};
use pxl_engine::{render_doc_bytes_as, FrameSelection, OutputFormat, OutputPurpose};
use pxl_export::{decode_png_rgba, encode_rgba_png, scale_rgba_checked};
use pxl_format::PxlDoc;
use serde_json::{json, Value};

use super::args::RenderArgs;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

/// One frame or animation to encode. Shared by `render` and `ascii render`.
pub(super) struct RenderTarget<'a> {
    pub frame: Option<&'a str>,
    pub anim: Option<&'a str>,
    pub dir: Option<&'a str>,
    pub scale: u32,
    pub grid: bool,
    pub purpose: OutputPurpose,
    pub out: &'a str,
}

pub(super) fn render_cmd(ctx: &Ctx, args: RenderArgs) -> CmdResult<()> {
    check_scale(args.scale)?;
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
        body.insert("native_w".into(), json!(w));
        body.insert("native_h".into(), json!(h));
        body.insert("scale".into(), json!(args.scale));
        body.insert("sampling".into(), json!("nearest"));
        body.insert("grid".into(), json!(args.grid && args.scale >= 2));
        body.insert("purpose".into(), json!("preview"));
        body.insert("frames_count".into(), json!(1));
        body.insert("hash".into(), json!(hash));
        util::emit_ok(ctx, &Value::Object(body));
        return Ok(());
    }
    let (doc, doc_path) = util::load_doc(&args.input)?;
    let _ = path;
    let palette = util::doc_palette(&doc, &doc_path)?;
    render_doc(
        ctx,
        &doc,
        &palette,
        RenderTarget {
            frame: args.frame.as_deref(),
            anim: args.anim.as_deref(),
            dir: args.dir.as_deref(),
            scale: args.scale,
            grid: args.grid,
            purpose: if args.asset {
                OutputPurpose::Asset
            } else {
                OutputPurpose::Preview
            },
            out: &args.out,
        },
    )
}

pub(super) fn check_scale(scale: u32) -> CmdResult<()> {
    Ok(pxl_engine::check_scale(scale)?)
}

pub(super) fn render_doc(
    ctx: &Ctx,
    doc: &PxlDoc,
    palette: &Palette,
    target: RenderTarget<'_>,
) -> CmdResult<()> {
    let format = if target.out.ends_with(".gif") {
        OutputFormat::Gif
    } else {
        OutputFormat::Png
    };
    let rendered = render_doc_bytes_as(
        doc,
        palette,
        FrameSelection {
            frame: target.frame,
            anim: target.anim,
            dir: target.dir,
        },
        target.scale,
        target.grid,
        format,
        target.purpose,
    )?;
    let hash = util::hash_of(&rendered.bytes);
    util::write_out(target.out, &rendered.bytes, ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("render");
    body.insert("out".into(), json!(target.out));
    body.insert("png".into(), json!(target.out));
    body.insert("png_w".into(), json!(rendered.width));
    body.insert("png_h".into(), json!(rendered.height));
    body.insert("native_w".into(), json!(rendered.native_width));
    body.insert("native_h".into(), json!(rendered.native_height));
    body.insert("scale".into(), json!(rendered.scale));
    body.insert("sampling".into(), json!("nearest"));
    body.insert("grid".into(), json!(rendered.grid));
    body.insert("purpose".into(), json!(rendered.purpose.name()));
    body.insert("frames_count".into(), json!(rendered.frame_count));
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
