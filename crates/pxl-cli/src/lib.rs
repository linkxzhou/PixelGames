//! `pxl` command implementation. The binary is a thin wrapper around [`entry`].

mod cmd;
mod error;
mod util;

use clap::{Parser, Subcommand};
use pxl_core::PXL_VERSION;

pub use error::CmdError;

#[derive(Parser)]
#[command(
    name = "pxl",
    version = PXL_VERSION,
    about = "Deterministic pixel-art asset CLI",
    disable_colored_help = true,
    disable_version_flag = false
)]
struct Cli {
    /// Print one JSON object on stdout. Logs go to stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Override the seed stored in the input or recipe.
    #[arg(long, global = true)]
    seed: Option<u64>,
    /// Suppress human-readable stdout.
    #[arg(long, global = true)]
    quiet: bool,
    /// Treat warnings as errors.
    #[arg(long, global = true)]
    strict: bool,
    /// Require this pxl version (prefix match, e.g. 0.1).
    #[arg(long, global = true, value_name = "VER")]
    pxl_version_lock: Option<String>,
    /// Replace an existing output file. Off by default.
    #[arg(long, global = true)]
    overwrite: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a static frame.
    Gen(cmd::GenArgs),
    /// Apply one or more transform ops.
    Op(cmd::OpArgs),
    /// Expand an animation macro or an explicit frame list.
    Anim(cmd::AnimArgs),
    /// Show or edit palettes.
    Palette {
        #[command(subcommand)]
        action: cmd::PaletteCmd,
    },
    /// Tile generators.
    Tile {
        #[command(subcommand)]
        action: cmd::TileCmd,
    },
    /// Pack documents into an indexed spritesheet plus Aseprite JSON.
    Sheet(cmd::SheetArgs),
    /// Render a PNG or GIF preview.
    Render(cmd::RenderArgs),
    /// Print size, colors, bounds, and the text matrix.
    Inspect(cmd::InspectArgs),
    /// Run programmatic QA.
    Validate(cmd::ValidateArgs),
    /// Run a recipe: generate, animate, pack, preview, validate.
    Build(cmd::BuildArgs),
    /// Print a JSON Schema.
    Schema { kind: String },
    /// List generators, ops, macros, templates, or palettes.
    List { kind: String },
    /// stdio MCP server (M2).
    Mcp,
}

pub fn entry() -> i32 {
    let json_mode = std::env::args().any(|a| a == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return usage_exit(err, json_mode),
    };
    let ctx = util::Ctx {
        json: cli.json,
        seed: cli.seed,
        quiet: cli.quiet,
        strict: cli.strict,
        lock: cli.pxl_version_lock,
        overwrite: cli.overwrite,
    };
    match cmd::dispatch(&ctx, cli.cmd) {
        Ok(()) => 0,
        Err(err) => {
            util::emit_err(&err, ctx.json);
            err.code().exit_code()
        }
    }
}

fn usage_exit(err: clap::Error, json_mode: bool) -> i32 {
    use clap::error::ErrorKind::{DisplayHelp, DisplayVersion};
    if matches!(err.kind(), DisplayHelp | DisplayVersion) {
        let _ = err.print();
        return 0;
    }
    if json_mode {
        let message = err.to_string().replace('\n', " ");
        println!(
            "{}",
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "E_USAGE",
                    "message": message.trim(),
                    "hint": "run `pxl <command> --help` for the expected arguments"
                }
            })
        );
        2
    } else {
        err.exit();
    }
}
