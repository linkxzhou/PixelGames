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
mod convert;
mod ops;
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
    }
}
