//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use pxl_ase::color::convert_sprite_color_mode;
use pxl_ase::doc::ColorMode;
use pxl_ase::{
    export_frame_png, export_sheet_png_json, frames_for_tag, inspect_json, list_layers_text,
    list_tags_text, open,
};
use pxl_core::PxlError;
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;

#[derive(Subcommand, Debug)]
pub enum AseCmd {
    /// Print sprite size, color mode, layers, frames, and tags.
    Inspect(AseInspectArgs),
    /// Flatten and export (PNG). Compatible with a small `--save-as` subset.
    Convert(AseConvertArgs),
    /// List layer names (hierarchy indented).
    ListLayers(AseFileArgs),
    /// List animation tags.
    ListTags(AseFileArgs),
    /// Pack frames into an indexed spritesheet PNG + JSON.
    Sheet(AseSheetArgs),
    /// Apply outline filter then save `.aseprite` or PNG.
    Filter(AseFilterArgs),
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
    pub file: String,
    /// Output path (`.png` or `.aseprite`).
    #[arg(long = "save-as", short = 'o')]
    pub save_as: String,
    #[arg(long)]
    pub batch: bool,
    #[arg(long)]
    pub preview: bool,
    #[arg(long)]
    pub oneframe: bool,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub scale: u32,
    /// rgba | indexed | grayscale (optional convert before export).
    #[arg(long = "color-mode")]
    pub color_mode: Option<String>,
    /// Frame index when not using --tag (default 0).
    #[arg(long, default_value_t = 0)]
    pub frame: u16,
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
    #[arg(long)]
    pub oneframe: bool,
}

#[derive(Args, Debug)]
pub struct AseFilterArgs {
    pub file: String,
    /// Currently: `outline` or `replace-index`.
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
        AseCmd::Sheet(a) => sheet(ctx, a),
        AseCmd::Filter(a) => filter(ctx, a),
    }
}

fn inspect(ctx: &Ctx, a: AseInspectArgs) -> CmdResult<()> {
    let doc = open(&a.file, a.oneframe).map_err(map_err)?;
    let v = inspect_json(&doc);
    if ctx.json {
        println!("{}", json!({"ok": true, "sprite": v}));
    } else if !ctx.quiet {
        println!(
            "{}  {}x{}  {:?}  frames={}  layers={}  tags={}",
            a.file,
            doc.sprite.width,
            doc.sprite.height,
            doc.sprite.color_mode,
            doc.sprite.frame_count(),
            doc.sprite.layers.len(),
            doc.sprite.tags.len()
        );
        for l in &doc.sprite.layers {
            println!("  layer[{}]: {}", l.id.0, l.name);
        }
        for t in &doc.sprite.tags {
            println!("  tag: {} ({}-{})", t.name, t.from, t.to);
        }
    }
    Ok(())
}

fn convert(ctx: &Ctx, a: AseConvertArgs) -> CmdResult<()> {
    let _ = a.batch; // accepted / ignored (always headless)
    let mut doc = open(&a.file, a.oneframe).map_err(map_err)?;
    if let Some(mode) = &a.color_mode {
        let target = parse_color_mode(mode)?;
        convert_sprite_color_mode(&mut doc.sprite, target).map_err(map_err)?;
    }
    let frames = frames_for_tag(&doc.sprite, a.tag.as_deref()).map_err(map_err)?;
    let frame = if a.tag.is_some() {
        *frames.first().ok_or_else(|| PxlError::usage("tag has no frames"))?
    } else {
        a.frame
    };
    if a.preview {
        if ctx.json {
            println!(
                "{}",
                json!({"ok": true, "preview": true, "frame": frame, "save_as": a.save_as})
            );
        } else if !ctx.quiet {
            println!("preview: would write frame {frame} -> {}", a.save_as);
        }
        return Ok(());
    }
    let out = PathBuf::from(&a.save_as);
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
            let png = export_frame_png(&doc.sprite, frame, a.scale).map_err(map_err)?;
            std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
        }
    }
    if ctx.json {
        println!("{}", json!({"ok": true, "out": a.save_as, "frame": frame}));
    } else if !ctx.quiet {
        println!("wrote {}", a.save_as);
    }
    Ok(())
}

fn sheet(ctx: &Ctx, a: AseSheetArgs) -> CmdResult<()> {
    let doc = open(&a.file, a.oneframe).map_err(map_err)?;
    let frames = if a.oneframe {
        vec![0]
    } else {
        frames_for_tag(&doc.sprite, a.tag.as_deref()).map_err(map_err)?
    };
    let out = PathBuf::from(&a.out);
    ensure_out(ctx, &out)?;
    let image_name = out
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png")
        .to_string();
    let (png, js) = export_sheet_png_json(&doc.sprite, &frames, &image_name).map_err(map_err)?;
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
            pxl_ase::filters::outline_flatten(&mut doc.sprite, a.color, mode, brush, a.width)
                .map_err(map_err)?;
        }
        "replace-index" => {
            let from = a.from.ok_or_else(|| PxlError::usage("--from required"))?;
            let to = a.to.ok_or_else(|| PxlError::usage("--to required"))?;
            pxl_ase::filters::replace_index(&mut doc.sprite, from, to).map_err(map_err)?;
        }
        other => {
            return Err(PxlError::usage(format!("unknown filter '{other}'"))
                .with_hint("outline or replace-index")
                .into());
        }
    }
    let out = PathBuf::from(&a.out);
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
        println!("{}", json!({"ok": true, "out": a.out}));
    } else if !ctx.quiet {
        println!("wrote {}", a.out);
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
