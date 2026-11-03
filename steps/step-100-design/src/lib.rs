//! step-100-design: brief -> design package.
//!
//! The designer loop lands here in M3 (plan/v3.0/pxl-step-pipeline-plan.md
//! §4.1). This step never reads `work/200+` and never invokes `step-300`.

mod contracts;
mod gates;
mod init;
mod lessons;
mod review;
mod run;
mod tools;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const STEP_ID: &str = "step-100-design";
pub const CONTRACT_ID: &str = "design/v3";

/// The contract this step publishes: `steps/contracts/design.schema.json`.
pub const CONTRACT_JSON: &str = include_str!("../../contracts/design.schema.json");

/// The pipeline lock contract step-100 writes: `pipeline.schema.json`.
pub const PIPELINE_CONTRACT_JSON: &str = include_str!("../../contracts/pipeline.schema.json");

#[derive(Debug, Parser)]
#[command(
    name = "step-100",
    version = VERSION,
    about = "Design step: brief -> design package (LLM)",
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
    /// Fold a hand-written brief.json into the workspace root.
    Init(InitArgs),
    /// Run the designer loop over the brief.
    Run(RunArgs),
    /// Re-evaluate gates without a model; verifies approval digests.
    Gate(GateArgs),
    /// Approve the current revision by manifest digest.
    Approve(ApproveArgs),
    /// Report the last run state from step.json.
    Status(StatusArgs),
    /// Deterministically analyze run evidence or compare locked experiments.
    Lessons(LessonsArgs),
}

#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Path to the hand-written brief.json.
    #[arg(long, value_name = "FILE")]
    pub brief: PathBuf,
    /// Workspace root for this game (becomes work/100-design).
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
    /// Allow re-init over an existing step.json.
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Directory produced by `step-100 init`.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// Design package output; defaults to the input directory.
    #[arg(short, long, value_name = "DIR")]
    pub out: Option<PathBuf>,
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
    /// Stop before gates: write the draft and exit 0.
    #[arg(long, default_value = "gates")]
    pub until: String,
    /// Path to the .env file for the openai provider.
    #[arg(long, value_name = "FILE")]
    pub env: Option<PathBuf>,
    /// en | zh.
    #[arg(long, default_value = "en")]
    pub lang: String,
}

#[derive(Debug, clap::Args)]
pub struct GateArgs {
    /// Design package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct ApproveArgs {
    /// Design package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
    /// The manifest digest from revision.manifest.json.
    #[arg(long)]
    pub digest: String,
}

#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// Design package directory.
    #[arg(long = "in", value_name = "DIR")]
    pub input: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct LessonsArgs {
    /// analyze | compare.
    #[arg(default_value = "analyze")]
    pub action: String,
    /// Run directory or a parent containing run directories.
    #[arg(long, value_name = "DIR")]
    pub runs: PathBuf,
    /// Baseline experiment input for compare.
    #[arg(long, value_name = "FILE")]
    pub baseline: Option<PathBuf>,
    /// Candidate experiment input for compare.
    #[arg(long, value_name = "FILE")]
    pub candidate: Option<PathBuf>,
    /// Output directory for lesson candidates or the experiment report.
    #[arg(short, long, value_name = "DIR")]
    pub out: PathBuf,
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
        Command::Init(args) => init::init(args),
        Command::Run(args) => run::run(args),
        Command::Gate(args) => run::gate(args),
        Command::Approve(args) => run::approve(args),
        Command::Status(args) => run::status(args),
        Command::Lessons(args) => lessons::lessons(args),
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
