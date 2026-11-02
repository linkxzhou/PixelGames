//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).

use std::path::{Path, PathBuf};

use pxl_ase::doc::ColorMode;
use pxl_ase::{
    export_frame_png,
    export_sheet_png_json_layout_opts, frame_is_empty,
    open, select_frames_opts, set_all_layers_visible,
    split_slices_to_dir, SheetExportOpts,
};
use pxl_ase::batch::run_batch_argv;
use pxl_ase::script::{run_script_file, ScriptOptions};
use pxl_core::{parse_hex, PxlError};
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;


use super::args::*;
use super::util::{ensure_out, map_err};

pub(crate) fn sheet(ctx: &Ctx, a: AseSheetArgs) -> CmdResult<()> {
    let mut doc = open(&a.file, a.oneframe).map_err(map_err)?;
    if a.all_layers {
        set_all_layers_visible(&mut doc.sprite, true);
    }
    if a.trim_by_grid {
        pxl_ase::ops::trim_sprite_by_grid(&mut doc.sprite).map_err(map_err)?;
    }
    let mut frames = if a.oneframe {
        vec![0]
    } else {
        select_frames_opts(
            &doc.sprite,
            a.tag.as_deref(),
            a.frame_range.as_deref(),
            None,
            a.play_subtags,
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

pub(crate) fn filter(ctx: &Ctx, a: AseFilterArgs) -> CmdResult<()> {
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

pub(crate) fn op_cmd(ctx: &Ctx, a: AseOpArgs) -> CmdResult<()> {
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

pub(crate) fn import_cmd(ctx: &Ctx, a: AseImportArgs) -> CmdResult<()> {
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


pub(crate) fn run_cmd(ctx: &Ctx, a: AseRunArgs) -> CmdResult<()> {
    let mut args = a.args;
    // Allow `ase run -- ...` where clap may keep a leading `--`.
    if args.first().map(|s| s.as_str()) == Some("--") {
        args.remove(0);
    }
    let (st, _events) = run_batch_argv(&args, None).map_err(map_err)?;
    if ctx.json {
        println!("{}", json!({"ok": true, "written": st.written}));
    } else if !ctx.quiet {
        for w in &st.written {
            println!("wrote {w}");
        }
    }
    Ok(())
}

pub(crate) fn script_cmd(ctx: &Ctx, a: AseScriptArgs) -> CmdResult<()> {
    let workdir = a
        .workdir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut params = Vec::new();
    for p in &a.script_param {
        let (k, v) = p
            .split_once('=')
            .ok_or_else(|| PxlError::usage("--script-param expects key=value"))?;
        params.push((k.to_string(), v.to_string()));
    }
    let opts = ScriptOptions {
        workdir,
        allow_fs_escape: a.allow_fs,
        params,
        ..Default::default()
    };
    let path = PathBuf::from(&a.path);
    let _ = run_script_file(&path, &opts).map_err(map_err)?;
    if ctx.json {
        println!("{}", json!({"ok": true, "script": a.path}));
    } else if !ctx.quiet {
        println!("script ok: {}", a.path);
    }
    Ok(())
}


pub(crate) fn split_slices_cmd(ctx: &Ctx, a: AseSplitSlicesArgs) -> CmdResult<()> {
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

pub(crate) fn write_filter_out(ctx: &Ctx, doc: &pxl_ase::doc::AseDoc, out_s: &str) -> CmdResult<()> {
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

