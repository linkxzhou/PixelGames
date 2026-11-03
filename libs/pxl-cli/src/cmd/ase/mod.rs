//! `step-300 ase` — headless Aseprite-aligned batch subset (v8.0).


use clap::Subcommand;
use pxl_ase::{
    inspect_json,
    list_layers_text, list_slices_json, list_slices_text, list_tags_text,
    open,
};
use serde_json::json;

use crate::error::CmdResult;
use crate::util::Ctx;


mod args;
mod gc_flags;
mod convert;
mod ops;
mod edit_args;
mod edit_cmd;
mod util;

pub use args::*;
use util::map_err;

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
    /// Left-to-right Aseprite-like argv subset (`ase run -- file.aseprite --save-as out.png`).
    Run(AseRunArgs),
    /// Run a Rhai script (`ase script path.rhai --script-param k=v`).
    Script(AseScriptArgs),
    /// 非官方：frame CRUD
    #[command(subcommand)]
    Frame(edit_args::FrameCmd),
    /// 非官方：tag CRUD
    #[command(subcommand)]
    Tag(edit_args::TagCmd),
    /// 非官方：layer CRUD
    #[command(subcommand)]
    Layer(edit_args::LayerCmd),
    /// 非官方：cel move/unlink/clear/set
    #[command(subcommand)]
    Cel(edit_args::CelCmd),
    /// 非官方：flatten layers write-back
    Flatten(edit_args::FlattenArgs),
    /// 非官方：sprite attrs
    #[command(subcommand)]
    Sprite(edit_args::SpriteCmd),
    /// 非官方：palette sort
    #[command(subcommand)]
    Palette(edit_args::PaletteCmd),
    /// Export tileset atlas PNG (GE-5)
    ExportTileset(edit_args::ExportTilesetArgs),
}


pub fn ase_cmd(ctx: &Ctx, action: AseCmd) -> CmdResult<()> {
    match action {
        AseCmd::Inspect(a) => convert::inspect(ctx, a),
        AseCmd::Convert(a) => convert::convert(ctx, a),
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
        AseCmd::Sheet(a) => ops::sheet(ctx, a),
        AseCmd::Filter(a) => ops::filter(ctx, a),
        AseCmd::Op(a) => ops::op_cmd(ctx, a),
        AseCmd::SplitSlices(a) => ops::split_slices_cmd(ctx, a),
        AseCmd::Import(a) => ops::import_cmd(ctx, a),
        AseCmd::Run(a) => ops::run_cmd(ctx, a),
        AseCmd::Script(a) => ops::script_cmd(ctx, a),
        AseCmd::Frame(c) => edit_cmd::run_frame(ctx, c),
        AseCmd::Tag(c) => edit_cmd::run_tag(ctx, c),
        AseCmd::Layer(c) => edit_cmd::run_layer(ctx, c),
        AseCmd::Cel(c) => edit_cmd::run_cel(ctx, c),
        AseCmd::Flatten(a) => edit_cmd::run_flatten(ctx, a),
        AseCmd::Sprite(c) => edit_cmd::run_sprite(ctx, c),
        AseCmd::Palette(c) => edit_cmd::run_palette(ctx, c),
        AseCmd::ExportTileset(a) => edit_cmd::run_export_tileset(ctx, a),
    }
}
