use std::path::{Path, PathBuf};

use pxl_art::palette::{ramp_palette, RampOptions};
use pxl_core::{Palette, PxlError};
use pxl_format::{CycleFile, Op, PaletteFile, SpecialFile};
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
        PaletteCmd::Ramp {
            palette,
            levels,
            hue_shift,
            max,
            out,
        } => ramp_cmd(ctx, &palette, levels, hue_shift, max, &out),
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

fn ramp_cmd(
    ctx: &Ctx,
    palette: &str,
    levels: u8,
    hue_shift: u8,
    max: usize,
    out: &str,
) -> CmdResult<()> {
    if !(2..=4).contains(&levels) {
        return Err(PxlError::usage(format!("--levels {levels} is outside 2..=4")).into());
    }
    if hue_shift > 100 {
        return Err(PxlError::usage(format!("--hue-shift {hue_shift} is outside 0..=100")).into());
    }
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let source = util::load_palette(palette, &base)?;
    if max < source.colors.len() {
        return Err(PxlError::usage(format!(
            "--max {max} is below the palette's own {} colours",
            source.colors.len()
        ))
        .with_hint("raise --max; ramp only appends colours, it never removes any")
        .into());
    }
    let (ramped, _map, report) = ramp_palette(
        &source,
        RampOptions {
            levels,
            hue_shift,
            max_colors: max.min(256),
            generate: true,
        },
    );
    ramped.validate()?;
    let id = util::stem_id(Path::new(out))
        .trim_end_matches(".palette")
        .to_string();
    let text = pxl_format::to_pretty(&palette_file(&ramped, &id))?;
    let hash = util::hash_of(text.as_bytes());
    util::write_out(out, text.as_bytes(), ctx.overwrite, ctx.json)?;

    let mut warnings = Vec::new();
    if !report.skipped.is_empty() {
        warnings.push(format!(
            "palette cap {max} reached: {} colour(s) have no generated ramp",
            report.skipped.len()
        ));
    }
    util::guard_warnings(ctx, &warnings)?;
    let mut body = util::ok_base("palette_ramp");
    body.insert("out".into(), json!(out));
    body.insert("hash".into(), json!(hash));
    body.insert("colors_before".into(), json!(source.colors.len()));
    body.insert("colors_after".into(), json!(ramped.colors.len()));
    body.insert("appended".into(), json!(report.appended));
    body.insert("generated".into(), json!(report.generated));
    body.insert("declared".into(), json!(report.declared));
    body.insert("skipped".into(), json!(report.skipped));
    body.insert("ramps".into(), json!(ramped.ramps));
    body.insert("warnings".into(), json!(warnings));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn palette_file(p: &Palette, id: &str) -> PaletteFile {
    PaletteFile {
        v: 1,
        id: id.to_string(),
        colors: p.colors.iter().map(|c| c.to_hex()).collect(),
        ramps: p.ramps.clone(),
        cycles: p
            .cycles
            .iter()
            .map(|(name, c)| {
                (
                    name.clone(),
                    CycleFile {
                        indices: c.indices.clone(),
                        period: c.period,
                    },
                )
            })
            .collect(),
        special: SpecialFile {
            transparent: p.special.transparent,
            outline: p.special.outline,
            flash: p.special.flash,
        },
    }
}
