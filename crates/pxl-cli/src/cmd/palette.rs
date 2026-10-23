use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use pxl_format::Op;
use serde_json::{json, Value};

use super::args::PaletteCmd;
use super::catalog::list_cmd;
use super::document::apply_with;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn palette_cmd(ctx: &Ctx, action: PaletteCmd) -> CmdResult<()> {
    match action {
        PaletteCmd::List => list_cmd(ctx, "palettes"),
        PaletteCmd::Show { palette } => {
            let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let pal = util::load_palette(&palette, &base)?;
            let mut body = util::ok_base("palette");
            body.insert("id".into(), json!(pal.id));
            body.insert("colors".into(), json!(pal.colors.len()));
            body.insert(
                "ramps".into(),
                json!(pal.ramps.keys().cloned().collect::<Vec<_>>()),
            );
            body.insert(
                "cycles".into(),
                json!(pal.cycles.keys().cloned().collect::<Vec<_>>()),
            );
            body.insert(
                "special".into(),
                json!({
                    "transparent": pal.special.transparent,
                    "outline": pal.special.outline,
                    "flash": pal.special.flash,
                }),
            );
            util::emit_ok(ctx, &Value::Object(body));
            Ok(())
        }
        PaletteCmd::Apply {
            input,
            palette,
            out,
        } => {
            let (mut doc, _) = util::load_doc(&input)?;
            let base = Path::new(&input).parent().unwrap_or_else(|| Path::new("."));
            let pal = util::load_palette(&palette, base)?;
            for name in doc.frame_names() {
                let frame = doc.frame(&name)?;
                if frame.pixels.iter().any(|p| *p as usize >= pal.len()) {
                    return Err(PxlError::semantic(format!(
                        "frame '{name}' uses an index the new palette does not have"
                    ))
                    .into());
                }
            }
            doc.palette = palette;
            let hash = util::save_doc(&doc, &out, ctx.overwrite, ctx.json)?;
            let mut body = util::ok_base("palette");
            body.insert("out".into(), json!(out));
            body.insert("hash".into(), json!(hash));
            util::emit_ok(ctx, &Value::Object(body));
            Ok(())
        }
        PaletteCmd::Swap {
            input,
            map,
            frame,
            out,
        } => apply_palette_op(
            ctx,
            &input,
            &out,
            frame.as_deref(),
            Op::PaletteSwap { map, region: None },
        ),
        PaletteCmd::Cycle {
            input,
            cycle,
            step,
            frame,
            out,
        } => apply_palette_op(
            ctx,
            &input,
            &out,
            frame.as_deref(),
            Op::PaletteCycle {
                cycle,
                step,
                region: None,
            },
        ),
    }
}

pub(super) fn apply_palette_op(
    ctx: &Ctx,
    input: &str,
    out: &str,
    only: Option<&str>,
    op: Op,
) -> CmdResult<()> {
    let (mut doc, doc_path) = util::load_doc(input)?;
    let palette = util::doc_palette(&doc, &doc_path)?;
    let base = doc_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let names: Vec<String> = if let Some(name) = only {
        vec![name.to_string()]
    } else {
        doc.frame_names()
    };
    let seed = ctx.seed.or(doc.meta.seed).unwrap_or(1);
    for name in names {
        let frame = doc.frame(&name)?;
        let next = apply_with(&frame, &[op.clone()], &palette, seed, &name, &base, None)?;
        doc.insert_frame(&name, &next)?;
    }
    let hash = util::save_doc(&doc, out, ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("palette");
    body.insert("out".into(), json!(out));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}
