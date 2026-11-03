//! step-400-package: build outputs -> asset-package.
//!
//! M4 (plan/v3.0/pxl-step-pipeline-plan.md §4.5). Hard rule: no pixel
//! arithmetic here — only reading files, hashing, and comparing. Decoding
//! lives in step-300 (pxl-qa / pxl-export).

mod audio;
mod catalog;
mod run;
mod scenes;
mod visual;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const STEP_ID: &str = "step-400-package";
pub const CONTRACT_ID: &str = "asset-package/v4";

/// The contract this step publishes: `steps/contracts/asset-package.schema.json`.
pub const CONTRACT_JSON: &str = include_str!("../../contracts/asset-package.schema.json");

#[derive(Debug, Parser)]
#[command(
    name = "step-400",
    version = VERSION,
    about = "Package step: build outputs -> asset-package (deterministic)",
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
    /// Coverage check, content addressing, packaging.
    Run(RunArgs),
    /// Recompute digests and verify the package self-consistently.
    Check(CheckArgs),
    /// Approve a revision by digest; APPROVED / NEEDS REVISION / REJECTED.
    Approve(ApproveArgs),
    /// Report the last run state from step.json.
    Status(StatusArgs),
}

#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// step-300 plan output (index.json + per-recipe directories).
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// asset-requirements.json from step-100 (or hand-written).
    #[arg(long, value_name = "FILE")]
    pub requirements: PathBuf,
    /// Optional audio-requirements.json; defaults to the sibling of --requirements.
    #[arg(long, value_name = "FILE")]
    pub audio_requirements: Option<PathBuf>,
    /// Package output directory.
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
    /// Level definitions directory to reference from the package.
    #[arg(long, value_name = "DIR")]
    pub levels: Option<PathBuf>,
    /// all | slice. `slice` keeps only pipeline.json slice.assets[].
    #[arg(long, default_value = "all")]
    pub scope: String,
    /// pipeline.json holding the slice definition (required for --scope slice).
    #[arg(long, value_name = "FILE")]
    pub pipeline: Option<PathBuf>,
    /// Game id used in package_id; defaults to the requirements parent name.
    #[arg(long)]
    pub game_id: Option<String>,
    /// catalog.lock.json from `step-300 prototype lock`. Copied into the package
    /// and bound into the revision digest. Every entry must be human-approved.
    #[arg(long, value_name = "FILE")]
    pub catalog_lock: Option<PathBuf>,
    /// Directory of compiled scenes from `step-300 scene compile`. Each scene
    /// is copied into the package and its scene->resource->sheet->PNG closure
    /// is verified before anything is published.
    #[arg(long, value_name = "DIR")]
    pub scenes: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    /// Package directory produced by `step-400 run`.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct ApproveArgs {
    /// Package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// The manifest digest from revision.manifest.json.
    #[arg(long)]
    pub digest: String,
}

#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// Package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
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
        Command::Run(args) => run::run(args),
        Command::Check(args) => run::check(args),
        Command::Approve(args) => run::approve(args),
        Command::Status(args) => run::status(args),
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
