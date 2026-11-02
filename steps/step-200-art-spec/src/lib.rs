//! step-200-art-spec: requirements -> per-asset spec files.
//!
//! The only stage where a model describes pixels. Hard rule (plan §4.2):
//! this step never builds. The only allowed external calls are the read-only
//! `step-300 list --json` and `step-300 schema <kind> --json`.

mod catalog;
mod lint;
mod provider;
mod run;
mod tools;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const STEP_ID: &str = "step-200-art-spec";
pub const CONTRACT_ID: &str = "art-spec/v3";

/// The contract this step publishes: `steps/contracts/art-spec.schema.json`.
pub const CONTRACT_JSON: &str = include_str!("../../contracts/art-spec.schema.json");

#[derive(Debug, Parser)]
#[command(
    name = "step-200",
    version = VERSION,
    about = "Art-spec step: requirements -> SVG/ASCII/JSON specs (LLM)",
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
    /// Author specs with the artist role. Honors --scope all|slice.
    Run(RunArgs),
    /// Local validation only: manifest digests plus `step-300 plan --dry-run`
    /// over the whole spec tree. Zero model calls, zero builds.
    Lint(LintArgs),
    /// Forward the read-only `step-300 list` / `step-300 schema`.
    Capabilities(CapabilitiesArgs),
    /// Report the last run state from step.json.
    Status(StatusArgs),
}

#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Design output directory holding asset-requirements.json and
    /// style-guide.json (from step-100, or hand-written).
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// Spec output directory.
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
    /// all | slice. `slice` reads pipeline.json and keeps only
    /// slice.assets[].
    #[arg(long, default_value = "all")]
    pub scope: String,
    /// Build only the requirement whose id matches.
    #[arg(long)]
    pub asset: Option<String>,
    /// fake | openai.
    #[arg(long, default_value = "fake")]
    pub provider: String,
    /// Maximum model turns.
    #[arg(long, default_value_t = 24)]
    pub max_turns: u32,
    /// Token ceiling for this run (prompt + completion). 0 = no ceiling.
    #[arg(long, default_value_t = 0)]
    pub token_budget: u64,
    /// Provider request ceiling for this run. 0 = no ceiling.
    #[arg(long, default_value_t = 0)]
    pub request_budget: u64,
    /// Wall-clock ceiling for this run in seconds. 0 = no ceiling.
    #[arg(long, default_value_t = 0)]
    pub wall_clock_secs: u64,
    /// Prototype library root(s) the model may search (read-only). Repeatable.
    /// Without one the prototype tools report an empty library.
    #[arg(long = "library", value_name = "DIR")]
    pub libraries: Vec<PathBuf>,
    /// Path to the .env file for the openai provider.
    #[arg(long, value_name = "FILE")]
    pub env: Option<PathBuf>,
    /// Explicit pxl / step-300 binary for the read-only capability tools.
    /// Empty means auto-discover.
    #[arg(long, value_name = "FILE")]
    pub pxl_bin: Option<PathBuf>,
    /// en | zh (artist role prompt language).
    #[arg(long, default_value = "en")]
    pub lang: String,
}

#[derive(Debug, clap::Args)]
pub struct LintArgs {
    /// Spec output directory holding spec-manifest.json.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// Explicit pxl / step-300 binary. Empty means auto-discover.
    #[arg(long, value_name = "FILE")]
    pub pxl_bin: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct CapabilitiesArgs {
    /// list | schema.
    #[arg(default_value = "list")]
    pub action: String,
    /// The pxl kind, e.g. shapes, recipe, generators.
    pub kind: String,
    /// Explicit pxl / step-300 binary. Empty means auto-discover.
    #[arg(long, value_name = "FILE")]
    pub pxl_bin: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// Spec output directory holding step.json.
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
        Command::Lint(args) => lint::lint(args),
        Command::Capabilities(args) => lint::capabilities(args),
        Command::Status(args) => lint::status(args),
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
