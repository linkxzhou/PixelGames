//! GD edit CLI handlers（非官方）.
use std::path::PathBuf;

use pxl_ase::doc::{BlendMode, Grid, TagDirection};
use pxl_ase::edit::{
    cel_clear, cel_move, cel_set_opacity, cel_set_position, cel_unlink, flatten_layers, frame_add,
    frame_copy, frame_remove, frame_set_duration, layer_add, layer_remove, layer_rename, layer_set,
    palette_sort, sprite_set_attrs, tag_add, tag_remove, tag_rename, tag_set, SpriteAttrPatch,
};
use pxl_ase::{export_tileset_png, open};
use pxl_core::PxlError;
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;

use super::edit_args::*;
use super::util::{ensure_out, map_err};

fn save(ctx: &Ctx, file: &str, out: Option<&str>, sprite: &pxl_ase::doc::Sprite) -> CmdResult<()> {
    let path = PathBuf::from(out.unwrap_or(file));
    ensure_out(ctx, &path)?;
    pxl_ase::io::write_aseprite(&path, sprite, &Default::default()).map_err(map_err)?;
    if ctx.json {
        println!("{}", json!({"ok": true, "out": path.display().to_string()}));
    } else if !ctx.quiet {
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn parse_dir(s: &str) -> CmdResult<TagDirection> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "forward" | "f" => TagDirection::Forward,
        "reverse" | "r" => TagDirection::Reverse,
        "pingpong" | "pp" => TagDirection::PingPong,
        "pingpong_reverse" | "ppr" => TagDirection::PingPongReverse,
        other => return Err(PxlError::usage(format!("unknown tag direction '{other}'")).into()),
    })
}

fn parse_blend(s: &str) -> CmdResult<BlendMode> {
    BlendMode::from_name(s).ok_or_else(|| PxlError::usage(format!("unknown blend '{s}'")).into())
}

pub(crate) fn run_frame(ctx: &Ctx, cmd: FrameCmd) -> CmdResult<()> {
    match cmd {
        FrameCmd::Add(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            frame_add(&mut doc.sprite, a.at, a.duration).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        FrameCmd::Remove(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            let _ = frame_remove(&mut doc.sprite, a.index).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        FrameCmd::Copy(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            frame_copy(&mut doc.sprite, a.from, a.to).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        FrameCmd::SetDuration(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            frame_set_duration(&mut doc.sprite, a.index, a.ms).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_tag(ctx: &Ctx, cmd: TagCmd) -> CmdResult<()> {
    match cmd {
        TagCmd::Add(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            tag_add(&mut doc.sprite, a.name, a.from, a.to, parse_dir(&a.direction)?, a.repeat)
                .map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        TagCmd::Remove(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            tag_remove(&mut doc.sprite, &a.name).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        TagCmd::Rename(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            tag_rename(&mut doc.sprite, &a.old, &a.new).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        TagCmd::Set(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            let dir = a.direction.as_deref().map(parse_dir).transpose()?;
            tag_set(&mut doc.sprite, &a.name, a.from, a.to, dir, a.repeat).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_layer(ctx: &Ctx, cmd: LayerCmd) -> CmdResult<()> {
    match cmd {
        LayerCmd::Add(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            layer_add(&mut doc.sprite, a.name).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        LayerCmd::Remove(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            layer_remove(&mut doc.sprite, &a.name).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        LayerCmd::Rename(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            layer_rename(&mut doc.sprite, &a.old, &a.new).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        LayerCmd::Set(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            let blend = a.blend.as_deref().map(parse_blend).transpose()?;
            layer_set(&mut doc.sprite, &a.name, a.opacity, blend, a.visible).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_cel(ctx: &Ctx, cmd: CelCmd) -> CmdResult<()> {
    match cmd {
        CelCmd::Move(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            cel_move(&mut doc.sprite, &a.layer, a.from, a.to).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        CelCmd::Unlink(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            cel_unlink(&mut doc.sprite, &a.layer, a.frame).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        CelCmd::Clear(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            cel_clear(&mut doc.sprite, &a.layer, a.frame).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
        CelCmd::Set(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            if let (Some(x), Some(y)) = (a.x, a.y) {
                cel_set_position(&mut doc.sprite, &a.layer, a.frame, x, y).map_err(map_err)?;
            } else if a.x.is_some() || a.y.is_some() {
                return Err(PxlError::usage("cel set needs both --x and --y").into());
            }
            if let Some(o) = a.opacity {
                cel_set_opacity(&mut doc.sprite, &a.layer, a.frame, o).map_err(map_err)?;
            }
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_flatten(ctx: &Ctx, a: FlattenArgs) -> CmdResult<()> {
    let mut doc = open(&a.file, false).map_err(map_err)?;
    flatten_layers(&mut doc.sprite).map_err(map_err)?;
    save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
}

pub(crate) fn run_sprite(ctx: &Ctx, cmd: SpriteCmd) -> CmdResult<()> {
    match cmd {
        SpriteCmd::Set(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            let mut patch = SpriteAttrPatch {
                transparent_index: a.transparent_index,
                pixel_ratio: None,
                grid: None,
            };
            if let Some(pr) = &a.pixel_ratio {
                let parts: Vec<_> = pr.split(&[':', 'x', ','][..]).collect();
                if parts.len() != 2 {
                    return Err(PxlError::usage("pixel-ratio expects W:H").into());
                }
                let w: u8 = parts[0].parse().map_err(|_| PxlError::usage("bad pixel-ratio w"))?;
                let h: u8 = parts[1].parse().map_err(|_| PxlError::usage("bad pixel-ratio h"))?;
                patch.pixel_ratio = Some((w, h));
            }
            if let Some(g) = &a.grid {
                let p: Vec<_> = g.split(',').collect();
                if p.len() != 4 {
                    return Err(PxlError::usage("grid expects x,y,w,h").into());
                }
                patch.grid = Some(Grid {
                    x: p[0].parse().map_err(|_| PxlError::usage("bad grid x"))?,
                    y: p[1].parse().map_err(|_| PxlError::usage("bad grid y"))?,
                    w: p[2].parse().map_err(|_| PxlError::usage("bad grid w"))?,
                    h: p[3].parse().map_err(|_| PxlError::usage("bad grid h"))?,
                });
            }
            sprite_set_attrs(&mut doc.sprite, patch).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_palette(ctx: &Ctx, cmd: PaletteCmd) -> CmdResult<()> {
    match cmd {
        PaletteCmd::Sort(a) => {
            let mut doc = open(&a.file, false).map_err(map_err)?;
            palette_sort(&mut doc.sprite, &a.by, a.keep_zero).map_err(map_err)?;
            save(ctx, &a.file, a.out.as_deref(), &doc.sprite)
        }
    }
}

pub(crate) fn run_export_tileset(ctx: &Ctx, a: ExportTilesetArgs) -> CmdResult<()> {
    let doc = open(&a.file, false).map_err(map_err)?;
    let png = export_tileset_png(&doc.sprite, a.index).map_err(map_err)?;
    let out = PathBuf::from(&a.out);
    ensure_out(ctx, &out)?;
    std::fs::write(&out, png).map_err(|e| PxlError::io(e.to_string()))?;
    if ctx.json {
        println!("{}", json!({"ok": true, "out": a.out}));
    } else if !ctx.quiet {
        println!("wrote {}", a.out);
    }
    Ok(())
}
