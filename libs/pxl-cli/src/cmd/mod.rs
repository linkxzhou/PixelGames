//! Subcommand implementations. File IO stays here; pixel work stays in the libraries.

use pxl_core::PxlError;

use crate::error::CmdResult;
use crate::util::Ctx;

mod ase;
mod animation;
mod args;
mod ascii;
mod audio;
mod baseline;
mod build;
mod catalog;
mod dependencies;
mod document;
mod enhance;
mod generate;
mod inspect;
mod operations;
mod palette;
mod pixelize;
mod plan;
mod prototype;
mod qa_input;
mod scene;
mod render;
mod shapes;
mod sheet;
mod sprite;
mod sprite_build;
mod svg;
mod visual;
mod visual_artifacts;

use ase::ase_cmd;
pub use ase::AseCmd;
use animation::anim_cmd;
pub use args::{
    AnimArgs, AsciiCmd, AudioCmd, BuildArgs, EnhanceArgs, GenArgs, InspectArgs, OpArgs, PaletteCmd,
    PixelizeArgs, PlanArgs, ProfileArgs, PrototypeCmd, RenderArgs, SceneCmd, ShapesCmd, SheetArgs,
    SpriteCmd, SvgCmd, TileCmd, ValidateArgs,
};
use ascii::ascii_cmd;
use audio::audio_cmd;
use build::build_cmd;
use catalog::{list_cmd, schema_cmd};
use enhance::enhance_cmd;
use generate::gen;
use inspect::{inspect_cmd, validate_cmd};
use operations::{op_cmd, tile_cmd};
use palette::palette_cmd;
use pixelize::pixelize_cmd;
use plan::plan_cmd;
use prototype::prototype_cmd;
use render::render_cmd;
use scene::scene_cmd;
use shapes::shapes_cmd;
use sheet::sheet_cmd;
use sprite_build::{profile_show, sprite_build};
use svg::svg_cmd;

pub fn dispatch(ctx: &Ctx, cmd: super::Cmd) -> CmdResult<()> {
    use super::Cmd::*;
    match cmd {
        Ase { action } => ase_cmd(ctx, action),
        Gen(a) => gen(ctx, a),
        Ascii { action } => ascii_cmd(ctx, action),
        Op(a) => op_cmd(ctx, a),
        Anim(a) => anim_cmd(ctx, a),
        Palette { action } => palette_cmd(ctx, action),
        Tile { action } => tile_cmd(ctx, action),
        Sheet(a) => sheet_cmd(ctx, a),
        Shapes { action } => shapes_cmd(ctx, action),
        Svg { action } => svg_cmd(ctx, action),
        Sprite { action } => match action {
            SpriteCmd::Build(a) => sprite_build(ctx, a),
        },
        Prototype { action } => prototype_cmd(ctx, action),
        Scene { action } => scene_cmd(ctx, action),
        Audio { action } => audio_cmd(ctx, action),
        Pixelize(a) => pixelize_cmd(ctx, a),
        Profile(a) => profile_show(ctx, a),
        Render(a) => render_cmd(ctx, a),
        Enhance(a) => enhance_cmd(ctx, a),
        Inspect(a) => inspect_cmd(ctx, a),
        Validate(a) => validate_cmd(ctx, a),
        Build(a) => build_cmd(ctx, a),
        Plan(a) => plan_cmd(ctx, a),
        Schema { kind } => schema_cmd(ctx, &kind),
        List { kind } => list_cmd(ctx, &kind),
        Mcp => Err(PxlError::usage("pxl mcp is not implemented")
            .with_hint(
                "M2 will add a stdio MCP server. Drive pxl with --json from the shell for now",
            )
            .into()),
    }
}
