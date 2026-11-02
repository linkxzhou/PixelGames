//! step-500-engine: asset-package -> browser bundle and deterministic playtest.

mod build;
mod check;
mod playtest;
mod slice;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const STEP_ID: &str = "step-500-engine";
pub const CONTRACT_ID: &str = "engine/v4";
pub const CONTRACT_JSON: &str = include_str!("../../contracts/engine.schema.json");
pub const ENGINE_JSON: &str = include_str!("../engine.json");

#[derive(Debug, Parser)]
#[command(
    name = "step-500",
    version = VERSION,
    about = "Engine step: asset-package -> miniquad wasm + JavaScript browser game",
    disable_colored_help = true
)]
pub struct Cli {
    /// Print this step's contract JSON Schema and exit.
    #[arg(long)]
    pub contract: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate an asset-package against the engine contract.
    Check(CheckArgs),
    /// Vertical-slice verdict: PROCEED / PIVOT / KILL / NOT ASSESSED.
    Slice(SliceArgs),
    /// Print the engine capability sheet and the contract.
    Contract,
    /// Build a static browser bundle from prebuilt runtime artifacts.
    Build(BuildArgs),
    /// Run deterministic headless replay and optional browser acceptance.
    Playtest(PlaytestArgs),
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    /// Package directory produced by `step-400 run`.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// Where to write engine-check.json and step.json; defaults to --in.
    #[arg(short, long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// all | slice. In slice mode, tiles outside the delivered set are warnings.
    #[arg(long, default_value = "all")]
    pub scope: String,
    /// Engine capability sheet to validate; defaults to the embedded engine/v4.
    #[arg(long, value_name = "FILE")]
    pub engine: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct SliceArgs {
    /// Workspace root containing the step .slice/ artifacts.
    #[arg(long, value_name = "DIR")]
    pub work: PathBuf,
    /// Where to write slice-report.json and step.json.
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct BuildArgs {
    /// Asset-package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// JavaScript game source containing main.js; omit for the generic asset showcase.
    #[arg(long, value_name = "DIR")]
    pub game: Option<PathBuf>,
    /// Output work/500-engine directory.
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
    /// Runtime assets; defaults to the source runtime-js directory.
    #[arg(long, value_name = "DIR")]
    pub runtime_dir: Option<PathBuf>,
    /// Runtime implementation mode. JS is the default.
    #[arg(long, value_parser = ["js", "rust"])]
    pub mode: Option<String>,
    /// Replace an existing output after the new build succeeds.
    #[arg(long)]
    pub overwrite: bool,
}

impl BuildArgs {
    pub fn mode(&self) -> &str {
        self.mode.as_deref().unwrap_or("js")
    }

    pub fn mode_was_default(&self) -> bool {
        self.mode.is_none()
    }
}

#[derive(Debug, clap::Args)]
pub struct PlaytestArgs {
    /// Directory containing engine-build.json and web/.
    #[arg(long = "in", value_name = "DIR")]
    pub in_dir: PathBuf,
    /// headless | browser | visual | all.
    #[arg(long, default_value = "all")]
    pub mode: String,
    /// Playtest output; defaults to <in>/playtest.
    #[arg(short, long, value_name = "DIR")]
    pub out: Option<PathBuf>,
}

pub fn entry() -> i32 {
    let cli = Cli::parse();
    if cli.contract {
        println!("{CONTRACT_JSON}");
        return 0;
    }
    let Some(command) = cli.command else {
        let _ = Cli::command().print_help();
        return 2;
    };
    let result = match command {
        Command::Check(args) => check::check(args),
        Command::Slice(args) => slice::slice(args),
        Command::Contract => {
            println!("{ENGINE_JSON}");
            Ok(0)
        }
        Command::Build(args) => build::build(args),
        Command::Playtest(args) => playtest::playtest(args),
    };
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error [{}]: {}", err.code.as_str(), err.message);
            if let Some(hint) = &err.hint {
                eprintln!("hint: {hint}");
            }
            err.code.exit_code()
        }
    }
}
