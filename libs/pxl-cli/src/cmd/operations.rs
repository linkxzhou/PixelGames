use std::path::Path;

use pxl_core::PxlError;
use serde_json::{json, Value};

use super::args::{GenArgs, OpArgs, TileCmd};
use super::document::{apply_with, collect_ops, load_parts};
use super::generate::gen;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn op_cmd(ctx: &Ctx, args: OpArgs) -> CmdResult<()> {
    let (mut doc, doc_path) = util::load_doc(&args.input)?;
    util::check_lock(ctx, doc.meta.pxl_version.as_deref())?;
    let palette = util::doc_palette(&doc, &doc_path)?;
    let base = doc_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let source_name = args.frame.unwrap_or_else(|| "base".into());
    let source = doc.frame(&source_name)?;
    let ops = collect_ops(args.ops.as_deref(), &args.op)?;
    let dest_name = args.as_frame.unwrap_or_else(|| source_name.clone());
    let seed = ctx.seed.or(doc.meta.seed).unwrap_or(1);
    let parts = load_parts(&doc, &base)?;
    let frame = apply_with(
        &source,
        &ops,
        &palette,
        seed,
        &dest_name,
        &base,
        parts.as_ref(),
    )?;
    doc.insert_frame(&dest_name, &frame)?;
    let hash = util::save_doc(&doc, &args.out, ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("op");
    body.insert("out".into(), json!(args.out));
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert("colors_used".into(), json!(frame.colors_used()));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn tile_cmd(ctx: &Ctx, action: TileCmd) -> CmdResult<()> {
    match action {
        TileCmd::Autotile => Err(PxlError::usage("tile autotile is not implemented")
            .with_hint("autotile is M3. `pxl tile gen` builds seamless dirt, grass, and sand")
            .into()),
        TileCmd::Gen {
            kind,
            size,
            palette,
            ramp,
            variants,
            out,
        } => gen(
            ctx,
            GenArgs {
                generator: "noise-tile".into(),
                template: None,
                spec: None,
                palette: Some(palette),
                ramp,
                mirror: None,
                dir: None,
                kind: Some(kind),
                size: Some(size),
                variants,
                out,
            },
        ),
    }
}
