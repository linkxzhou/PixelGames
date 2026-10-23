//! Subcommand implementations. File IO stays here; pixel work stays in the libraries.

use pxl_core::PxlError;

use crate::error::CmdResult;
use crate::util::Ctx;

mod animation;
mod args;
mod build;
mod catalog;
mod document;
mod generate;
mod inspect;
mod operations;
mod palette;
mod qa_input;
mod render;
mod sheet;
mod sprite;

use animation::anim_cmd;
pub use args::{
    AnimArgs, BuildArgs, GenArgs, InspectArgs, OpArgs, PaletteCmd, RenderArgs, SheetArgs, TileCmd,
    ValidateArgs,
};
use build::build_cmd;
use catalog::{list_cmd, schema_cmd};
use generate::gen;
use inspect::{inspect_cmd, validate_cmd};
use operations::{op_cmd, tile_cmd};
use palette::palette_cmd;
use render::render_cmd;
use sheet::sheet_cmd;

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
