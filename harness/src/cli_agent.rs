//! `pxl-harness agent` subcommand: the generic agent mode. One attempt
//! directory, built-in tools (fs + todo), optional MCP bridges, optional
//! goal (evaluator-judged acceptance). Distinct from the pipeline stages:
//! no frozen contracts, but the same sandbox, budget and event audit.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;

use crate::agent::compress::transcript_size;
use crate::agent::provider::ModelProvider;
use crate::agent::tools::{TodoManager, ToolContext, ToolRegistry};
use crate::config::Config;
use crate::error::{exit, HarnessError, Result};
use crate::policy::budget::BudgetLedger;
use crate::policy::{Permission, PermissionSet};
use crate::storage::{Event, EventLog};

#[derive(Debug, Args)]
pub struct AgentArgs {
    /// Project directory (holds harness.config.json).
    #[arg(long)]
    pub project: PathBuf,
    /// What the agent should accomplish, in one sentence.
    #[arg(long)]
    pub objective: String,
    /// Measurable completion condition. Empty disables the goal loop (the
    /// model alone decides when to stop).
    #[arg(long, default_value = "")]
    pub completion: String,
    /// Working directory for the agent (created if missing). Defaults to
    /// `<project>/agent-attempts/a<ts>`.
    #[arg(long)]
    pub workdir: Option<PathBuf>,
    /// Maximum model turns.
    #[arg(long)]
    pub max_turns: Option<u32>,
    /// Maximum goal continuations after an evaluator "no".
    #[arg(long, default_value = "3")]
    pub max_continuations: u32,
    /// Register MCP servers from `<project>/mcp_servers.json`.
    #[arg(long, default_value = "false")]
    pub mcp: bool,
}

pub fn run(args: &AgentArgs) -> Result<i32> {
    let cfg = Config::load(&args.project)?;
    if args.objective.trim().is_empty() {
        return Err(HarnessError::usage("--objective must not be empty"));
    }
    let workdir = match &args.workdir {
        Some(dir) => dir.clone(),
        None => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            args.project.join("agent-attempts").join(format!("a{ts}"))
        }
    };
    std::fs::create_dir_all(&workdir)?;

    // Event log + ledger inside the attempt directory keep the generic mode
    // auditable exactly like pipeline stages.
    let mut events = EventLog::open(&workdir)?;
    events.append(Event::RunCreated {
        harness_version: crate::VERSION.to_string(),
        config_digest: crate::digest::digest_bytes(
            serde_json::to_string(&cfg)?.as_bytes(),
        ),
    })?;
    let mut ledger = BudgetLedger::from_config(&cfg);
    ledger.save(&workdir)?;

    // Tool assembly: built-ins + optional MCP bridges.
    let mut registry = ToolRegistry::new().with_builtin();
    let mcp = if args.mcp {
        let configs = crate::agent::mcp::config::load_mcp_config(&args.project)?;
        if configs.is_empty() {
            return Err(HarnessError::blocked(
                "--mcp was requested but mcp_servers.json declares no servers",
            )
            .with_hint("add a stdio server config to <project>/mcp_servers.json"));
        }
        let reg = Arc::new(crate::agent::mcp::McpRegistry::start(&configs)?);
        for tool in reg.bridge_tools() {
            registry.register_tool(tool);
        }
        Some(reg)
    } else {
        None
    };

    // Permissions: generic mode grants fs + todo (+ mcp when configured).
    let mut perms = PermissionSet::default()
        .allow(Permission::ReadProject)
        .allow(Permission::WriteCandidate)
        .allow(Permission::Todo);
    if mcp.is_some() {
        perms = perms.allow(Permission::Mcp);
    }
    let todo_manager = Arc::new(TodoManager::new());
    let ctx = ToolContext {
        host: perms.clone(),
        stage: perms.clone(),
        role: perms,
        attempt_dir: workdir.clone(),
        template_roots: vec![args.project.join("templates")],
        genre: String::new(),
        design_dir: None,
        pxl: Arc::new(crate::adapters::PxlAdapter::unavailable()),
        todos: Some(todo_manager),
    };
    let specs = registry.specs(&ctx.role);

    let mut provider = crate::pipeline::runner::provider_from_config(&cfg)?;
    let system_prompt = generic_system_prompt(&workdir, mcp.is_some());

    let task = crate::agent::runner::AgentTask {
        stage: "generic".into(),
        task_id: "agent".into(),
        attempt: 1,
        max_turns: args.max_turns.unwrap_or(cfg.max_model_turns_per_task),
        max_fixes: 0,
    };
    let result = if args.completion.trim().is_empty() {
        run_plain(&mut provider, &registry, &ctx, &mut ledger, &mut events, &workdir, &task, &system_prompt, &args.objective, &specs)
    } else {
        run_with_goal(
            &mut provider,
            &registry,
            &ctx,
            &mut ledger,
            &mut events,
            &workdir,
            &task,
            &system_prompt,
            args,
            &specs,
        )
    };
    if let Some(reg) = &mcp {
        reg.shutdown_all();
    }
    let result = result?;
    events.append(Event::RunStatus {
        status: result.status.clone(),
    })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": result.status == "succeeded",
            "command": "agent",
            "workdir": workdir,
            "status": result.status,
            "stop_reason": result.stop_reason,
            "turns_used": result.turns_used,
            "final_text": result.final_text,
        }))?
    );
    Ok(if result.status == "succeeded" {
        exit::OK
    } else {
        exit::BLOCKED
    })
}

#[allow(clippy::too_many_arguments)]
fn run_plain(
    provider: &mut Box<dyn ModelProvider>,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    ledger: &mut BudgetLedger,
    events: &mut EventLog,
    workdir: &std::path::Path,
    task: &crate::agent::runner::AgentTask,
    system_prompt: &str,
    objective: &str,
    specs: &[crate::agent::provider::ToolSpec],
) -> Result<AgentSummary> {
    let mut runner = crate::agent::runner::AgentRunner {
        provider: provider.as_mut(),
        registry,
        ctx,
        ledger,
        events,
        run_dir: Some(workdir),
    };
    let result = runner.run(task, system_prompt, objective, specs)?;
    Ok(AgentSummary {
        status: "succeeded".into(),
        stop_reason: result.stop_reason,
        turns_used: result.turns_used,
        final_text: result.final_text,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_with_goal(
    provider: &mut Box<dyn ModelProvider>,
    registry: &ToolRegistry,
    ctx: &ToolContext,
    ledger: &mut BudgetLedger,
    events: &mut EventLog,
    workdir: &std::path::Path,
    task: &crate::agent::runner::AgentTask,
    system_prompt: &str,
    args: &AgentArgs,
    specs: &[crate::agent::provider::ToolSpec],
) -> Result<AgentSummary> {
    // The evaluator is a second provider instance from the same config —
    // independent budget accounting happens through its own calls being
    // judged, while the loop keeps one ledger for the conversation.
    let cfg = Config::load(&args.project)?;
    let evaluator_provider = crate::pipeline::runner::provider_from_config(&cfg)?;
    let evaluator = crate::agent::goal::GoalEvaluator::new(evaluator_provider, 10);
    let mut goal = crate::agent::goal::GoalRuntime::new(
        crate::agent::goal::GoalRecord::new(
            args.objective.trim().to_string(),
            Some(args.completion.trim().to_string()),
        ),
        Some(evaluator),
        args.max_continuations,
        Some(workdir.to_path_buf()),
    );
    let mut runner = crate::agent::runner::AgentRunner {
        provider: provider.as_mut(),
        registry,
        ctx,
        ledger,
        events,
        run_dir: Some(workdir),
    };
    let result = runner.run_with_goal(
        task,
        system_prompt,
        &args.objective,
        specs,
        &mut goal,
    )?;
    let status = if result.stop_reason == "goal_complete" {
        "succeeded".into()
    } else {
        "incomplete".into()
    };
    Ok(AgentSummary {
        status,
        stop_reason: result.stop_reason,
        turns_used: result.turns_used,
        final_text: result.final_text,
    })
}

struct AgentSummary {
    status: String,
    stop_reason: String,
    turns_used: u32,
    final_text: String,
}

fn generic_system_prompt(workdir: &std::path::Path, mcp: bool) -> String {
    let mut prompt = format!(
        "You are a general-purpose agent operating inside the pxl-harness sandbox.\n\
         Working directory: {}.\n\
         You may read and write files only inside this directory (fs.read, fs.glob, fs.grep, fs.write, fs.edit).\n\
         Use todo.write to plan multi-step work; the todo list is re-injected every turn.\n\
         Finish by replying with a short summary of what you did.",
        workdir.display()
    );
    if mcp {
        prompt.push_str(
            "\nMCP tools (mcp_<server>_<tool>) are configured by the host; use them when they help.",
        );
    }
    prompt
}

/// Context helper re-exported for tests.
pub fn transcript_weight(messages: &[crate::agent::provider::ChatMessage]) -> usize {
    transcript_size(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_mentions_sandbox() {
        let p = generic_system_prompt(std::path::Path::new("/tmp/x"), false);
        assert!(p.contains("sandbox"));
        assert!(!p.contains("MCP tools"));
        let pm = generic_system_prompt(std::path::Path::new("/tmp/x"), true);
        assert!(pm.contains("mcp_<server>_<tool>"));
    }

    #[test]
    fn empty_objective_is_rejected() {
        let args = AgentArgs {
            project: PathBuf::from("/tmp/nope"),
            objective: "   ".into(),
            completion: String::new(),
            workdir: None,
            max_turns: None,
            max_continuations: 3,
            mcp: false,
        };
        let err = run(&args).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Usage);
    }
}
