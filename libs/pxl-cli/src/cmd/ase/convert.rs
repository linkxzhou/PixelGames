//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).

use std::path::{Path, PathBuf};

use pxl_art::quantize::DitherMatrix;
use pxl_ase::color::{
    apply_palette, convert_sprite_color_mode_opts, load_palette_file, ConvertColorOpts,
    DitherAlgorithm,
};
use pxl_ase::doc::ColorMode;
use pxl_ase::{
    expand_filename_format_ctx, export_frame_png, export_gif, frame_is_empty, inspect_json, merge_duplicate_frames,
    open, select_frames_opts, set_all_layers_visible, set_layer_visibility_by_name,
    split_slices_to_dir, FilenameFormatCtx,
};
use pxl_core::PxlError;
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;


use super::args::*;
use super::util::{ensure_out, map_err, parse_color_mode, parse_crop_rect};

pub(crate) fn inspect(ctx: &Ctx, a: AseInspectArgs) -> CmdResult<()> {
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

pub(crate) fn convert(ctx: &Ctx, a: AseConvertArgs) -> CmdResult<()> {
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

pub(crate) fn convert_one(
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
    if a.trim_by_grid {
        pxl_ase::ops::trim_sprite_by_grid(&mut doc.sprite).map_err(map_err)?;
    } else if a.trim {
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

    let mut frames = select_frames_opts(
        &doc.sprite,
        a.tag.as_deref(),
        a.frame_range.as_deref(),
        if a.tag.is_none() && a.frame_range.is_none() {
            // Single-frame PNG default; GIF/sequences use all unless framed.
            None
        } else {
            None
        },
        a.play_subtags,
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

pub(crate) fn resolve_batch_out(save_as: &str, file: &str, index: usize, n: usize) -> CmdResult<PathBuf> {
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
