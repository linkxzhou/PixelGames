//! CLI entry: parses subcommands, resolves the project, prints one JSON
//! summary to stdout and exits through the frozen exit-code table.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::{exit, HarnessError, Result};
use crate::pipeline::runner::{provider_from_config, PipelineRunner, UntilStage};
use crate::storage::artifact_store::{ArtifactStore, ProjectIdentity};

/// Brief → design package → asset package harness that drives `pxl`.
#[derive(Debug, Parser)]
#[command(name = "pxl-harness", version, about = "Agent harness for pixel game design + asset pipelines")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a game project with a brief snapshot (never mutates twice).
    Init {
        /// Project directory (created if missing).
        #[arg(long)]
        project: PathBuf,
        /// Path to the brief JSON file.
        #[arg(long)]
        brief: PathBuf,
        /// Genre template name (default: top_down_tank).
        #[arg(long, default_value = "top_down_tank")]
        genre: String,
    },
    /// Run pipeline stages up to the requested end point.
    Run {
        #[arg(long)]
        project: PathBuf,
        /// End point: design | assets | build | playtest.
        #[arg(long, default_value = "design")]
        until: String,
        /// First stage to execute. Earlier stages are reused when approved,
        /// or left not_requested. design | assets | build | playtest.
        #[arg(long, default_value = "design")]
        from: String,
    },
    /// Approve a stage revision, binding game_id + revision + digest.
    Approve {
        #[arg(long)]
        project: PathBuf,
        /// Stage to approve: design | assets.
        #[arg(long)]
        stage: String,
        #[arg(long)]
        revision: String,
        /// Expected digest; empty means "read it from the revision manifest".
        #[arg(long, default_value = "")]
        digest: String,
    },
    /// Show project + stage + revision + approval status as JSON.
    Status {
        #[arg(long)]
        project: PathBuf,
    },
    /// Generic agent mode: one objective, sandboxed tools, optional MCP and
    /// evaluator-judged goal completion.
    Agent(crate::cli_agent::AgentArgs),
}

pub fn main() -> i32 {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            // Machine-readable error on stdout; humans read stderr too.
            println!("{}", serde_json::json!({"ok": false, "error": e.to_json()}));
            eprintln!("pxl-harness: {e}");
            e.code.exit_code()
        }
    }
}

fn dispatch(cli: Cli) -> Result<i32> {
    match cli.command {
        Command::Init { project, brief, genre } => {
            let brief_text = std::fs::read_to_string(&brief).map_err(|e| {
                HarnessError::not_found(format!("cannot read brief {}: {e}", brief.display()))
            })?;
            let brief_value: serde_json::Value = serde_json::from_str(&brief_text)
                .map_err(|e| HarnessError::schema(format!("brief is not JSON: {e}")))?;
            let game_id = brief_value
                .get("game_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .ok_or_else(|| {
                    HarnessError::schema("brief must carry a non-empty 'game_id' string")
                })?;
            if !crate::contracts::stable_id(&game_id) {
                return Err(HarnessError::schema(format!(
                    "game_id '{game_id}' is not a stable id (letters, digits, _, -)"
                )));
            }
            if !crate::contracts::stable_id(&genre) {
                return Err(HarnessError::schema(format!(
                    "genre '{genre}' is not a stable id"
                )));
            }
            std::fs::create_dir_all(&project)?;
            let identity = ProjectIdentity {
                game_id,
                genre_template: genre,
                ..Default::default()
            };
            let store = ArtifactStore::init(project.clone(), &identity)?;
            let digest = store.snapshot_brief(&brief_value)?;
            print_json(&serde_json::json!({
                "ok": true,
                "command": "init",
                "project": project,
                "game_id": identity.game_id,
                "genre_template": identity.genre_template,
                "brief_digest": digest,
            }))?;
            Ok(exit::OK)
        }
        Command::Run { project, until, from } => {
            let until = UntilStage::parse(&until)?;
            let from = UntilStage::parse(&from)?;
            if from.ord() > until.ord() {
                return Err(crate::error::HarnessError::usage(format!(
                    "--from {} is after --until {}",
                    from.as_str(),
                    until.as_str()
                )));
            }
            let mut runner = PipelineRunner::start(project.clone(), until, from)?;
            let cfg = runner.cfg().clone();
            let mut provider = provider_from_config(&cfg)?;
            let summary = runner.run(provider.as_mut())?;
            print_json(&serde_json::json!({
                "ok": summary.exit_code == exit::OK,
                "command": "run",
                "project": project,
                "run_id": summary.run_id,
                "status": summary.status,
                "run_dir": runner.run_dir(),
            }))?;
            Ok(summary.exit_code)
        }
        Command::Approve { project, stage, revision, digest } => {
            let store = ArtifactStore::open(project.clone())?;
            if stage != "design" && stage != "assets" {
                return Err(HarnessError::usage(format!(
                    "unknown stage '{stage}'"
                ))
                .with_hint("approval stages: design, assets"));
            }
            // Approve binds the manifest digest; an explicit mismatch is a
            // hard error, an empty digest means "trust the manifest".
            // A design run that reached waiting_approval before revisions
            // were committed can still be approved by its deterministic id
            // `d{created_ms}`: the passed attempt is frozen at that moment.
            let recorded = match store.revision_digest(&stage, &revision) {
                Ok(digest) => digest,
                Err(err) if stage == "design" => {
                    commit_waiting_design(&store, &revision).map_err(|_| err)?;
                    store.revision_digest(&stage, &revision)?
                }
                Err(err) => return Err(err),
            };
            if !digest.is_empty() && digest != recorded {
                return Err(HarnessError::integrity(format!(
                    "digest mismatch: given {digest}, revision manifest says {recorded}"
                ))
                .with_hint("approve the revision as committed or investigate the difference"));
            }
            store.record_approval(&stage, &revision, &recorded)?;
            let identity = store.identity()?;
            print_json(&serde_json::json!({
                "ok": true,
                "command": "approve",
                "project": project,
                "stage": stage,
                "revision": revision,
                "digest": recorded,
                "design_revision": identity.design_revision,
                "assets_revision": identity.assets_revision,
            }))?;
            Ok(exit::OK)
        }
        Command::Status { project } => {
            let store = ArtifactStore::open(project.clone())?;
            let identity = store.identity()?;
            let design_approval = identity
                .design_revision
                .as_deref()
                .map(|r| store.approval_valid("design", r))
                .transpose()?
                .unwrap_or(false);
            let assets_approval = identity
                .assets_revision
                .as_deref()
                .map(|r| store.approval_valid("assets", r))
                .transpose()?
                .unwrap_or(false);
            // Latest run summary, when one exists.
            let runs_dir = store.runs_dir();
            let mut runs: Vec<String> = Vec::new();
            if runs_dir.is_dir() {
                for entry in std::fs::read_dir(&runs_dir)? {
                    runs.push(entry?.file_name().to_string_lossy().to_string());
                }
                runs.sort();
            }
            let latest_run = runs.last().cloned();
            let mut run_summary = serde_json::Value::Null;
            if let Some(run_id) = &latest_run {
                let dir = runs_dir.join(run_id);
                if dir.join("run.json").is_file() {
                    let state = crate::storage::run_store::RunStore::open(dir.clone())?
                        .load()?;
                    run_summary = serde_json::to_value(&state)?;
                }
            }
            print_json(&serde_json::json!({
                "ok": true,
                "command": "status",
                "project": project,
                "identity": {
                    "game_id": identity.game_id,
                    "genre_template": identity.genre_template,
                    "design_revision": identity.design_revision,
                    "assets_revision": identity.assets_revision,
                },
                "design_approval_valid": design_approval,
                "assets_approval_valid": assets_approval,
                "latest_run": latest_run,
                "run": run_summary,
            }))?;
            Ok(exit::OK)
        }
        Command::Agent(args) => crate::cli_agent::run(&args),
    }
}

/// Freeze a design attempt that already passed gates but was recorded
/// before the runner committed revisions. The revision id is `d{created_ms}`.
fn commit_waiting_design(store: &ArtifactStore, revision: &str) -> Result<()> {
    let runs_dir = store.runs_dir();
    if !runs_dir.is_dir() {
        return Err(HarnessError::not_found(format!(
            "revision {revision} has no manifest"
        )));
    }
    let mut runs: Vec<String> = std::fs::read_dir(&runs_dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    runs.sort();
    for run_id in runs.iter().rev() {
        let dir = runs_dir.join(run_id);
        let Ok(state) = crate::storage::RunStore::open(dir.clone()).and_then(|s| s.load()) else {
            continue;
        };
        if format!("d{}", state.created_ms) != revision {
            continue;
        }
        if state.status != "waiting_approval" {
            return Err(HarnessError::blocked(format!(
                "run {} is {}, not waiting_approval",
                run_id, state.status
            )));
        }
        let report_path = dir.join("reports/design.gates.json");
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&report_path)?)?;
        if report.get("status").and_then(|s| s.as_str()) != Some("passed") {
            return Err(HarnessError::blocked(
                "design gates did not pass; refusing to freeze the attempt",
            ));
        }
        let attempt = dir.join("attempts/design/design_package/a001");
        if !attempt.is_dir() {
            return Err(HarnessError::not_found(format!(
                "design attempt {} is missing",
                attempt.display()
            )));
        }
        store.commit_revision("design", revision, &attempt)?;
        return Ok(());
    }
    Err(HarnessError::not_found(format!(
        "revision {revision} has no manifest"
    )))
}

fn print_json(v: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}
