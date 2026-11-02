//! `pxl ascii import|render`: text grid to spec, and one-step rendering.
//! Compilation shared with `gen ascii` and recipes lives in [`compile`].

mod compile;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pxl_core::PxlError;
use pxl_engine::file_name;
use pxl_format::{
    format_ascii_text, parse_ascii_text, refine_text, Anchor, AsciiFrame, AsciiSpec,
    AsciiTextOptions, BlockKind, Charset, LegendValue, RefineMode,
};
use serde_json::{json, Value};

use super::args::{AsciiCmd, AsciiImportArgs, AsciiRefineArgs, AsciiRenderArgs};
use super::render::{self, RenderTarget};
use crate::error::CmdResult;
use crate::util::{self, Ctx};

use compile::{build_parts, write_doc_and_palette};
pub(super) use compile::{frames_for_recipe, gen_ascii, RecipeAscii};

pub(super) fn ascii_cmd(ctx: &Ctx, action: AsciiCmd) -> CmdResult<()> {
    match action {
        AsciiCmd::Import(args) => import_cmd(ctx, args),
        AsciiCmd::Render(args) => render_cmd(ctx, args),
        AsciiCmd::Refine(args) => refine_cmd(ctx, args),
    }
}

struct ImportOptions {
    charset: Option<Charset>,
    size: u16,
    cell: Option<String>,
    anchor: Anchor,
    palette: Option<String>,
    legend: Option<String>,
    blocks: BlockKind,
    pad: bool,
    directives: bool,
    kind: Option<String>,
    size_explicit: bool,
}

impl ImportOptions {
    fn from_import(args: &AsciiImportArgs) -> CmdResult<Self> {
        Ok(Self {
            charset: optional_charset(args.charset.as_deref())?,
            size: args.size.unwrap_or(32),
            cell: args.cell.clone(),
            anchor: optional_anchor(args.anchor.as_deref())?,
            palette: args.palette.clone(),
            legend: args.legend.clone(),
            blocks: BlockKind::parse(&args.blocks)?,
            pad: args.pad,
            directives: !args.no_directives,
            kind: args.kind.clone(),
            size_explicit: args.size.is_some(),
        })
    }

    fn from_render(args: &AsciiRenderArgs) -> CmdResult<Self> {
        Ok(Self {
            charset: optional_charset(args.charset.as_deref())?,
            size: args.size.unwrap_or(32),
            cell: args.cell.clone(),
            anchor: optional_anchor(args.anchor.as_deref())?,
            palette: args.palette.clone(),
            legend: args.legend.clone(),
            blocks: BlockKind::parse(&args.blocks)?,
            pad: args.pad,
            directives: !args.no_directives,
            kind: args.kind.clone(),
            size_explicit: args.size.is_some(),
        })
    }
}

fn optional_charset(value: Option<&str>) -> CmdResult<Option<Charset>> {
    Ok(match value {
        Some(text) => Some(Charset::parse(text)?),
        None => None,
    })
}

fn optional_anchor(value: Option<&str>) -> CmdResult<Anchor> {
    Ok(match value {
        Some(text) => Anchor::parse(text)?,
        None => Anchor::default(),
    })
}

fn import_cmd(ctx: &Ctx, args: AsciiImportArgs) -> CmdResult<()> {
    let opts = ImportOptions::from_import(&args)?;
    let (spec, text_warnings) = build_spec(&opts, &args.input, &parent_dir(&args.out))?;
    util::guard_warnings(ctx, &text_warnings)?;
    let text = pxl_format::to_pretty(&spec)?;
    let hash = util::hash_of(text.as_bytes());
    util::write_out(&args.out, text.as_bytes(), ctx.overwrite, ctx.json)?;
    let (layout_w, layout_h) = layout_extent(&spec);
    let mut body = util::ok_base("ascii");
    body.insert("warnings".into(), json!(text_warnings));
    body.insert("out".into(), json!(args.out));
    body.insert("size".into(), json!(spec.size));
    body.insert("cell".into(), json!(spec.cell));
    body.insert("anchor".into(), json!(spec.anchor));
    body.insert(
        "frames".into(),
        json!(spec.frames.keys().collect::<Vec<_>>()),
    );
    body.insert(
        "layers".into(),
        json!(spec.layers.iter().map(|l| &l.name).collect::<Vec<_>>()),
    );
    body.insert("chars".into(), json!(used_chars(&spec)));
    body.insert("auto".into(), json!(auto_chars(&spec)));
    body.insert(
        "layout_px".into(),
        json!([layout_w * spec.cell_w(), layout_h * spec.cell_h()]),
    );
    body.insert(
        "blocks".into(),
        json!(spec.source.as_ref().map(|s| s.blocks.len())),
    );
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn refine_cmd(ctx: &Ctx, args: AsciiRefineArgs) -> CmdResult<()> {
    let charset = match args.charset.as_deref() {
        Some(text) => Charset::parse(text)?,
        None => Charset::default(),
    };
    let mode = RefineMode::parse(&args.mode)?;
    let source = util::read_text(&args.input)?;
    let mut opts = AsciiTextOptions::new(charset, BlockKind::Frames);
    opts.directives = !args.no_directives;
    opts.file = Some(args.input.clone());
    let text = parse_ascii_text(&source, &opts)?;
    let refined = refine_text(&text, charset, mode)?;
    let out_text = format_ascii_text(&refined);
    let hash = util::hash_of(out_text.as_bytes());
    util::write_out(&args.out, out_text.as_bytes(), ctx.overwrite, ctx.json)?;
    let mut body = util::ok_base("ascii_refine");
    body.insert("out".into(), json!(args.out));
    body.insert("mode".into(), json!(mode.name()));
    body.insert("charset".into(), json!(charset.name()));
    body.insert("in_grid".into(), json!([text.width, text.height]));
    body.insert("out_grid".into(), json!([refined.width, refined.height]));
    body.insert(
        "blocks".into(),
        json!(refined
            .blocks
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()),
    );
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn render_cmd(ctx: &Ctx, args: AsciiRenderArgs) -> CmdResult<()> {
    render::check_scale(args.scale)?;
    if args.input.ends_with(".txt") && args.anim.is_some() {
        return Err(PxlError::semantic(
            "ascii.anim_source: the text grid declares no animations",
        )
        .with_hint(
            "run `pxl ascii import`, add anims to the *.ascii.json, then render with --anim",
        )
        .into());
    }
    let (spec, spec_dir, text_warnings) = if args.input.ends_with(".txt") {
        let opts = ImportOptions::from_render(&args)?;
        let (spec, warnings) = build_spec(&opts, &args.input, &parent_dir(&args.input))?;
        (spec, parent_dir(&args.input), warnings)
    } else {
        let text = util::read_text(&args.input)?;
        let spec = pxl_format::parse_ascii_spec(&text)?;
        (spec, parent_dir(&args.input), Vec::new())
    };
    let seed = ctx.seed.unwrap_or(1);
    let (mut doc, palette, palette_file, mut warnings) =
        build_parts(&spec, &spec_dir, args.palette.as_deref(), seed, true)?;
    warnings.splice(0..0, text_warnings);
    doc.meta.ascii = Some(file_name(&args.input));
    if let Some(path) = &args.emit_json {
        let text = pxl_format::to_pretty(&spec)?;
        util::write_out(path, text.as_bytes(), ctx.overwrite, ctx.json)?;
    }
    if let Some(path) = &args.emit_doc {
        write_doc_and_palette(ctx, &mut doc, palette_file, path)?;
        util::save_doc(&doc, path, ctx.overwrite, ctx.json)?;
    }
    util::guard_warnings(ctx, &warnings)?;
    let out = args.out;
    render::render_doc(
        ctx,
        &doc,
        &palette,
        RenderTarget {
            frame: None,
            anim: args.anim.as_deref(),
            dir: args.dir.as_deref(),
            scale: args.scale,
            grid: args.grid,
            purpose: pxl_engine::OutputPurpose::Preview,
            out: &out,
        },
    )
}

/// Read a text grid and turn it into an `AsciiSpec`, filling colours the text
/// does not carry. No filesystem output happens here. `spec_dir` is where the
/// spec will live, so `legend_ref` can be stored relative to it.
fn build_spec(
    opts: &ImportOptions,
    input: &str,
    spec_dir: &Path,
) -> CmdResult<(AsciiSpec, Vec<String>)> {
    let source = util::read_text(input)?;
    let input_dir = parent_dir(input);
    let legend_ref = opts
        .legend
        .as_deref()
        .map(|name| relative_to(spec_dir, name, &input_dir));
    let engine_opts = pxl_engine::AsciiImportOptions {
        charset: opts.charset,
        size: opts.size,
        cell: opts.cell.clone(),
        anchor: opts.anchor,
        palette: opts.palette.clone(),
        legend: opts.legend.clone(),
        legend_ref,
        blocks: opts.blocks,
        pad: opts.pad,
        directives: opts.directives,
        kind: opts.kind.clone(),
        size_explicit: opts.size_explicit,
    };
    Ok(pxl_engine::build_spec(
        &engine_opts,
        &source,
        input,
        &util::DirResolver::new(&input_dir),
    )?)
}

fn used_chars(spec: &AsciiSpec) -> String {
    let mut chars = BTreeSet::new();
    for frame in spec.frames.values() {
        if let AsciiFrame::Rows { rows } = frame {
            for row in rows {
                chars.extend(row.chars());
            }
        }
    }
    for layer in &spec.layers {
        for row in &layer.rows {
            chars.extend(row.chars());
        }
    }
    chars.into_iter().collect()
}

fn auto_chars(spec: &AsciiSpec) -> String {
    spec.legend
        .iter()
        .filter(|(_, value)| matches!(value, LegendValue::Entry(entry) if entry.auto))
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>()
        .join("")
}

fn layout_extent(spec: &AsciiSpec) -> (u16, u16) {
    let mut width = 0usize;
    let mut height = 0usize;
    for frame in spec.frames.values() {
        if let AsciiFrame::Rows { rows } = frame {
            width = width.max(rows.first().map(|r| r.chars().count()).unwrap_or(0));
            height = height.max(rows.len());
        }
    }
    for layer in &spec.layers {
        width = width.max(layer.rows.first().map(|r| r.chars().count()).unwrap_or(0));
        height = height.max(layer.rows.len());
    }
    (width as u16, height as u16)
}

/// Store a shared-legend path relative to the spec directory when both live in
/// the same tree, so the spec keeps working from any working directory.
fn relative_to(spec_dir: &Path, name: &str, input_dir: &Path) -> String {
    let target = if input_dir.join(name).is_file() {
        input_dir.join(name)
    } else {
        PathBuf::from(name)
    };
    let (Ok(base), Ok(target)) = (spec_dir.canonicalize(), target.canonicalize()) else {
        return name.to_string();
    };
    relative_path(&base, &target)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string())
}

fn relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    let base_parts: Vec<_> = base.components().collect();
    let target_parts: Vec<_> = target.components().collect();
    let common = base_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = PathBuf::new();
    for _ in common..base_parts.len() {
        out.push("..");
    }
    for part in &target_parts[common..] {
        out.push(part.as_os_str());
    }
    Some(out)
}

pub(super) fn parent_dir(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
