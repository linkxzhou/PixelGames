//! `pxl svg render|lint|matrix`: SVG in, palette-indexed frames out.

use std::path::Path;

use pxl_core::{PxlError, SizeProfile, SizeTier};
use pxl_engine::load_palette;
use pxl_export::render_preview_png;
use pxl_svg::{ColorMode, SvgOptions};
use serde_json::{json, Value};

use super::args::{SvgCmd, SvgLintArgs, SvgMatrixArgs, SvgRenderArgs};
use crate::error::CmdResult;
use crate::util::{self, Ctx, DirResolver};

pub(super) fn svg_cmd(ctx: &Ctx, action: SvgCmd) -> CmdResult<()> {
    match action {
        SvgCmd::Render(a) => svg_render(ctx, a),
        SvgCmd::Lint(a) => svg_lint(ctx, a),
        SvgCmd::Matrix(a) => svg_matrix(ctx, a),
    }
}

pub(super) fn svg_render(ctx: &Ctx, args: SvgRenderArgs) -> CmdResult<()> {
    let sizes: Vec<u16> = match &args.sizes {
        Some(list) => list
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                SizeTier::parse(s)
                    .map(SizeTier::side)
                    .map_err(|e| -> crate::error::CmdError { e.into() })
            })
            .collect::<CmdResult<Vec<u16>>>()?,
        None => Vec::new(),
    };
    let targets: Vec<u16> = if sizes.is_empty() {
        vec![args.size]
    } else {
        let mut all = vec![args.size];
        for s in sizes {
            if !all.contains(&s) {
                all.push(s);
            }
        }
        all
    };
    let base = parent_dir(&args.input);
    let palette_ref = args.palette.clone().unwrap_or_else(|| "p16".into());
    let palette = load_palette(&palette_ref, &DirResolver::new(&base))?;
    let color_mode = ColorMode::parse(&args.color_mode).ok_or_else(|| {
        PxlError::usage(format!("unknown color mode '{}'", args.color_mode))
            .with_hint("color_mode is strict, nearest, or extend")
    })?;
    let text = util::read_text(&args.input)?;
    let mut outputs = Vec::new();
    let mut main_doc = None;
    let mut warnings = Vec::new();
    for (i, size) in targets.iter().enumerate() {
        let profile = SizeProfile::for_size(*size, *size)?;
        let opts = SvgOptions {
            w: *size,
            h: *size,
            samples: args.samples,
            color_mode,
            gradient_bands: args.gradient_bands,
            strict: args.strict,
            max_colors: profile.max_colors,
            view_box: None,
            snap: true,
            finish: if args.no_finish {
                None
            } else {
                Some(pxl_art::FinishOptions::for_profile(&profile))
            },
        };
        let out = pxl_svg::rasterize(&text, &opts, &palette)?;
        warnings.extend(out.report.warnings());
        let first = out
            .frames
            .first()
            .map(|(_, f)| f.frame.clone())
            .ok_or_else(|| PxlError::semantic("the SVG drew no frames"))?;
        let path = if i == 0 {
            args.out.clone()
        } else {
            sized_name(&args.out, *size)
        };
        let png = if args.asset {
            pxl_export::render_asset_png(&first, &out.palette, args.scale)?
        } else {
            render_preview_png(&first, &out.palette, args.scale, false)?
        };
        util::write_out(&path, &png, ctx.overwrite, ctx.json)?;
        outputs.push(json!({"out": path, "size": size, "hash": util::hash_of(&png)}));
        if i == 0 {
            main_doc = Some(out);
        }
    }
    util::guard_warnings(ctx, &warnings)?;
    let out = main_doc.expect("at least one size");
    if let Some(path) = &args.doc {
        let doc = doc_of(&out, &palette_ref);
        util::save_doc(&doc, path, ctx.overwrite, ctx.json)?;
    }
    let mut body = util::ok_base("svg");
    body.insert("out".into(), json!(args.out));
    body.insert("palette".into(), json!(palette_ref));
    body.insert(
        "profile".into(),
        json!(SizeProfile::for_size(args.size, args.size)?.tier.name()),
    );
    body.insert("palette_colors".into(), json!(out.palette.colors.len()));
    body.insert(
        "frames".into(),
        json!(out.frames.iter().map(|(n, _)| n).collect::<Vec<_>>()),
    );
    body.insert("variants".into(), Value::Array(outputs));
    body.insert("samples".into(), json!(out.report.samples));
    body.insert("shapes".into(), json!(out.report.shapes));
    body.insert("lost".into(), json!(out.report.lost));
    body.insert("forced".into(), json!(out.report.forced));
    body.insert(
        "colors".into(),
        json!({
            "exact": out.report.colors.exact,
            "snapped": out.report.colors.snapped,
            "appended": out.report.colors.appended,
            "entries": out.report.colors.entries,
        }),
    );
    body.insert("finish".into(), json!(out.report.finish));
    body.insert("warnings".into(), json!(warnings));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn svg_lint(ctx: &Ctx, args: SvgLintArgs) -> CmdResult<()> {
    let base = parent_dir(&args.input);
    let palette_ref = args.palette.clone().unwrap_or_else(|| "p16".into());
    let palette = load_palette(&palette_ref, &DirResolver::new(&base))?;
    let color_mode = ColorMode::parse(&args.color_mode)
        .ok_or_else(|| PxlError::usage(format!("unknown color mode '{}'", args.color_mode)))?;
    let text = util::read_text(&args.input)?;
    let profile = SizeProfile::for_size(args.size, args.size)?;
    let opts = SvgOptions {
        w: args.size,
        h: args.size,
        color_mode,
        max_colors: profile.max_colors,
        finish: None,
        ..Default::default()
    };
    let out = pxl_svg::rasterize(&text, &opts, &palette)?;
    let warnings = out.report.warnings();
    util::guard_warnings(ctx, &warnings)?;
    let mut body = util::ok_base("svg_lint");
    body.insert("size".into(), json!(args.size));
    body.insert("samples".into(), json!(out.report.samples));
    body.insert("shapes".into(), json!(out.report.shapes));
    body.insert("frames".into(), json!(out.report.frames));
    body.insert("unsupported".into(), json!(out.report.unsupported));
    body.insert("lost".into(), json!(out.report.lost));
    body.insert("forced".into(), json!(out.report.forced));
    body.insert("size_filtered".into(), json!(out.report.size_filtered));
    body.insert("outline_role".into(), json!(out.report.outline_role));
    body.insert(
        "colors".into(),
        json!({
            "exact": out.report.colors.exact,
            "snapped": out.report.colors.snapped,
            "appended": out.report.colors.appended,
            "entries": out.report.colors.entries,
        }),
    );
    body.insert("warnings".into(), json!(warnings));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) fn svg_matrix(ctx: &Ctx, args: SvgMatrixArgs) -> CmdResult<()> {
    let base = parent_dir(&args.input);
    let palette_ref = args.palette.clone().unwrap_or_else(|| "p16".into());
    let palette = load_palette(&palette_ref, &DirResolver::new(&base))?;
    let color_mode = ColorMode::parse(&args.color_mode)
        .ok_or_else(|| PxlError::usage(format!("unknown color mode '{}'", args.color_mode)))?;
    let text = util::read_text(&args.input)?;
    let profile = SizeProfile::for_size(args.size, args.size)?;
    let opts = SvgOptions {
        w: args.size,
        h: args.size,
        color_mode,
        max_colors: profile.max_colors,
        finish: None,
        ..Default::default()
    };
    let out = pxl_svg::rasterize(&text, &opts, &palette)?;
    // One layout per frame, RLE-encoded past 64 so a 128 or 256 row stays
    // short enough to read.
    let mut blocks = Vec::new();
    for (name, f) in &out.frames {
        blocks.push((name.clone(), pxl_core::encode_matrix(&f.frame)?));
    }
    let encoding = if args.size > 64 { "rle" } else { "rows" };
    let mut doc = String::new();
    doc.push_str(&format!("::size {}\n", args.size));
    doc.push_str(&format!("::encoding {}\n", encoding));
    doc.push_str(&format!("::palette {}\n", palette_ref));
    for (name, rows) in &blocks {
        doc.push_str(&format!("::name {}\n", name));
        for row in rows {
            doc.push_str(&row_replace(row, encoding));
            doc.push('\n');
        }
        doc.push('\n');
    }
    util::write_out(&args.out, doc.as_bytes(), ctx.overwrite, ctx.json)?;
    util::guard_warnings(ctx, &out.report.warnings())?;
    let mut body = util::ok_base("svg_matrix");
    body.insert("out".into(), json!(args.out));
    body.insert("size".into(), json!(args.size));
    body.insert("encoding".into(), json!(encoding));
    body.insert(
        "frames".into(),
        json!(blocks.iter().map(|(n, _)| n).collect::<Vec<_>>()),
    );
    body.insert("palette".into(), json!(palette_ref));
    body.insert("warnings".into(), json!(out.report.warnings()));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

/// RLE a matrix row: `....` becomes `4.`, `.3` stays escaped as `'3`.
pub fn row_replace(row: &str, encoding: &str) -> String {
    if encoding != "rle" {
        return row.to_string();
    }
    let mut out = String::new();
    let chars: Vec<char> = row.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        if n > 1 {
            out.push_str(&n.to_string());
        }
        if c.is_ascii_digit() {
            out.push('\'');
        }
        out.push(c);
        i += n;
    }
    out
}

fn doc_of(out: &pxl_svg::SvgOut, palette_ref: &str) -> pxl_format::PxlDoc {
    let mut doc = pxl_format::PxlDoc {
        v: 1,
        palette: palette_ref.to_string(),
        w: out.frames[0].1.frame.w,
        h: out.frames[0].1.frame.h,
        frames: Default::default(),
        anims: Default::default(),
        meta: Default::default(),
    };
    for (name, f) in &out.frames {
        let _ = doc.insert_frame(name, &f.frame);
    }
    doc
}

fn sized_name(path: &str, size: u16) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("png");
    let dir = p.parent().unwrap_or_else(|| Path::new(""));
    dir.join(format!("{stem}@{size}.{ext}"))
        .display()
        .to_string()
}

fn parent_dir(input: &str) -> std::path::PathBuf {
    if input == "-" {
        std::env::current_dir().unwrap_or_default()
    } else {
        Path::new(input)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_run_length_encoded() {
        assert_eq!(row_replace("....aabb", "rows"), "....aabb");
        assert_eq!(row_replace("....aabb", "rle"), "4.2a2b");
        assert_eq!(row_replace("...333", "rle"), "3.3'3");
        assert_eq!(row_replace("abc", "rle"), "abc");
    }

    #[test]
    fn variants_are_named_beside_the_output() {
        assert_eq!(sized_name("out/knight.png", 32), "out/knight@32.png");
        assert_eq!(sized_name("knight.png", 256), "knight@256.png");
    }
}
