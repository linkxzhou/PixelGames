//! `pxl shapes refine`: render a shapes spec at its native size, double it,
//! and write a new shapes spec whose only command is the doubled pixel matrix.
//! The result is a starting point for hand edits on the finer grid.

use std::path::Path;

use pxl_core::encode_matrix;
use pxl_format::{DrawCmd, RefineMode, ShapesFile};
use pxl_gen::generate_shapes;
use serde_json::{json, Value};

use super::args::{ShapesCmd, ShapesRefineArgs};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn shapes_cmd(ctx: &Ctx, action: ShapesCmd) -> CmdResult<()> {
    match action {
        ShapesCmd::Refine(args) => refine_cmd(ctx, args),
    }
}

fn refine_cmd(ctx: &Ctx, args: ShapesRefineArgs) -> CmdResult<()> {
    let mode = RefineMode::parse(&args.mode)?;
    let text = util::read_text(&args.input)?;
    let spec: ShapesFile = pxl_format::parse_shapes(&text)?;
    let palette_ref = spec.palette.clone().ok_or_else(|| {
        pxl_core::PxlError::usage("shapes refine needs the spec to name a palette")
            .with_hint("add \"palette\": \"<file>.palette.json\" to the shapes spec")
    })?;
    let base = Path::new(&args.input)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let palette = util::load_palette(&palette_ref, base)?;
    let frame = generate_shapes(&spec, &palette)?;
    let refined = match mode {
        RefineMode::Copy => pxl_core::double_copy(&frame)?,
        RefineMode::Corners => pxl_core::epx2x(&frame)?,
    };
    let out_spec = ShapesFile {
        w: refined.w,
        h: refined.h,
        palette: Some(palette_ref),
        mirror: None,
        draw: vec![DrawCmd::Pixels {
            pixels: encode_matrix(&refined)?,
            x: 0,
            y: 0,
            mirror: None,
        }],
        regions: Default::default(),
    };
    let out_text = pxl_format::to_pretty(&out_spec)?;
    let hash = util::hash_of(out_text.as_bytes());
    util::write_out(&args.out, out_text.as_bytes(), ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("shapes_refine");
    body.insert("out".into(), json!(args.out));
    body.insert("mode".into(), json!(mode.name()));
    body.insert("in_size".into(), json!([frame.w, frame.h]));
    body.insert("out_size".into(), json!([refined.w, refined.h]));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}
