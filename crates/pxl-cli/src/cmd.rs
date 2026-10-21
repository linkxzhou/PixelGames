//! Subcommand implementations. File IO stays here; pixel work stays in the libraries.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use pxl_core::{IndexFrame, Mirror, Palette, PxlError, GEN_VERSION, PXL_VERSION};
use pxl_export::{
    decode_indexed, decode_png_rgba, encode_gif, encode_indexed_png, encode_rgba_png, pack_sheet,
    render_preview_png, scale_rgba, sheet_json, SheetCell,
};
use pxl_format::{
    schema_json, Anim, AnimFrames, Meta, Op, PaletteFile, PxlDoc, Recipe, ShapesFile,
};
use pxl_gen::{
    builtin_palette, builtin_template, generate_bollinger, generate_noise_tile, generate_shapes,
    palette_ids, template_ids, variant_seed, RuntimeMask,
};
use pxl_ops::{apply_ops, compose_rig, expand_macro, ApplyCx, MACRO_NAMES};
use pxl_qa::{validate, AnimSeq, Report, ValidateInput};
use serde_json::{json, Map, Value};

use crate::error::{CmdError, CmdResult};
use crate::util::{self, Ctx};

#[derive(Args, Debug)]
pub struct GenArgs {
    /// bollinger, shapes, noise-tile, or parts.
    pub generator: String,
    #[arg(long)]
    pub template: Option<String>,
    #[arg(long)]
    pub spec: Option<String>,
    #[arg(long)]
    pub palette: Option<String>,
    #[arg(long)]
    pub ramp: Option<String>,
    #[arg(long)]
    pub mirror: Option<String>,
    /// Facing to draw when `generator` is `parts`. Omit for a blank canvas.
    #[arg(long)]
    pub dir: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long)]
    pub size: Option<u16>,
    #[arg(long, default_value_t = 1)]
    pub variants: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct OpArgs {
    pub input: String,
    #[arg(long)]
    pub ops: Option<String>,
    #[arg(long = "op")]
    pub op: Vec<String>,
    #[arg(long)]
    pub frame: Option<String>,
    /// Store the result under this frame name instead of replacing `--frame`.
    #[arg(long = "as")]
    pub as_frame: Option<String>,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct AnimArgs {
    pub input: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub dirs: Option<String>,
    #[arg(long = "macro")]
    pub macro_call: Option<String>,
    #[arg(long)]
    pub frames: Option<String>,
    #[arg(long)]
    pub fps: Option<u32>,
    #[arg(long)]
    pub no_loop: bool,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Subcommand, Debug)]
pub enum PaletteCmd {
    List,
    Show {
        palette: String,
    },
    Apply {
        input: String,
        palette: String,
        #[arg(short, long)]
        out: String,
    },
    Swap {
        input: String,
        #[arg(long)]
        map: String,
        #[arg(long)]
        frame: Option<String>,
        #[arg(short, long)]
        out: String,
    },
    Cycle {
        input: String,
        #[arg(long)]
        cycle: String,
        #[arg(long, default_value_t = 1)]
        step: i32,
        #[arg(long)]
        frame: Option<String>,
        #[arg(short, long)]
        out: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TileCmd {
    Gen {
        #[arg(long)]
        kind: String,
        #[arg(long, default_value_t = 16)]
        size: u16,
        #[arg(long)]
        palette: String,
        #[arg(long)]
        ramp: Option<String>,
        #[arg(long, default_value_t = 4)]
        variants: u32,
        #[arg(short, long)]
        out: String,
    },
    Autotile,
}

#[derive(Args, Debug)]
pub struct SheetArgs {
    pub inputs: Vec<String>,
    #[arg(long, default_value = "rows-by-anim")]
    pub layout: String,
    #[arg(long, default_value_t = 1)]
    pub pad: u32,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct RenderArgs {
    pub input: String,
    #[arg(long)]
    pub frame: Option<String>,
    #[arg(long)]
    pub anim: Option<String>,
    #[arg(long)]
    pub dir: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub grid: bool,
    #[arg(short, long)]
    pub out: String,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    pub input: String,
    #[arg(long)]
    pub frame: Option<String>,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    pub input: String,
    #[arg(long, default_value = "default")]
    pub rules: String,
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    pub recipe: String,
    #[arg(short, long)]
    pub out: String,
}

pub fn dispatch(ctx: &Ctx, cmd: super::Cmd) -> CmdResult<()> {
    use super::Cmd::*;
    match cmd {
        Gen(a) => gen(ctx, a),
        Op(a) => op_cmd(ctx, a),
        Anim(a) => anim_cmd(ctx, a),
        Palette { action } => palette_cmd(ctx, action),
        Tile { action } => tile_cmd(ctx, action),
        Sheet(a) => sheet_cmd(ctx, a),
        Render(a) => render_cmd(ctx, a),
        Inspect(a) => inspect_cmd(ctx, a),
        Validate(a) => validate_cmd(ctx, a),
        Build(a) => build_cmd(ctx, a),
        Schema { kind } => schema_cmd(ctx, &kind),
        List { kind } => list_cmd(ctx, &kind),
        Mcp => Err(PxlError::usage("pxl mcp is not implemented")
            .with_hint(
                "M2 will add a stdio MCP server. Drive pxl with --json from the shell for now",
            )
            .into()),
    }
}

fn gen(ctx: &Ctx, args: GenArgs) -> CmdResult<()> {
    util::check_lock(ctx, None)?;
    let seed = ctx.seed.unwrap_or(1);
    let mut warnings = Vec::new();
    if ctx.seed.is_none() && args.generator != "shapes" && args.generator != "parts" {
        warnings.push("seed defaulted to 1; pass --seed to make the choice explicit".into());
    }
    let base = PathBuf::from(&args.out);
    let base = base.parent().unwrap_or_else(|| Path::new("."));
    let (palette_ref, palette) = resolve_gen_palette(ctx, &args, base)?;
    let (doc, frames) = match args.generator.as_str() {
        "bollinger" => {
            let template = args.template.as_deref().ok_or_else(|| {
                PxlError::usage("bollinger requires --template")
                    .with_hint("template is a built-in id or a .template.json path")
            })?;
            let mirror = match &args.mirror {
                Some(m) => Some(Mirror::parse(m)?),
                None => None,
            };
            let mask = load_mask_maybe_mirror(template, base, mirror)?;
            let frame = generate_bollinger(&mask, seed, &palette, args.ramp.as_deref())?;
            let doc = doc_from_frames(
                &palette_ref,
                &[("base".into(), frame.clone())],
                seed,
                "sprite",
            )?;
            (doc, vec![frame])
        }
        "shapes" => {
            let spec_path = args.spec.as_deref().ok_or_else(|| {
                PxlError::usage("shapes requires --spec")
                    .with_hint("pass the shapes JSON, for example tank_body.shapes.json")
            })?;
            let text = util::read_text(spec_path)?;
            let spec: ShapesFile = pxl_format::parse_shapes(&text)?;
            let frame = generate_shapes(&spec, &palette)?;
            let doc = doc_from_frames(
                &palette_ref,
                &[("base".into(), frame.clone())],
                seed,
                "sprite",
            )?;
            (doc, vec![frame])
        }
        "noise-tile" => {
            let kind = args.kind.as_deref().unwrap_or("dirt");
            let size = args.size.unwrap_or(16);
            let n = args.variants.max(1);
            if n > 32 {
                return Err(PxlError::usage("variants must be <= 32").into());
            }
            let mut pairs = Vec::new();
            for i in 0..n {
                let frame = generate_noise_tile(
                    kind,
                    size,
                    variant_seed(seed, i),
                    &palette,
                    args.ramp.as_deref(),
                )?;
                let name = if n == 1 {
                    "base".to_string()
                } else {
                    format!("tile_{i}")
                };
                pairs.push((name, frame));
            }
            let frames = pairs.iter().map(|(_, f)| f.clone()).collect();
            let kind_name = if n == 1 { "sprite" } else { "tiles" };
            let doc = doc_from_frames(&palette_ref, &pairs, seed, kind_name)?;
            (doc, frames)
        }
        "parts" => {
            let spec_path = args.spec.as_deref().ok_or_else(|| {
                PxlError::usage("parts requires --spec")
                    .with_hint("pass a .parts.json rig, for example farmer.parts.json")
            })?;
            let text = util::read_text(spec_path)?;
            let rig = pxl_format::parse_parts(&text)?;
            let blank = IndexFrame::new(rig.w, rig.h)?;
            let frame = if let Some(dir) = &args.dir {
                compose_rig(&blank, &rig, dir, &[], &BTreeMap::new())?
            } else {
                blank
            };
            let mut pairs = vec![("base".into(), frame.clone())];
            if let Some(dir) = &args.dir {
                pairs.push((dir.clone(), frame.clone()));
            }
            let mut doc = doc_from_frames(&palette_ref, &pairs, seed, "parts")?;
            doc.meta.parts = Some(spec_file_name(spec_path));
            let frames = pairs.into_iter().map(|(_, f)| f).collect();
            (doc, frames)
        }
        other => {
            return Err(PxlError::usage(format!("unknown generator '{other}'"))
                .with_hint("generators: bollinger, shapes, noise-tile, parts")
                .into());
        }
    };
    let hash = util::save_doc(&doc, &args.out, ctx.overwrite, ctx.json)?;
    util::guard_warnings(ctx, &warnings)?;
    let mut body = util::ok_base("gen");
    body.insert("out".into(), json!(args.out));
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert("colors_used".into(), json!(util::colors_used(&frames)));
    body.insert("warnings".into(), json!(warnings));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn resolve_gen_palette(ctx: &Ctx, args: &GenArgs, base: &Path) -> CmdResult<(String, Palette)> {
    let _ = ctx;
    if let Some(p) = &args.palette {
        let pal = util::load_palette(p, base)?;
        return Ok((p.clone(), pal));
    }
    if let Some(spec) = &args.spec {
        let text = util::read_text(spec)?;
        let palette_ref = if args.generator == "parts" {
            pxl_format::parse_parts(&text)?.palette
        } else {
            let shapes: ShapesFile = pxl_format::parse_shapes(&text)?;
            shapes.palette
        };
        if let Some(p) = palette_ref {
            let spec_base = Path::new(spec).parent().unwrap_or(base);
            let pal = util::load_palette(&p, spec_base)?;
            return Ok((p, pal));
        }
    }
    Err(PxlError::usage("missing --palette")
        .with_hint("pass --palette tank.palette.json or a built-in id")
        .into())
}

fn load_mask_maybe_mirror(
    name: &str,
    base: &Path,
    mirror: Option<Mirror>,
) -> CmdResult<RuntimeMask> {
    let mask = util::load_mask(name, base)?;
    if let Some(m) = mirror {
        let mut mask = mask;
        mask.mirror = m;
        return Ok(mask);
    }
    Ok(mask)
}

fn doc_from_frames(
    palette_ref: &str,
    pairs: &[(String, IndexFrame)],
    seed: u64,
    kind: &str,
) -> CmdResult<PxlDoc> {
    let (w, h) = pairs
        .first()
        .map(|(_, f)| (f.w, f.h))
        .ok_or_else(|| PxlError::semantic("generator produced no frames"))?;
    let mut frames = BTreeMap::new();
    for (name, frame) in pairs {
        if frame.w != w || frame.h != h {
            return Err(PxlError::semantic("generated frames differ in size").into());
        }
        frames.insert(name.clone(), pxl_core::encode_matrix(frame)?);
    }
    Ok(PxlDoc {
        v: 1,
        palette: palette_ref.to_string(),
        w,
        h,
        frames,
        anims: BTreeMap::new(),
        meta: Meta {
            seed: Some(seed),
            recipe: None,
            pxl_version: Some(PXL_VERSION.into()),
            gen_version: Some(GEN_VERSION),
            asset: None,
            kind: Some(kind.into()),
            parts: None,
        },
    })
}

fn spec_file_name(spec: &str) -> String {
    Path::new(spec)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(spec)
        .to_string()
}

fn op_cmd(ctx: &Ctx, args: OpArgs) -> CmdResult<()> {
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

fn collect_ops(file: Option<&str>, inline: &[String]) -> CmdResult<Vec<Op>> {
    let mut ops = Vec::new();
    if let Some(path) = file {
        ops.extend(pxl_format::parse_ops_value(&util::read_text(path)?)?);
    }
    for text in inline {
        ops.extend(pxl_format::parse_ops_value(text)?);
    }
    if ops.is_empty() {
        return Err(PxlError::usage("no ops given")
            .with_hint("pass --op '{\"op\":\"flip\",\"axis\":\"x\"}' or --ops ops.json")
            .into());
    }
    Ok(ops)
}

fn apply_with(
    frame: &IndexFrame,
    ops: &[Op],
    palette: &Palette,
    seed: u64,
    frame_name: &str,
    base: &Path,
    parts: Option<&pxl_format::PartsFile>,
) -> CmdResult<IndexFrame> {
    let loader = |name: &str| util::load_mask(name, base);
    let cx = ApplyCx {
        palette,
        seed,
        frame_name,
        load_template: Some(&loader),
        parts,
        pose: RefCell::new(BTreeMap::new()),
    };
    Ok(apply_ops(frame, ops, &cx)?)
}

fn load_parts(doc: &PxlDoc, base: &Path) -> CmdResult<Option<pxl_format::PartsFile>> {
    let Some(name) = &doc.meta.parts else {
        return Ok(None);
    };
    let path = if Path::new(name).is_absolute() {
        PathBuf::from(name)
    } else {
        base.join(name)
    };
    let text = fs::read_to_string(&path).map_err(|e| {
        PxlError::io(format!("cannot read parts {}: {e}", path.display()))
            .with_hint("meta.parts is the rig file name next to the pxl document")
    })?;
    Ok(Some(pxl_format::parse_parts(&text)?))
}

fn anim_cmd(ctx: &Ctx, args: AnimArgs) -> CmdResult<()> {
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
    if fps == 0 || fps > 60 {
        return Err(PxlError::schema("fps must be in 1..=60").into());
    }
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
        if lists.is_empty() || lists.len() > 16 {
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

fn default_fps(name: &str) -> u32 {
    match name {
        "move" => 10,
        "idle" => 4,
        _ => 8,
    }
}

fn frames_vec(doc: &PxlDoc) -> CmdResult<Vec<IndexFrame>> {
    Ok(util::frames_of(doc)?.into_iter().map(|(_, f)| f).collect())
}

fn palette_cmd(ctx: &Ctx, action: PaletteCmd) -> CmdResult<()> {
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

fn apply_palette_op(
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

fn tile_cmd(ctx: &Ctx, action: TileCmd) -> CmdResult<()> {
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

fn sheet_cmd(ctx: &Ctx, args: SheetArgs) -> CmdResult<()> {
    if args.inputs.is_empty() {
        return Err(PxlError::usage("sheet needs at least one .pxl.json")
            .with_hint("pxl sheet tank.pxl.json ground.pxl.json -o out/tank.sheet.png")
            .into());
    }
    let mut loaded = Vec::new();
    for input in &args.inputs {
        let (doc, path) = util::load_doc(input)?;
        util::check_lock(ctx, doc.meta.pxl_version.as_deref())?;
        let palette = util::doc_palette(&doc, &path)?;
        let id = doc
            .meta
            .asset
            .clone()
            .unwrap_or_else(|| util::stem_id(&path));
        loaded.push(Loaded { id, doc, palette });
    }
    let (png, json_text, hash, w, h, count) =
        pack_loaded(&loaded, &args.layout, args.pad, &args.out)?;
    util::write_out(&args.out, &png, ctx.overwrite, ctx.json)?;
    let json_path = Path::new(&args.out).with_extension("json");
    util::write_out(
        json_path.to_str().unwrap_or("sheet.json"),
        json_text.as_bytes(),
        true,
        false,
    )?;
    let mut body = util::ok_base("sheet");
    body.insert("out".into(), json!(args.out));
    body.insert("png".into(), json!(args.out));
    body.insert("png_w".into(), json!(w));
    body.insert("png_h".into(), json!(h));
    body.insert("frames_count".into(), json!(count));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

struct Loaded {
    id: String,
    doc: PxlDoc,
    palette: Palette,
}

fn pack_loaded(
    loaded: &[Loaded],
    layout: &str,
    pad: u32,
    png_out: &str,
) -> CmdResult<(Vec<u8>, String, String, u32, u32, usize)> {
    let palette = &loaded[0].palette;
    for doc in loaded.iter().skip(1) {
        if doc.palette.colors.len() != palette.colors.len()
            || doc
                .palette
                .colors
                .iter()
                .zip(palette.colors.iter())
                .any(|(a, b)| a != b)
        {
            return Err(PxlError::semantic("sheet inputs use different palettes")
                .with_hint("quantize every document onto one palette before packing")
                .into());
        }
    }
    let mut rows = Vec::new();
    let mut pxl_anims = Map::new();
    let mut fps = Map::new();
    let mut facings = Map::new();
    let mut tiles = Map::new();
    let mut dirs = Vec::new();
    let mut used_tags = BTreeSet::new();
    let mut seen_anims = BTreeSet::new();
    for doc in loaded {
        let frames = frame_map(&doc.doc)?;
        let mut doc_rows = rows_for(&doc.id, &doc.doc, &frames)?;
        for row in &mut doc_rows {
            let Some(current) = row.first().and_then(|c| c.tag.clone()) else {
                continue;
            };
            let tag = if used_tags.contains(&current) {
                format!("{}_{current}", doc.id)
            } else {
                current
            };
            used_tags.insert(tag.clone());
            for cell in row.iter_mut() {
                cell.tag = Some(tag.clone());
            }
        }
        if doc.doc.anims.is_empty() {
            let names: Vec<String> = doc_rows.iter().flatten().map(|c| c.name.clone()).collect();
            if !names.is_empty() {
                tiles.insert(
                    doc.id.clone(),
                    json!({
                        "from": 0,
                        "to": 0,
                        "ramp": doc.palette.ramps.keys().find(|k| k.as_str()=="ground").cloned().unwrap_or_else(|| "ground".into()),
                        "frames": names,
                    }),
                );
            }
        }
        for (name, anim) in &doc.doc.anims {
            if !seen_anims.insert(name.clone()) {
                continue;
            }
            fps.insert(name.clone(), json!(anim.fps));
            let mut dir_map = Map::new();
            for row in &doc_rows {
                if let Some(tag) = row.first().and_then(|c| c.tag.clone()) {
                    if tag.starts_with(&format!("{name}_")) || tag == *name {
                        let names: Vec<_> = row.iter().map(|c| c.name.clone()).collect();
                        let dir = tag.strip_prefix(&format!("{name}_")).unwrap_or("");
                        if dir.is_empty() {
                            dir_map.insert("_".into(), json!(names));
                        } else {
                            dir_map.insert(dir.to_string(), json!(names));
                        }
                    }
                }
            }
            pxl_anims.insert(
                name.clone(),
                json!({"fps": anim.fps, "loop": anim.looped, "dirs": Value::Object(dir_map)}),
            );
        }
        if dirs.is_empty() {
            for d in FACING_NAMES {
                if frames.contains_key(*d) {
                    dirs.push((*d).to_string());
                }
            }
        }
        for d in FACING_NAMES {
            if facings.contains_key(*d) {
                continue;
            }
            let key = format!("idle_{d}_0");
            if frames.contains_key(&key) {
                facings.insert((*d).to_string(), json!(format!("{}_{}", doc.id, key)));
            } else if frames.contains_key(*d) {
                facings.insert((*d).to_string(), json!(format!("{}_{}", doc.id, *d)));
            }
        }
        rows.extend(doc_rows);
    }
    // Fill tile from/to after we know placement order.
    let packed = pack_sheet(&rows, layout, pad)?;
    let cursor = 0u32;
    for (id, value) in tiles.iter_mut() {
        let n = value
            .get("frames")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0) as u32;
        if let Some(obj) = value.as_object_mut() {
            // Find real indices by name.
            if let Some(frames) = obj.get("frames").and_then(|v| v.as_array()) {
                let names: Vec<String> = frames
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if let Some(first) = names.first() {
                    if let Some(pos) = packed.placements.iter().position(|p| p.name == *first) {
                        obj.insert("from".into(), json!(pos));
                        obj.insert("to".into(), json!(pos + names.len() - 1));
                    }
                }
            }
            let _ = (id, n, cursor);
        }
    }
    let image_name = Path::new(png_out)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png");
    let pivot = [
        packed.placements.first().map(|p| p.w / 2).unwrap_or(0),
        packed.placements.first().map(|p| p.h / 2).unwrap_or(0),
    ];
    let pxl = json!({
        "version": PXL_VERSION,
        "gen_version": GEN_VERSION,
        "palette": loaded[0].doc.palette,
        "colors": loaded[0].palette.colors.iter().map(|c| c.to_hex()).collect::<Vec<_>>(),
        "ramps": loaded[0].palette.ramps,
        "cycles": loaded[0].palette.cycles.iter().map(|(k, c)| (k.clone(), json!({"indices": c.indices, "period": c.period}))).collect::<Map<_, _>>(),
        "special": {
            "transparent": loaded[0].palette.special.transparent,
            "outline": loaded[0].palette.special.outline,
            "flash": loaded[0].palette.special.flash,
        },
        "pivot": pivot,
        "dirs": dirs,
        "fps": fps,
        "facings": facings,
        "anims": pxl_anims,
        "tiles": tiles,
    });
    let text = sheet_json(&packed, image_name, pxl)?;
    let png = encode_indexed_png(&packed.image, palette, 1)?;
    let hash = util::hash_of(&png);
    Ok((
        png,
        text,
        hash,
        packed.image.w as u32,
        packed.image.h as u32,
        packed.placements.len(),
    ))
}

fn frame_map(doc: &PxlDoc) -> CmdResult<BTreeMap<String, IndexFrame>> {
    let mut map = BTreeMap::new();
    for (name, frame) in util::frames_of(doc)? {
        map.insert(name, frame);
    }
    Ok(map)
}

fn rows_for(
    id: &str,
    doc: &PxlDoc,
    frames: &BTreeMap<String, IndexFrame>,
) -> CmdResult<Vec<Vec<SheetCell>>> {
    if doc.anims.is_empty() {
        let mut row = Vec::new();
        for (name, frame) in frames {
            row.push(SheetCell {
                name: format!("{id}_{name}"),
                frame: frame.clone(),
                duration_ms: 1000,
                tag: Some(id.to_string()),
            });
        }
        return Ok(vec![row]);
    }
    let mut names: Vec<String> = doc.anims.keys().cloned().collect();
    names.sort_by(|a, b| anim_rank(a).cmp(&anim_rank(b)).then(a.cmp(b)));
    let mut rows = Vec::new();
    for name in names {
        let anim = &doc.anims[&name];
        let duration = (1000 / anim.fps.max(1)).max(1);
        let dirs = anim.dir_list();
        if dirs.is_empty() {
            let seq = anim.frames.sequence(None)?;
            rows.push(cells_for(id, &name, &seq, frames, duration)?);
        } else {
            for dir in dirs {
                let seq = anim.frames.sequence(Some(&dir))?;
                let tag = format!("{name}_{dir}");
                rows.push(cells_for(id, &tag, &seq, frames, duration)?);
            }
        }
    }
    Ok(rows)
}

fn cells_for(
    id: &str,
    tag: &str,
    names: &[String],
    frames: &BTreeMap<String, IndexFrame>,
    duration: u32,
) -> CmdResult<Vec<SheetCell>> {
    let mut row = Vec::new();
    for name in names {
        let frame = frames.get(name).ok_or_else(|| {
            PxlError::semantic(format!("animation references missing frame '{name}'"))
        })?;
        row.push(SheetCell {
            name: format!("{id}_{name}"),
            frame: frame.clone(),
            duration_ms: duration,
            tag: Some(tag.to_string()),
        });
    }
    Ok(row)
}

const FACING_NAMES: &[&str] = &["N", "E", "S", "W", "down", "up", "left", "right"];

fn anim_rank(name: &str) -> u8 {
    match name {
        "move" | "walk" => 0,
        "idle" => 1,
        "hoe" => 2,
        _ => 3,
    }
}

fn render_cmd(ctx: &Ctx, args: RenderArgs) -> CmdResult<()> {
    if args.scale == 0 || args.scale > 16 {
        return Err(PxlError::usage("scale must be in 1..=16").into());
    }
    let path = Path::new(&args.input);
    if args.input.ends_with(".png") || looks_like_png(&args.input) {
        let bytes =
            fs::read(&args.input).map_err(|e| PxlError::io(format!("{}: {e}", args.input)))?;
        let (w, h, rgba) = decode_png_rgba(&bytes)?;
        let (ow, oh, scaled) = scale_rgba(w, h, &rgba, args.scale, args.grid);
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
    let (frames, delay) = if let Some(anim_name) = &args.anim {
        let anim = doc.anims.get(anim_name).ok_or_else(|| {
            PxlError::semantic(format!("unknown anim '{anim_name}'"))
                .with_hint("pxl inspect lists animations")
        })?;
        let names = anim.frames.sequence(args.dir.as_deref())?;
        let mut frames = Vec::new();
        for name in names {
            frames.push(doc.frame(&name)?);
        }
        let delay = (100 / anim.fps.max(1)).max(1) as u16;
        (frames, delay)
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
        (vec![doc.frame(&name)?], 10u16)
    };
    let gif = args.out.ends_with(".gif");
    let bytes = if gif {
        encode_gif(&frames, &palette, args.scale, delay)?
    } else if frames.len() == 1 {
        render_preview_png(&frames[0], &palette, args.scale, args.grid)?
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
        render_preview_png(&packed.image, &palette, args.scale, args.grid)?
    };
    let hash = util::hash_of(&bytes);
    util::write_out(&args.out, &bytes, ctx.overwrite, ctx.json)?;
    let png_w = frames[0].w as u32 * args.scale;
    let png_h = frames[0].h as u32 * args.scale;
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

fn looks_like_png(path: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"\x89PNG")
}

fn inspect_cmd(ctx: &Ctx, args: InspectArgs) -> CmdResult<()> {
    let (doc, doc_path) = util::load_doc(&args.input)?;
    let palette = util::doc_palette(&doc, &doc_path).ok();
    let names: Vec<String> = if let Some(frame) = &args.frame {
        vec![frame.clone()]
    } else {
        doc.frame_names()
    };
    let mut listed = Vec::new();
    for name in &names {
        let frame = doc.frame(name)?;
        let stats = frame.stats();
        let ascii = pxl_core::encode_matrix(&frame)?;
        if !ctx.json && !ctx.quiet {
            println!(
                "{name} {}x{} colors {} opaque {} components {} isolated {}",
                frame.w, frame.h, stats.colors_used, stats.opaque, stats.components, stats.isolated
            );
            if let Some(b) = stats.bbox {
                println!("  bbox x={} y={} w={} h={}", b.x, b.y, b.w, b.h);
            }
            for row in &ascii {
                println!("  {row}");
            }
        }
        listed.push(json!({
            "name": name,
            "colors_used": stats.colors_used,
            "opaque": stats.opaque,
            "components": stats.components,
            "isolated": stats.isolated,
            "bbox": stats.bbox.map(|b| json!({"x": b.x, "y": b.y, "w": b.w, "h": b.h})),
            "ascii": ascii,
        }));
    }
    let mut body = util::ok_base("inspect");
    body.insert("w".into(), json!(doc.w));
    body.insert("h".into(), json!(doc.h));
    body.insert("palette".into(), json!(doc.palette));
    body.insert(
        "palette_colors".into(),
        json!(palette.as_ref().map(|p| p.colors.len()).unwrap_or(0)),
    );
    body.insert("frames".into(), json!(doc.frame_names()));
    body.insert(
        "anims".into(),
        json!(doc.anims.keys().cloned().collect::<Vec<_>>()),
    );
    body.insert("view".into(), Value::Array(listed));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn validate_cmd(ctx: &Ctx, args: ValidateArgs) -> CmdResult<()> {
    let report = load_report(&args.input, &args.rules)?;
    let failed = report.checks.iter().filter(|c| !c.pass).count();
    let checks = checks_json(&report);
    if report.ok() {
        let mut body = util::ok_base("validate");
        body.insert("rules".into(), json!(report.rules));
        body.insert("checks".into(), checks);
        util::emit_ok(ctx, &Value::Object(body));
        Ok(())
    } else {
        Err(CmdError::new(
            PxlError::qa(format!("{failed} check(s) failed"))
                .with_hint("read the checks array and adjust the recipe"),
        )
        .with_extra(json!({"cmd": "validate", "rules": report.rules, "checks": checks})))
    }
}

fn checks_json(report: &Report) -> Value {
    Value::Array(
        report
            .checks
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "pass": c.pass,
                    "value": c.value,
                    "limit": c.limit,
                })
            })
            .collect(),
    )
}

fn load_report(path: &str, rules: &str) -> CmdResult<Report> {
    let text = util::read_text(path)?;
    let value: Value = pxl_format::parse_json(&text, "validate input")?;
    if value.get("meta").and_then(|m| m.get("frameTags")).is_some() {
        let input = sheet_input(Path::new(path), &value)?;
        return Ok(validate(&input, rules)?);
    }
    let (doc, doc_path) = {
        let doc: PxlDoc = pxl_format::parse_json(&text, "pxl document")?;
        (doc, PathBuf::from(path))
    };
    let palette = util::doc_palette(&doc, &doc_path)?;
    Ok(validate(&doc_input(&doc, &palette)?, rules)?)
}

fn doc_input(doc: &PxlDoc, palette: &Palette) -> CmdResult<ValidateInput> {
    let frames = util::frames_of(doc)?;
    let mut anims = Vec::new();
    for (name, anim) in &doc.anims {
        let dirs = anim.dir_list();
        if dirs.is_empty() {
            let names = anim.frames.sequence(None)?;
            anims.push(AnimSeq {
                name: name.clone(),
                dir: None,
                looped: anim.looped,
                frames: lookup_frames(doc, &names)?,
            });
        } else {
            for dir in dirs {
                let names = anim.frames.sequence(Some(&dir))?;
                anims.push(AnimSeq {
                    name: name.clone(),
                    dir: Some(dir.clone()),
                    looped: anim.looped,
                    frames: lookup_frames(doc, &names)?,
                });
            }
        }
    }
    let mut facings = BTreeMap::new();
    for d in FACING_NAMES {
        let idle = format!("idle_{d}_0");
        if doc.frames.contains_key(&idle) {
            facings.insert((*d).to_string(), doc.frame(&idle)?);
        } else if doc.frames.contains_key(*d) {
            facings.insert((*d).to_string(), doc.frame(d)?);
        }
    }
    let tiles = if doc.meta.kind.as_deref() == Some("tiles") {
        frames.clone()
    } else {
        Vec::new()
    };
    Ok(ValidateInput {
        palette: palette.clone(),
        frames,
        anims,
        facings,
        tiles,
        tile_ramp: "ground".into(),
    })
}

fn lookup_frames(doc: &PxlDoc, names: &[String]) -> CmdResult<Vec<IndexFrame>> {
    let mut out = Vec::new();
    for name in names {
        out.push(doc.frame(name)?);
    }
    Ok(out)
}

fn sheet_input(json_path: &Path, value: &Value) -> CmdResult<ValidateInput> {
    let meta = value
        .get("meta")
        .ok_or_else(|| PxlError::schema("sheet json has no meta"))?;
    let image = meta
        .get("image")
        .and_then(|v| v.as_str())
        .unwrap_or("sheet.png");
    let png_path = json_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(image);
    let bytes = fs::read(&png_path).map_err(|e| {
        PxlError::io(format!(
            "cannot read sheet image {}: {e}",
            png_path.display()
        ))
        .with_hint("the png sits next to the sheet json; meta.image is the file name")
    })?;
    let (w, _h, pixels, png_colors) = decode_indexed(&bytes)?;
    let pxl = meta.get("pxl").cloned().unwrap_or_else(|| json!({}));
    let palette = palette_from_pxl(&pxl).or_else(|_| palette_from_colors(&png_colors))?;
    let frames_obj = value
        .get("frames")
        .and_then(|v| v.as_object())
        .ok_or_else(|| PxlError::schema("sheet json has no frames object"))?;
    let mut named = BTreeMap::new();
    let mut ordered = Vec::new();
    for (name, entry) in frames_obj {
        let fr = entry
            .get("frame")
            .ok_or_else(|| PxlError::schema(format!("frame '{name}' has no frame rect")))?;
        let x = json_u32(fr, "x")?;
        let y = json_u32(fr, "y")?;
        let fw = json_u32(fr, "w")?;
        let fh = json_u32(fr, "h")?;
        let frame = crop_indexed(&pixels, w, x, y, fw, fh)?;
        ordered.push((name.clone(), frame.clone()));
        named.insert(name.clone(), frame);
    }
    let mut anims = Vec::new();
    if let Some(obj) = pxl.get("anims").and_then(|v| v.as_object()) {
        for (name, anim) in obj {
            let looped = anim.get("loop").and_then(|v| v.as_bool()).unwrap_or(true);
            let dirs = anim.get("dirs").and_then(|v| v.as_object());
            if let Some(dirs) = dirs {
                for (dir, list) in dirs {
                    let names = string_list(list)?;
                    let frames = names
                        .iter()
                        .map(|n| {
                            named.get(n).cloned().ok_or_else(|| {
                                CmdError::from(PxlError::semantic(format!(
                                    "sheet is missing frame '{n}'"
                                )))
                            })
                        })
                        .collect::<CmdResult<Vec<_>>>()?;
                    anims.push(AnimSeq {
                        name: name.clone(),
                        dir: if dir == "_" { None } else { Some(dir.clone()) },
                        looped,
                        frames,
                    });
                }
            }
        }
    }
    let mut facings = BTreeMap::new();
    if let Some(obj) = pxl.get("facings").and_then(|v| v.as_object()) {
        for (dir, name) in obj {
            if let Some(n) = name.as_str() {
                if let Some(frame) = named.get(n) {
                    facings.insert(dir.clone(), frame.clone());
                }
            }
        }
    }
    let mut tiles = Vec::new();
    let mut tile_ramp = "ground".to_string();
    if let Some(obj) = pxl.get("tiles").and_then(|v| v.as_object()) {
        for (_id, tile) in obj {
            if let Some(r) = tile.get("ramp").and_then(|v| v.as_str()) {
                tile_ramp = r.to_string();
            }
            for name in string_list(tile.get("frames").unwrap_or(&Value::Null)).unwrap_or_default()
            {
                if let Some(frame) = named.get(&name) {
                    tiles.push((name, frame.clone()));
                }
            }
        }
    }
    Ok(ValidateInput {
        palette,
        frames: ordered,
        anims,
        facings,
        tiles,
        tile_ramp,
    })
}

fn palette_from_pxl(pxl: &Value) -> CmdResult<Palette> {
    let colors = pxl
        .get("colors")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PxlError::schema("sheet pxl.colors is missing"))?;
    let file = PaletteFile {
        v: 1,
        id: "sheet".into(),
        colors: colors
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        ramps: serde_json::from_value(pxl.get("ramps").cloned().unwrap_or_else(|| json!({})))
            .unwrap_or_default(),
        cycles: serde_json::from_value(pxl.get("cycles").cloned().unwrap_or_else(|| json!({})))
            .unwrap_or_default(),
        special: serde_json::from_value(pxl.get("special").cloned().unwrap_or_else(|| json!({})))
            .unwrap_or_default(),
    };
    Ok(file.into_palette("sheet")?)
}

fn palette_from_colors(colors: &[pxl_core::Rgba]) -> CmdResult<Palette> {
    Ok(Palette {
        id: "png".into(),
        colors: colors.to_vec(),
        ramps: BTreeMap::new(),
        cycles: BTreeMap::new(),
        special: pxl_core::Special::default(),
    })
}

fn crop_indexed(
    pixels: &[u8],
    sheet_w: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> CmdResult<IndexFrame> {
    let mut frame = IndexFrame::canvas(w as u16, h as u16)?;
    for row in 0..h {
        for col in 0..w {
            let src = ((y + row) * sheet_w + (x + col)) as usize;
            let c = pixels.get(src).copied().unwrap_or(0);
            frame.set(col as i32, row as i32, c);
        }
    }
    Ok(frame)
}

fn json_u32(v: &Value, key: &str) -> CmdResult<u32> {
    v.get(key)
        .and_then(|n| n.as_u64())
        .map(|n| n as u32)
        .ok_or_else(|| PxlError::schema(format!("missing integer '{key}'")).into())
}

fn string_list(v: &Value) -> CmdResult<Vec<String>> {
    let Some(arr) = v.as_array() else {
        return Ok(Vec::new());
    };
    Ok(arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

fn schema_cmd(ctx: &Ctx, kind: &str) -> CmdResult<()> {
    let value = schema_json(kind)?;
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string(&value).map_err(|e| PxlError::io(e.to_string()))?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).map_err(|e| PxlError::io(e.to_string()))?
        );
    }
    Ok(())
}

fn list_cmd(ctx: &Ctx, kind: &str) -> CmdResult<()> {
    let items = match kind {
        "generators" => vec![
            "bollinger".into(),
            "shapes".into(),
            "noise-tile".into(),
            "parts".into(),
        ],
        "ops" => pxl_format::OP_NAMES
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        "macros" => MACRO_NAMES.iter().map(|s| (*s).to_string()).collect(),
        "templates" => template_ids().into_iter().map(|s| s.to_string()).collect(),
        "palettes" => palette_ids().into_iter().map(|s| s.to_string()).collect(),
        other => {
            return Err(PxlError::usage(format!("unknown list kind '{other}'"))
                .with_hint("list generators, ops, macros, templates, or palettes")
                .into());
        }
    };
    let mut body = util::ok_base("list");
    body.insert("kind".into(), json!(kind));
    body.insert("items".into(), json!(items));
    util::emit_ok(ctx, &Value::Object(body));
    let _ = builtin_palette;
    let _ = builtin_template;
    Ok(())
}

fn build_cmd(ctx: &Ctx, args: BuildArgs) -> CmdResult<()> {
    util::check_lock(ctx, None)?;
    let text = util::read_text(&args.recipe)?;
    let recipe: Recipe = pxl_format::parse_json(&text, "recipe")?;
    if ctx.lock.is_some() && !crate::util::check_lock(ctx, Some(PXL_VERSION)).is_ok() {
        return util::check_lock(ctx, Some(PXL_VERSION));
    }
    let recipe_path = PathBuf::from(&args.recipe);
    let recipe_dir = recipe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let out_dir = PathBuf::from(&args.out);
    if out_dir.exists() && !out_dir.is_dir() {
        return Err(
            PxlError::semantic(format!("{} is not a directory", out_dir.display()))
                .with_hint("pxl build -o takes a directory")
                .into(),
        );
    }
    fs::create_dir_all(&out_dir)
        .map_err(|e| PxlError::io(format!("cannot create output dir: {e}")))?;
    let palette = util::load_palette(&recipe.palette, &recipe_dir)?;
    let pal_name = Path::new(&recipe.palette)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("palette.json")
        .to_string();
    let pal_src = if recipe_dir.join(&recipe.palette).is_file() {
        recipe_dir.join(&recipe.palette)
    } else {
        PathBuf::from(&recipe.palette)
    };
    let pal_bytes = if pal_src.is_file() {
        fs::read(&pal_src).map_err(|e| PxlError::io(format!("palette: {e}")))?
    } else {
        // Built-in palette: write canonical JSON.
        let file_text = match pal_name.as_str() {
            "tank.palette.json" | "tank" => {
                include_str!("../../../assets/palettes/tank.palette.json")
                    .as_bytes()
                    .to_vec()
            }
            _ => include_str!("../../../assets/palettes/p16.palette.json")
                .as_bytes()
                .to_vec(),
        };
        file_text
    };
    let stored_pal_name = if pal_name.ends_with(".json") {
        pal_name.clone()
    } else {
        format!("{pal_name}.palette.json")
    };
    util::write_out(
        out_dir.join(&stored_pal_name).to_str().unwrap(),
        &pal_bytes,
        true,
        false,
    )?;
    let seed = ctx.seed.unwrap_or(recipe.seed);
    let mut loaded = Vec::new();
    let mut outputs = vec![stored_pal_name.clone()];
    let mut copied_specs = BTreeSet::new();
    for sprite in &recipe.sprites {
        if sprite.base.gen == "parts" {
            if let Some(spec) = &sprite.base.spec {
                let name = spec_file_name(spec);
                if copied_specs.insert(name.clone()) {
                    let src = recipe_dir.join(spec);
                    let bytes = fs::read(&src).map_err(|e| {
                        PxlError::io(format!("cannot read parts {}: {e}", src.display()))
                    })?;
                    util::write_out(out_dir.join(&name).to_str().unwrap(), &bytes, true, false)?;
                    outputs.push(name);
                }
            }
        }
    }
    for sprite in &recipe.sprites {
        let doc = build_sprite(
            sprite,
            &palette,
            &stored_pal_name,
            seed,
            &recipe,
            &recipe_dir,
            &recipe_path,
        )?;
        let path = out_dir.join(format!("{}.pxl.json", sprite.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", sprite.id));
        loaded.push(Loaded {
            id: sprite.id.clone(),
            doc,
            palette: palette.clone(),
        });
    }
    for tile in &recipe.tiles {
        let mut pairs = Vec::new();
        let n = tile.variants.max(1);
        for i in 0..n {
            let frame = generate_noise_tile(
                &tile.kind,
                tile.size,
                variant_seed(ctx.seed.unwrap_or(tile.seed), i),
                &palette,
                tile.ramp.as_deref(),
            )?;
            pairs.push((format!("tile_{i}"), frame));
        }
        let mut doc = doc_from_frames(&stored_pal_name, &pairs, tile.seed, "tiles")?;
        doc.meta.asset = Some(tile.id.clone());
        doc.meta.recipe = Some(
            recipe_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("recipe")
                .into(),
        );
        let path = out_dir.join(format!("{}.pxl.json", tile.id));
        util::save_doc(&doc, path.to_str().unwrap(), true, false)?;
        outputs.push(format!("{}.pxl.json", tile.id));
        loaded.push(Loaded {
            id: tile.id.clone(),
            doc,
            palette: palette.clone(),
        });
    }
    let order: Vec<String> = if recipe.sheet.sources.is_empty() {
        loaded.iter().map(|d| d.id.clone()).collect()
    } else {
        recipe.sheet.sources.clone()
    };
    let mut ordered = Vec::new();
    for id in &order {
        let doc = loaded
            .iter()
            .find(|d| d.id == *id)
            .ok_or_else(|| PxlError::semantic(format!("sheet source '{id}' was not built")))?;
        ordered.push(Loaded {
            id: doc.id.clone(),
            doc: doc.doc.clone(),
            palette: doc.palette.clone(),
        });
    }
    let sheet_path = out_dir.join(&recipe.sheet.file);
    let (png, json_text, hash, _w, _h, _n) = pack_loaded(
        &ordered,
        &recipe.sheet.layout,
        recipe.sheet.pad,
        sheet_path.to_str().unwrap_or("sheet.png"),
    )?;
    util::write_out(sheet_path.to_str().unwrap(), &png, true, false)?;
    let sheet_json_path = sheet_path.with_extension("json");
    util::write_out(
        sheet_json_path.to_str().unwrap(),
        json_text.as_bytes(),
        true,
        false,
    )?;
    outputs.push(recipe.sheet.file.clone());
    outputs.push(
        sheet_json_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sheet.json")
            .to_string(),
    );
    for preview in &recipe.preview {
        let dest = out_dir.join(&preview.file);
        if preview.sheet == Some(true) {
            render_cmd(
                &overwrite_ctx(ctx),
                RenderArgs {
                    input: sheet_path.to_string_lossy().into_owned(),
                    frame: None,
                    anim: None,
                    dir: None,
                    scale: preview.scale,
                    grid: preview.grid,
                    out: dest.to_string_lossy().into_owned(),
                },
            )?;
        } else {
            let source = preview.source.clone().unwrap_or_else(|| {
                recipe
                    .sprites
                    .first()
                    .map(|s| s.id.clone())
                    .unwrap_or_default()
            });
            let doc_path = out_dir.join(format!("{source}.pxl.json"));
            render_cmd(
                &overwrite_ctx(ctx),
                RenderArgs {
                    input: doc_path.to_string_lossy().into_owned(),
                    frame: None,
                    anim: preview.anim.clone(),
                    dir: preview.dir.clone(),
                    scale: preview.scale,
                    grid: preview.grid,
                    out: dest.to_string_lossy().into_owned(),
                },
            )?;
        }
        outputs.push(preview.file.clone());
    }
    let rules = recipe
        .validate
        .as_ref()
        .map(|v| v.rules.as_str())
        .unwrap_or("default");
    let report = load_report(sheet_json_path.to_str().unwrap(), rules)?;
    let checks = checks_json(&report);
    if !report.ok() {
        return Err(CmdError::new(
            PxlError::qa(format!(
                "build wrote outputs but {} QA check(s) failed",
                report.checks.iter().filter(|c| !c.pass).count()
            ))
            .with_hint("read checks and adjust the recipe"),
        )
        .with_extra(json!({"cmd": "build", "checks": checks, "hash": hash})));
    }
    let mut body = util::ok_base("build");
    body.insert("out".into(), json!(args.out));
    body.insert("outputs".into(), json!(outputs));
    body.insert("hash".into(), json!(hash));
    body.insert("checks".into(), checks);
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

fn overwrite_ctx(ctx: &Ctx) -> Ctx {
    let mut c = ctx.clone();
    c.overwrite = true;
    c.json = false;
    c.quiet = true;
    c
}

fn build_sprite(
    sprite: &pxl_format::RecipeSprite,
    palette: &Palette,
    palette_ref: &str,
    seed: u64,
    recipe: &Recipe,
    recipe_dir: &Path,
    recipe_path: &Path,
) -> CmdResult<PxlDoc> {
    let base_seed = sprite.base.seed.unwrap_or(seed);
    let (frame, rig) = match sprite.base.gen.as_str() {
        "shapes" => {
            let spec = sprite.base.spec.as_deref().ok_or_else(|| {
                PxlError::schema("shapes base requires spec").with_path("/sprites/0/base/spec")
            })?;
            let path = recipe_dir.join(spec);
            let text = fs::read_to_string(&path)
                .map_err(|e| PxlError::io(format!("cannot read {}: {e}", path.display())))?;
            let shapes = pxl_format::parse_shapes(&text)?;
            (generate_shapes(&shapes, palette)?, None)
        }
        "bollinger" => {
            let template = sprite
                .base
                .template
                .as_deref()
                .ok_or_else(|| PxlError::schema("bollinger base requires template"))?;
            let mirror = sprite
                .base
                .mirror
                .as_deref()
                .map(Mirror::parse)
                .transpose()?;
            let mask = load_mask_maybe_mirror(template, recipe_dir, mirror)?;
            (
                generate_bollinger(&mask, base_seed, palette, sprite.base.ramp.as_deref())?,
                None,
            )
        }
        "parts" => {
            let spec = sprite.base.spec.as_deref().ok_or_else(|| {
                PxlError::schema("parts base requires spec").with_path("/sprites/0/base/spec")
            })?;
            let path = recipe_dir.join(spec);
            let text = fs::read_to_string(&path)
                .map_err(|e| PxlError::io(format!("cannot read {}: {e}", path.display())))?;
            let rig = pxl_format::parse_parts(&text)?;
            let frame = IndexFrame::new(rig.w, rig.h)?;
            (frame, Some(rig))
        }
        other => {
            return Err(
                PxlError::schema(format!("unknown base generator '{other}'"))
                    .with_hint("base.gen is shapes, bollinger, or parts")
                    .into(),
            );
        }
    };
    let parts_mode = rig.is_some();
    let mut doc = doc_from_frames(
        palette_ref,
        &[("base".into(), frame.clone())],
        seed,
        if parts_mode { "parts" } else { "sprite" },
    )?;
    doc.meta.asset = Some(sprite.id.clone());
    doc.meta.recipe = Some(
        recipe_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("recipe.json")
            .into(),
    );
    doc.meta.gen_version = Some(recipe.gen_version);
    if let Some(spec) = sprite.base.spec.as_ref().filter(|_| parts_mode) {
        doc.meta.parts = Some(spec_file_name(spec));
    }
    // Parts redraw the canvas in `compose`, so outline and palette swaps run after
    // that draw. Shape and mask sprites still post-process the base first.
    let base_ops: &[Op] = if parts_mode { &[] } else { &sprite.post };
    let base_frame = apply_with(
        &frame,
        base_ops,
        palette,
        seed,
        "base",
        recipe_dir,
        rig.as_ref(),
    )?;
    doc.insert_frame("base", &base_frame)?;
    let mut named: BTreeMap<String, IndexFrame> = BTreeMap::new();
    named.insert("base".into(), base_frame);
    for dir in &sprite.directions {
        let src = named.get(&dir.from).cloned().ok_or_else(|| {
            PxlError::semantic(format!(
                "direction '{}' reads '{}' which has not been built yet",
                dir.id, dir.from
            ))
            .with_hint("list directions so each `from` already exists")
        })?;
        let mut ops = dir.ops.clone();
        if parts_mode {
            ops.extend(sprite.post.iter().cloned());
        }
        let out = apply_with(&src, &ops, palette, seed, &dir.id, recipe_dir, rig.as_ref())?;
        named.insert(dir.id.clone(), out.clone());
        doc.insert_frame(&dir.id, &out)?;
    }
    for anim in &sprite.anims {
        let mut by_dir = BTreeMap::new();
        let mut seq = Vec::new();
        let targets: Vec<Option<String>> = if anim.dirs.is_empty() {
            vec![None]
        } else {
            anim.dirs.iter().cloned().map(Some).collect()
        };
        for target in &targets {
            let source_name = target.clone().unwrap_or_else(|| "base".into());
            let source = named
                .get(&source_name)
                .cloned()
                .or_else(|| doc.frame(&source_name).ok())
                .ok_or_else(|| {
                    PxlError::semantic(format!(
                        "anim '{}' missing source frame '{source_name}'",
                        anim.name
                    ))
                })?;
            let lists = if let Some(mac) = anim.macro_name() {
                expand_macro(mac, target.as_deref(), &source, palette, rig.as_ref())?
            } else if !anim.frames.is_empty() {
                anim.frames.clone()
            } else {
                return Err(PxlError::schema(format!(
                    "anim '{}' has no macro or frames",
                    anim.name
                ))
                .into());
            };
            let mut names = Vec::new();
            for (i, ops) in lists.iter().enumerate() {
                let name = match target {
                    Some(dir) => format!("{}_{dir}_{i}", anim.name),
                    None => format!("{}_{i}", anim.name),
                };
                let mut ops = ops.clone();
                if parts_mode {
                    ops.extend(sprite.post.iter().cloned());
                }
                let frame = apply_with(
                    &source,
                    &ops,
                    palette,
                    seed,
                    &name,
                    recipe_dir,
                    rig.as_ref(),
                )?;
                doc.insert_frame(&name, &frame)?;
                names.push(name);
            }
            if let Some(dir) = target {
                by_dir.insert(dir.clone(), names);
            } else {
                seq = names;
            }
        }
        doc.anims.insert(
            anim.name.clone(),
            Anim {
                fps: anim.fps,
                looped: anim.looped,
                dirs: anim.dirs.clone(),
                frames: if by_dir.is_empty() {
                    AnimFrames::Seq(seq)
                } else {
                    AnimFrames::ByDir(by_dir)
                },
            },
        );
    }
    Ok(doc)
}
