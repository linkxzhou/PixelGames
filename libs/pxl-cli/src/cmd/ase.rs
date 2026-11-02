//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use pxl_art::quantize::DitherMatrix;
use pxl_ase::color::{
    apply_palette, convert_sprite_color_mode_opts, load_palette_file, ConvertColorOpts,
    DitherAlgorithm,
};
use pxl_ase::doc::ColorMode;
use pxl_ase::{
    expand_filename_format_ctx, export_frame_png, export_gif,
    export_sheet_png_json_layout_opts, frame_is_empty, frames_for_tag, inspect_json,
    list_layers_text, list_slices_json, list_slices_text, list_tags_text, merge_duplicate_frames,
    open, select_frames, set_all_layers_visible, set_layer_visibility_by_name, split_slices_to_dir,
    FilenameFormatCtx, SheetExportOpts,
};
use pxl_core::{parse_hex, PxlError};
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;

#[derive(Subcommand, Debug)]
pub enum AseCmd {
    /// Print sprite size, color mode, layers, frames, and tags.
    Inspect(AseInspectArgs),
    /// Flatten and export (PNG/GIF/aseprite). Compatible with a `--save-as` subset.
    Convert(AseConvertArgs),
    /// List layer names (hierarchy indented).
    ListLayers(AseFileArgs),
    /// List animation tags.
    ListTags(AseFileArgs),
    /// List slices.
    ListSlices(AseFileArgs),
    /// Pack frames into an indexed spritesheet PNG + JSON.
    Sheet(AseSheetArgs),
    /// Apply a filter then save `.aseprite` or PNG.
    Filter(AseFilterArgs),
    /// Apply a multi-frame op (flip/rotate90) across the timeline.
    Op(AseOpArgs),
    /// Write one cropped PNG per slice.
    SplitSlices(AseSplitSlicesArgs),
    /// Import aseprite → schema-valid recipe + frames (M8 bridge).
    Import(AseImportArgs),
}

#[derive(Args, Debug)]
pub struct AseInspectArgs {
    pub file: String,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseFileArgs {
    pub file: String,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseConvertArgs {
    /// One or more `.aseprite` inputs (batch when multiple).
    pub files: Vec<String>,
    /// Output path (`.png`, `.gif`, or `.aseprite`). With multiple inputs, treat as directory or format pattern.
    #[arg(long = "save-as", short = 'o')]
    pub save_as: String,
    #[arg(long, short = 'b')]
    pub batch: bool,
    #[arg(long, short = 'p')]
    pub preview: bool,
    #[arg(long)]
    pub oneframe: bool,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long = "frame-range")]
    pub frame_range: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    /// rgba | indexed | grayscale
    #[arg(long = "color-mode")]
    pub color_mode: Option<String>,
    /// External `.gpl` / `.act` palette applied before color-mode convert.
    #[arg(long = "palette")]
    pub palette: Option<String>,
    /// none | ordered | old
    #[arg(long = "dithering-algorithm", default_value = "none")]
    pub dithering_algorithm: String,
    /// bayer2x2 | bayer4x4 | bayer8x8
    #[arg(long = "dithering-matrix", default_value = "bayer8x8")]
    pub dithering_matrix: String,
    /// Frame index when not using --tag/--frame-range (default 0 for single PNG).
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    /// Filename format for sequences: `{frame}` `{tag}` `{layer}` `{title}` …
    #[arg(long = "filename-format")]
    pub filename_format: Option<String>,
    /// Skip fully transparent frames when writing sequences/GIF/sheet.
    #[arg(long = "ignore-empty")]
    pub ignore_empty: bool,
    /// Include hidden layers in flatten.
    #[arg(long = "all-layers")]
    pub all_layers: bool,
    /// Only include this layer name (hide others).
    #[arg(long = "layer")]
    pub layer: Option<String>,
    /// Split slices to cropped PNGs under this directory (also usable via `ase split-slices`).
    #[arg(long = "split-slices")]
    pub split_slices: Option<String>,
    /// Crop document to `x,y,w,h` before export.
    #[arg(long = "crop")]
    pub crop: Option<String>,
    /// Skip consecutive duplicate frames.
    #[arg(long = "merge-duplicates")]
    pub merge_duplicates: bool,
    /// Trim transparent borders on the document before export.
    #[arg(long = "trim")]
    pub trim: bool,
}

#[derive(Args, Debug)]
pub struct AseSheetArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long)]
    pub data: Option<String>,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long = "frame-range")]
    pub frame_range: Option<String>,
    #[arg(long)]
    pub oneframe: bool,
    /// horizontal|vertical|packed|rows|grid (aliases map to pxl-export layouts).
    #[arg(long, default_value = "horizontal")]
    pub layout: String,
    #[arg(long = "ignore-empty")]
    pub ignore_empty: bool,
    #[arg(long = "all-layers")]
    pub all_layers: bool,
    /// Padding between packed cells (px).
    #[arg(long = "padding", default_value_t = 0)]
    pub padding: u32,
    /// Extrude each cell edge by N px (edge duplicate).
    #[arg(long = "extrude", default_value_t = 0)]
    pub extrude: u32,
    /// Trim transparent borders per cell before pack.
    #[arg(long = "trim")]
    pub trim: bool,
    /// Deferred: grid-aligned trim (see STATUS / cli-flag-matrix).
    #[arg(long = "trim-by-grid")]
    pub trim_by_grid: bool,
    /// Skip consecutive duplicate frames.
    #[arg(long = "merge-duplicates")]
    pub merge_duplicates: bool,
}

#[derive(Args, Debug)]
pub struct AseFilterArgs {
    pub file: String,
    /// outline | replace-index | replace-color | invert | brightness-contrast | hue-saturation
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long, default_value_t = 1)]
    pub color: u8,
    #[arg(long, default_value = "outer")]
    pub mode: String,
    #[arg(long, default_value = "square")]
    pub brush: String,
    #[arg(long, default_value_t = 1)]
    pub width: i32,
    #[arg(long)]
    pub from: Option<u8>,
    #[arg(long)]
    pub to: Option<u8>,
    /// Source color `#RRGGBB` / `#RRGGBBAA` for replace-color.
    #[arg(long = "from-color")]
    pub from_color: Option<String>,
    /// Target color for replace-color.
    #[arg(long = "to-color")]
    pub to_color: Option<String>,
    #[arg(long, default_value_t = 0)]
    pub tolerance: u8,
    #[arg(long, default_value_t = 0)]
    pub brightness: i32,
    #[arg(long, default_value_t = 0)]
    pub contrast: i32,
    /// Outline every frame (default: frame 0 only for outline).
    #[arg(long = "all-frames")]
    pub all_frames: bool,
    #[arg(long = "hue", default_value_t = 0)]
    pub hue: i32,
    #[arg(long = "sat", default_value_t = 0)]
    pub sat: i32,
    #[arg(long = "value", default_value_t = 0)]
    pub value: i32,
}

#[derive(Args, Debug)]
pub struct AseOpArgs {
    pub file: String,
    /// flip | rotate90 | flood-fill | crop | canvas-size | trim
    pub name: String,
    #[arg(long, short = 'o')]
    pub out: String,
    /// flip: h|v|horizontal|vertical ; rotate90: cw|ccw
    #[arg(long, default_value = "h")]
    pub axis: String,
    #[arg(long, default_value = "cw")]
    pub dir: String,
    #[arg(long, default_value_t = 0)]
    pub x: i32,
    #[arg(long, default_value_t = 0)]
    pub y: i32,
    #[arg(long)]
    pub w: Option<u16>,
    #[arg(long)]
    pub h: Option<u16>,
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    /// Fill / target color `#RRGGBB` / `#RRGGBBAA` for flood-fill.
    #[arg(long = "to-color")]
    pub to_color: Option<String>,
    #[arg(long, default_value_t = 0)]
    pub tolerance: u8,
    /// canvas-size: place old content at this origin.
    #[arg(long = "ox", default_value_t = 0)]
    pub ox: i32,
    #[arg(long = "oy", default_value_t = 0)]
    pub oy: i32,
}

#[derive(Args, Debug)]
pub struct AseImportArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    /// Asset id for recipe/palette (default: file stem).
    #[arg(long)]
    pub asset: Option<String>,
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseSplitSlicesArgs {
    pub file: String,
    #[arg(long, short = 'o')]
    pub out: String,
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    #[arg(long)]
    pub oneframe: bool,
}

pub fn ase_cmd(ctx: &Ctx, action: AseCmd) -> CmdResult<()> {
    match action {
        AseCmd::Inspect(a) => inspect(ctx, a),
        AseCmd::Convert(a) => convert(ctx, a),
        AseCmd::ListLayers(a) => {
            let doc = open(&a.file, a.oneframe).map_err(map_err)?;
            let text = list_layers_text(&doc);
            if ctx.json {
                println!(
                    "{}",
                    json!({"ok": true, "layers": doc.sprite.layers.iter().map(|l| &l.name).collect::<Vec<_>>()})
                );
            } else if !ctx.quiet {
                println!("{text}");
            }
            Ok(())
        }
        AseCmd::ListTags(a) => {
            let doc = open(&a.file, a.oneframe).map_err(map_err)?;
            let text = list_tags_text(&doc);
            if ctx.json {
                println!("{}", json!({"ok": true, "tags": inspect_json(&doc)["tags"]}));
            } else if !ctx.quiet {
                println!("{text}");
            }
            Ok(())
        }
        AseCmd::ListSlices(a) => {
            let doc = open(&a.file, a.oneframe).map_err(map_err)?;
            let text = list_slices_text(&doc);
            if ctx.json {
                println!("{}", json!({"ok": true, "slices": list_slices_json(&doc)}));
            } else if !ctx.quiet {
                println!("{text}");
            }
            Ok(())
        }
        AseCmd::Sheet(a) => sheet(ctx, a),
        AseCmd::Filter(a) => filter(ctx, a),
        AseCmd::Op(a) => op_cmd(ctx, a),
        AseCmd::SplitSlices(a) => split_slices_cmd(ctx, a),
        AseCmd::Import(a) => import_cmd(ctx, a),
    }
}

fn inspect(ctx: &Ctx, a: AseInspectArgs) -> CmdResult<()> {
    let doc = open(&a.file, a.oneframe).map_err(map_err)?;
    let v = inspect_json(&doc);
    if ctx.json {
        println!("{}", json!({"ok": true, "sprite": v}));
    } else if !ctx.quiet {
        println!(
            "{}  {}x{}  {:?}  frames={}  layers={}  tags={}  slices={}",
            a.file,
            doc.sprite.width,
            doc.sprite.height,
            doc.sprite.color_mode,
            doc.sprite.frame_count(),
            doc.sprite.layers.len(),
            doc.sprite.tags.len(),
            doc.sprite.slices.len()
        );
        for l in &doc.sprite.layers {
            println!("  layer[{}]: {}", l.id.0, l.name);
        }
        for t in &doc.sprite.tags {
            println!("  tag: {} ({}-{})", t.name, t.from, t.to);
        }
        for s in &doc.sprite.slices {
            println!("  slice: {} ({} keys)", s.name, s.keys.len());
        }
    }
    Ok(())
}

fn convert(ctx: &Ctx, a: AseConvertArgs) -> CmdResult<()> {
    let _ = a.batch;
    if a.files.is_empty() {
        return Err(PxlError::usage("ase convert requires at least one input file").into());
    }
    let mut outs = Vec::new();
    for (i, file) in a.files.iter().enumerate() {
        let out = convert_one(ctx, &a, file, i)?;
        outs.extend(out);
    }
    if ctx.json {
        println!("{}", json!({"ok": true, "outs": outs}));
    }
    Ok(())
}

fn convert_one(
    ctx: &Ctx,
    a: &AseConvertArgs,
    file: &str,
    index: usize,
) -> CmdResult<Vec<String>> {
    let mut doc = open(file, a.oneframe).map_err(map_err)?;
    if a.all_layers {
        set_all_layers_visible(&mut doc.sprite, true);
    }
    if let Some(layer) = &a.layer {
        for l in &mut doc.sprite.layers {
            l.flags.visible = l.name == *layer;
        }
        let _ = set_layer_visibility_by_name; // keep import used if inlined above
    }
    if let Some(pal_path) = &a.palette {
        let colors = load_palette_file(Path::new(pal_path)).map_err(map_err)?;
        apply_palette(&mut doc.sprite, colors).map_err(map_err)?;
    }
    if a.trim {
        pxl_ase::ops::trim_sprite(&mut doc.sprite).map_err(map_err)?;
    }
    if let Some(crop) = &a.crop {
        let (x, y, w, h) = parse_crop_rect(crop)?;
        pxl_ase::ops::crop_sprite(&mut doc.sprite, x, y, w, h).map_err(map_err)?;
    }
    let algo = DitherAlgorithm::parse(&a.dithering_algorithm).map_err(map_err)?;
    let matrix = DitherMatrix::parse(&a.dithering_matrix).ok_or_else(|| {
        PxlError::usage(format!(
            "unknown dithering-matrix '{}' (bayer2x2|bayer4x4|bayer8x8)",
            a.dithering_matrix
        ))
    })?;
    if let Some(mode) = &a.color_mode {
        let target = parse_color_mode(mode)?;
        convert_sprite_color_mode_opts(
            &mut doc.sprite,
            target,
            ConvertColorOpts {
                algorithm: algo,
                matrix,
                max_colors: 32,
            },
        )
        .map_err(map_err)?;
    } else if a.palette.is_some() && doc.sprite.color_mode != ColorMode::Indexed {
        // Applying an external palette implies indexed remap when mode omitted.
        convert_sprite_color_mode_opts(
            &mut doc.sprite,
            ColorMode::Indexed,
            ConvertColorOpts {
                algorithm: algo,
                matrix,
                max_colors: 32,
            },
        )
        .map_err(map_err)?;
    }

    let mut frames = select_frames(
        &doc.sprite,
        a.tag.as_deref(),
        a.frame_range.as_deref(),
        if a.tag.is_none() && a.frame_range.is_none() {
            // Single-frame PNG default; GIF/sequences use all unless framed.
            None
        } else {
            None
        },
    )
    .map_err(map_err)?;
    // For plain PNG without tag/range, default to --frame.
    let save_ext = Path::new(&a.save_as)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if a.tag.is_none() && a.frame_range.is_none() && save_ext != "gif" && a.filename_format.is_none()
    {
        frames = vec![a.frame];
    }
    if a.ignore_empty {
        frames.retain(|&f| {
            pxl_ase::render::flatten_frame_rgba(&doc.sprite, f)
                .map(|rgba| !frame_is_empty(&rgba))
                .unwrap_or(true)
        });
        if frames.is_empty() {
            return Err(PxlError::usage("all selected frames empty (--ignore-empty)").into());
        }
    }
    if a.merge_duplicates {
        frames = merge_duplicate_frames(&doc.sprite, &frames).map_err(map_err)?;
        if frames.is_empty() {
            return Err(PxlError::usage("no frames left after --merge-duplicates").into());
        }
    }

    if a.preview {
        if ctx.json {
            println!(
                "{}",
                json!({"ok": true, "preview": true, "file": file, "frames": frames, "save_as": a.save_as})
            );
        } else if !ctx.quiet {
            println!(
                "preview: {} frames {:?} -> {}",
                file, frames, a.save_as
            );
        }
        return Ok(vec![]);
    }

    let mut written = Vec::new();
    if let Some(dir) = &a.split_slices {
        let paths = split_slices_to_dir(&doc.sprite, Path::new(dir), a.frame, a.scale)
            .map_err(map_err)?;
        written.extend(paths);
        if !ctx.quiet && !ctx.json {
            for p in &written {
                println!("wrote {p}");
            }
        }
        return Ok(written);
    }

    let title = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sprite");
    let tag_name = a.tag.clone().unwrap_or_default();
    let layer_name = a.layer.clone().unwrap_or_else(|| "flat".into());

    match save_ext.as_str() {
        "ase" | "aseprite" => {
            let out = resolve_batch_out(&a.save_as, file, index, a.files.len())?;
            ensure_out(ctx, &out)?;
            pxl_ase::io::write_aseprite(&out, &doc.sprite, &Default::default()).map_err(map_err)?;
            written.push(out.display().to_string());
        }
        "gif" => {
            let out = resolve_batch_out(&a.save_as, file, index, a.files.len())?;
            ensure_out(ctx, &out)?;
            let bytes = export_gif(&doc.sprite, &frames, a.scale).map_err(map_err)?;
            std::fs::write(&out, bytes).map_err(|e| PxlError::io(e.to_string()))?;
            written.push(out.display().to_string());
        }
        _ => {
            // PNG: single or sequence via filename-format / multi-frame.
            if frames.len() == 1 && a.filename_format.is_none() && a.files.len() == 1 {
                let out = PathBuf::from(&a.save_as);
                ensure_out(ctx, &out)?;
                let png = export_frame_png(&doc.sprite, frames[0], a.scale).map_err(map_err)?;
                std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
                written.push(out.display().to_string());
            } else {
                let fmt = a
                    .filename_format
                    .clone()
                    .unwrap_or_else(|| {
                        if a.files.len() > 1 {
                            format!(
                                "{}/{{title}}_{{frame}}.png",
                                a.save_as.trim_end_matches('/')
                            )
                        } else {
                            // Derive from save-as stem.
                            let p = Path::new(&a.save_as);
                            let parent = p
                                .parent()
                                .map(|x| x.display().to_string())
                                .unwrap_or_else(|| ".".into());
                            let stem = p
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("frame");
                            format!("{parent}/{stem}_{{frame}}.png")
                        }
                    });
                for &f in &frames {
                    let dur = doc
                        .sprite
                        .frame_durations_ms
                        .get(f as usize)
                        .copied()
                        .unwrap_or(100);
                    let (tagframe, startframe, endframe) = doc
                        .sprite
                        .tags
                        .iter()
                        .find(|t| t.name == tag_name)
                        .map(|t| (f.saturating_sub(t.from), t.from, t.to))
                        .unwrap_or((f, 0, doc.sprite.frame_count().saturating_sub(1) as u16));
                    let name = expand_filename_format_ctx(
                        &fmt,
                        &FilenameFormatCtx {
                            frame: f,
                            tag: &tag_name,
                            layer: &layer_name,
                            title,
                            path: file,
                            duration_ms: dur as u32,
                            tagframe,
                            innertagframe: tagframe,
                            startframe,
                            endframe,
                            ..Default::default()
                        },
                    );
                    let out = PathBuf::from(name);
                    ensure_out(ctx, &out)?;
                    let png = export_frame_png(&doc.sprite, f, a.scale).map_err(map_err)?;
                    std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
                    written.push(out.display().to_string());
                }
            }
        }
    }
    if !ctx.quiet && !ctx.json {
        for p in &written {
            println!("wrote {p}");
        }
    }
    Ok(written)
}

fn resolve_batch_out(save_as: &str, file: &str, index: usize, n: usize) -> CmdResult<PathBuf> {
    if n <= 1 {
        return Ok(PathBuf::from(save_as));
    }
    let p = Path::new(save_as);
    if p.extension().is_none() || p.is_dir() {
        let stem = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out");
        let ext = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        // Prefer save_as extension if present on a template-like path.
        let out_ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(ext);
        Ok(PathBuf::from(save_as).join(format!("{stem}_{index}.{out_ext}")))
    } else {
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out");
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let parent = p.parent().unwrap_or_else(|| Path::new("."));
        Ok(parent.join(format!("{stem}_{index}.{ext}")))
    }
}

fn sheet(ctx: &Ctx, a: AseSheetArgs) -> CmdResult<()> {
    if a.trim_by_grid {
        return Err(PxlError::usage(
            "--trim-by-grid is deferred (M7); use --trim for transparent-border trim",
        )
        .into());
    }
    let mut doc = open(&a.file, a.oneframe).map_err(map_err)?;
    if a.all_layers {
        set_all_layers_visible(&mut doc.sprite, true);
    }
    let mut frames = if a.oneframe {
        vec![0]
    } else {
        select_frames(
            &doc.sprite,
            a.tag.as_deref(),
            a.frame_range.as_deref(),
            None,
        )
        .map_err(map_err)?
    };
    if a.ignore_empty {
        frames.retain(|&f| {
            pxl_ase::render::flatten_frame_rgba(&doc.sprite, f)
                .map(|rgba| !frame_is_empty(&rgba))
                .unwrap_or(true)
        });
    }
    let out = PathBuf::from(&a.out);
    ensure_out(ctx, &out)?;
    let image_name = out
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png")
        .to_string();
    let opts = SheetExportOpts {
        padding: a.padding,
        extrude: a.extrude,
        trim: a.trim,
        merge_duplicates: a.merge_duplicates,
    };
    let (png, js) = export_sheet_png_json_layout_opts(
        &doc.sprite,
        &frames,
        &image_name,
        &a.layout,
        &opts,
    )
    .map_err(map_err)?;
    std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
    let data_path = a.data.unwrap_or_else(|| {
        let mut p = out.clone();
        p.set_extension("json");
        p.display().to_string()
    });
    let data = PathBuf::from(&data_path);
    ensure_out(ctx, &data)?;
    std::fs::write(&data, js).map_err(|e| PxlError::io(e.to_string()))?;
    if ctx.json {
        println!("{}", json!({"ok": true, "out": a.out, "data": data_path}));
    } else if !ctx.quiet {
        println!("wrote {} and {}", a.out, data_path);
    }
    Ok(())
}

fn filter(ctx: &Ctx, a: AseFilterArgs) -> CmdResult<()> {
    let mut doc = open(&a.file, false).map_err(map_err)?;
    match a.name.as_str() {
        "outline" => {
            let mode = pxl_core::OutlineMode::parse(&a.mode)?;
            let brush = pxl_core::Brush::parse(&a.brush)?;
            if a.all_frames {
                pxl_ase::filters::outline_all_frames(
                    &mut doc.sprite,
                    a.color,
                    mode,
                    brush,
                    a.width,
                )
                .map_err(map_err)?;
            } else {
                pxl_ase::filters::outline_flatten(
                    &mut doc.sprite,
                    a.color,
                    mode,
                    brush,
                    a.width,
                )
                .map_err(map_err)?;
            }
        }
        "replace-index" => {
            let from = a.from.ok_or_else(|| PxlError::usage("--from required"))?;
            let to = a.to.ok_or_else(|| PxlError::usage("--to required"))?;
            pxl_ase::filters::replace_index(&mut doc.sprite, from, to).map_err(map_err)?;
        }
        "replace-color" => {
            let from = a
                .from_color
                .as_deref()
                .ok_or_else(|| PxlError::usage("--from-color required"))?;
            let to = a
                .to_color
                .as_deref()
                .ok_or_else(|| PxlError::usage("--to-color required"))?;
            let from = parse_hex(from)?;
            let to = parse_hex(to)?;
            // Ensure RGBA buffers exist for replace.
            if doc.sprite.color_mode != ColorMode::Rgba {
                pxl_ase::color::convert_sprite_color_mode(&mut doc.sprite, ColorMode::Rgba)
                    .map_err(map_err)?;
            }
            pxl_ase::filters::replace_color_rgba_tol(&mut doc.sprite, from, to, a.tolerance)
                .map_err(map_err)?;
        }
        "invert" => {
            pxl_ase::filters::invert(&mut doc.sprite).map_err(map_err)?;
        }
        "brightness-contrast" | "brightness" | "contrast" => {
            pxl_ase::filters::brightness_contrast(&mut doc.sprite, a.brightness, a.contrast)
                .map_err(map_err)?;
        }
        "hue-saturation" | "huesat" | "hue_saturation" => {
            pxl_ase::filters::hue_saturation(&mut doc.sprite, a.hue, a.sat, a.value)
                .map_err(map_err)?;
        }
        other => {
            return Err(PxlError::usage(format!("unknown filter '{other}'"))
                .with_hint(
                    "outline | replace-index | replace-color | invert | brightness-contrast | hue-saturation",
                )
                .into());
        }
    }
    write_filter_out(ctx, &doc, &a.out)
}

fn op_cmd(ctx: &Ctx, a: AseOpArgs) -> CmdResult<()> {
    let mut doc = open(&a.file, false).map_err(map_err)?;
    match a.name.as_str() {
        "flip" => pxl_ase::ops::flip_all_frames(&mut doc.sprite, &a.axis).map_err(map_err)?,
        "rotate90" | "rotate" => {
            pxl_ase::ops::rotate90_all_frames(&mut doc.sprite, &a.dir).map_err(map_err)?
        }
        "flood-fill" | "flood_fill" | "floodfill" => {
            let to = a
                .to_color
                .as_deref()
                .ok_or_else(|| PxlError::usage("--to-color required for flood-fill"))?;
            let fill = parse_hex(to)?;
            pxl_ase::filters::flood_fill_frame(
                &mut doc.sprite,
                a.frame,
                a.x,
                a.y,
                fill,
                a.tolerance,
            )
            .map_err(map_err)?;
        }
        "crop" => {
            let w = a.w.ok_or_else(|| PxlError::usage("--w required for crop"))?;
            let h = a.h.ok_or_else(|| PxlError::usage("--h required for crop"))?;
            pxl_ase::ops::crop_sprite(&mut doc.sprite, a.x, a.y, w, h).map_err(map_err)?;
        }
        "canvas-size" | "canvas_size" => {
            let w = a.w.ok_or_else(|| PxlError::usage("--w required for canvas-size"))?;
            let h = a.h.ok_or_else(|| PxlError::usage("--h required for canvas-size"))?;
            pxl_ase::ops::canvas_size(&mut doc.sprite, w, h, a.ox, a.oy).map_err(map_err)?;
        }
        "trim" => {
            pxl_ase::ops::trim_sprite(&mut doc.sprite).map_err(map_err)?;
        }
        other => {
            return Err(PxlError::usage(format!("unknown op '{other}'"))
                .with_hint("flip | rotate90 | flood-fill | crop | canvas-size | trim")
                .into());
        }
    }
    write_filter_out(ctx, &doc, &a.out)
}

fn import_cmd(ctx: &Ctx, a: AseImportArgs) -> CmdResult<()> {
    let doc = open(&a.file, a.oneframe).map_err(map_err)?;
    let asset = a.asset.clone().unwrap_or_else(|| {
        Path::new(&a.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported")
            .to_string()
    });
    let out = PathBuf::from(&a.out);
    if out.exists() && !ctx.overwrite {
        // allow existing dir only with overwrite; import creates many files
        if !out.is_dir() {
            return Err(PxlError::exists(&out.display().to_string()).into());
        }
    }
    std::fs::create_dir_all(&out).map_err(|e| PxlError::io(e.to_string()))?;
    let report = pxl_ase::import::import_ase_to_dir(
        &doc.sprite,
        Path::new(&a.file),
        &out,
        &asset,
    )
    .map_err(map_err)?;
    if ctx.json {
        println!(
            "{}",
            json!({
                "ok": true,
                "out": a.out,
                "manifest": report.manifest_path.display().to_string(),
                "recipe": report.recipe_path.display().to_string(),
                "palette": report.palette_path.display().to_string(),
                "frames": report.frame_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                "warnings": report.warnings,
            })
        );
    } else if !ctx.quiet {
        println!(
            "imported {} → {} (recipe {}, {} frames)",
            a.file,
            a.out,
            report.recipe_path.display(),
            report.frame_paths.len()
        );
        for w in &report.warnings {
            eprintln!("warning: {w}");
        }
    }
    Ok(())
}

fn parse_crop_rect(s: &str) -> CmdResult<(i32, i32, u16, u16)> {
    let parts: Vec<_> = s.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(PxlError::usage("--crop expects x,y,w,h").into());
    }
    let x: i32 = parts[0]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop x"))?;
    let y: i32 = parts[1]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop y"))?;
    let w: u16 = parts[2]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop w"))?;
    let h: u16 = parts[3]
        .parse()
        .map_err(|_| PxlError::usage("bad --crop h"))?;
    Ok((x, y, w, h))
}

fn split_slices_cmd(ctx: &Ctx, a: AseSplitSlicesArgs) -> CmdResult<()> {
    let doc = open(&a.file, a.oneframe).map_err(map_err)?;
    let paths =
        split_slices_to_dir(&doc.sprite, Path::new(&a.out), a.frame, a.scale).map_err(map_err)?;
    if ctx.json {
        println!("{}", json!({"ok": true, "outs": paths}));
    } else if !ctx.quiet {
        for p in paths {
            println!("wrote {p}");
        }
    }
    Ok(())
}

fn write_filter_out(ctx: &Ctx, doc: &pxl_ase::doc::AseDoc, out_s: &str) -> CmdResult<()> {
    let out = PathBuf::from(out_s);
    ensure_out(ctx, &out)?;
    match out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "ase" | "aseprite" => {
            pxl_ase::io::write_aseprite(&out, &doc.sprite, &Default::default()).map_err(map_err)?;
        }
        _ => {
            let png = export_frame_png(&doc.sprite, 0, 1).map_err(map_err)?;
            std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
        }
    }
    if ctx.json {
        println!("{}", json!({"ok": true, "out": out_s}));
    } else if !ctx.quiet {
        println!("wrote {out_s}");
    }
    Ok(())
}

fn parse_color_mode(s: &str) -> CmdResult<ColorMode> {
    match s.to_ascii_lowercase().as_str() {
        "rgba" | "rgb" | "32" => Ok(ColorMode::Rgba),
        "indexed" | "index" | "8" => Ok(ColorMode::Indexed),
        "grayscale" | "gray" | "grey" | "16" => Ok(ColorMode::Grayscale),
        other => Err(PxlError::usage(format!("unknown color-mode '{other}'"))
            .with_hint("rgba, indexed, or grayscale")
            .into()),
    }
}

fn ensure_out(ctx: &Ctx, path: &Path) -> CmdResult<()> {
    if path.exists() && !ctx.overwrite {
        return Err(PxlError::exists(&path.display().to_string()).into());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| PxlError::io(e.to_string()))?;
        }
    }
    Ok(())
}

fn map_err(e: impl std::fmt::Display) -> PxlError {
    PxlError::io(e.to_string())
}

// silence unused import if layer filter inlined
#[allow(dead_code)]
fn _keep_frames_for_tag() {
    let _ = frames_for_tag;
}
