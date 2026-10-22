//! Minimal agent loop: model turn → tool calls → results → repeat, with
//! bounded turns, budget settlement and event logging.

use crate::agent::provider::{ChatMessage, ModelProvider, ProviderResponse, ToolSpec};
use crate::agent::tools::{ToolContext, ToolOutcome, ToolRegistry};
use crate::error::{HarnessError, Result};
use crate::policy::budget::BudgetLedger;
use crate::storage::{Event, EventLog};

#[derive(Debug, Clone)]
pub struct AgentTask {
    pub stage: String,
    pub task_id: String,
    pub attempt: u32,
    /// Maximum model turns for this task.
    pub max_turns: u32,
    /// Maximum distinct rounds after a failed gate/repair.
    pub max_fixes: u32,
}

#[derive(Debug, Clone)]
pub struct LoopResult {
    pub turns_used: u32,
    pub stop_reason: String,
    pub final_text: String,
}

pub struct AgentRunner<'a> {
    pub provider: &'a mut dyn ModelProvider,
    pub registry: &'a ToolRegistry,
    pub ctx: &'a ToolContext,
    pub ledger: &'a mut BudgetLedger,
    pub events: &'a mut EventLog,
    /// Directory for immutable rendered tool-result texts (attempt-relative).
    /// `None` disables result persistence (kept for callers without a run dir).
    pub run_dir: Option<&'a std::path::Path>,
}

impl<'a> AgentRunner<'a> {
    /// Run the loop until the model stops calling tools, hits the turn cap,
    /// or the budget runs out.
    pub fn run(
        &mut self,
        task: &AgentTask,
        system_prompt: &str,
        user_prompt: &str,
        tools: &[ToolSpec],
    ) -> Result<LoopResult> {
        let mut messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(user_prompt),
        ];
        let mut turns_used = 0u32;
        loop {
            self.ledger.check()?;
            if turns_used >= task.max_turns {
                return Ok(LoopResult {
                    turns_used,
                    stop_reason: "max_turns".into(),
                    final_text: String::new(),
                });
            }
            self.events.append(Event::ModelTurnStarted {
                stage: task.stage.clone(),
                task_id: task.task_id.clone(),
                turn: turns_used,
                attempt: task.attempt,
            })?;
            let started = std::time::Instant::now();
            let response: ProviderResponse = self
                .provider
                .complete(&messages, tools)
                .map_err(|e| HarnessError::provider(e.message.clone()))?;
            let elapsed = started.elapsed().as_millis() as u64;
            self.ledger
                .settle(response.prompt_tokens, response.completion_tokens, elapsed);
            // Persist after every finished model call: a crash between
            // settlement and the stage-level save must not lose usage.
            self.save_ledger()?;
            turns_used += 1;
            self.events.append(Event::ModelTurnFinished {
                stage: task.stage.clone(),
                task_id: task.task_id.clone(),
                turn: turns_used.saturating_sub(1),
                stop_reason: response.stop_reason.clone(),
                prompt_tokens: response.prompt_tokens,
                completion_tokens: response.completion_tokens,
                attempt: task.attempt,
                elapsed_ms: elapsed,
            })?;
            if response.tool_calls.is_empty() {
                return Ok(LoopResult {
                    turns_used,
                    stop_reason: response.stop_reason.clone(),
                    final_text: response.text,
                });
            }
            // Record the assistant tool-call turn, then execute each call.
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: response.text.clone(),
                tool_calls: Some(crate::agent::provider::tool_calls_value(
                    &response.tool_calls,
                )),
                tool_name: None,
                tool_call_id: None,
            });
            for call in &response.tool_calls {
                let call_id = if call.id.is_empty() {
                    format!("call_{}_{}_{}", call.name, task.attempt, turns_used)
                } else {
                    call.id.clone()
                };
                self.events.append(Event::ToolStarted {
                    stage: task.stage.clone(),
                    task_id: task.task_id.clone(),
                    attempt: task.attempt,
                    tool: call.name.clone(),
                    arguments_digest: crate::digest::digest_bytes(call.arguments.as_bytes()),
                    call_id: call_id.clone(),
                })?;
                let tool_started = std::time::Instant::now();
                let outcome = self.execute_call(call, tools);
                let tool_elapsed = tool_started.elapsed().as_millis() as u64;
                let summary = match &outcome {
                    Ok(o) => render_outcome(o),
                    Err(e) => format!(
                        "tool error [{}]: {}{}",
                        e.code.as_str(),
                        e.message,
                        e.hint
                            .as_ref()
                            .map(|h| format!(" (hint: {h})"))
                            .unwrap_or_default()
                    ),
                };
                // Persist the exact rendered text the model will see, so the
                // feedback chain is auditable across crash and repair rounds.
                let result_digest = self.persist_tool_result(&call_id, &summary)?;
                let (ok, artifact_digests, error_code, exit_code, pxl_error_code) = match &outcome {
                    Ok(o) => (
                        o.ok,
                        artifact_pairs(o),
                        None,
                        o.exit_code,
                        o.error_code.clone(),
                    ),
                    Err(e) => (false, Vec::new(), Some(e.code.as_str().to_string()), None, None),
                };
                self.ledger.settle_tool(tool_elapsed);
                self.save_ledger()?;
                self.events.append(Event::ToolFinished {
                    stage: task.stage.clone(),
                    task_id: task.task_id.clone(),
                    attempt: task.attempt,
                    tool: call.name.clone(),
                    ok,
                    exit_code,
                    artifact_digests,
                    error_code,
                    call_id: call_id.clone(),
                    result_digest,
                    elapsed_ms: tool_elapsed,
                    pxl_error_code,
                })?;
                if let Ok(o) = outcome.as_ref() {
                    if o.ok {
                        if let Some(art) = &o.artifact {
                            self.events.append(Event::CandidateWritten {
                                stage: task.stage.clone(),
                                task_id: task.task_id.clone(),
                                attempt: task.attempt,
                                tool: call.name.clone(),
                                call_id: call_id.clone(),
                                turn: turns_used.saturating_sub(1),
                                path: art.path.clone(),
                                digest: art.digest.clone(),
                                prompt_tokens: response.prompt_tokens,
                                completion_tokens: response.completion_tokens,
                            })?;
                        }
                    }
                }
                messages.push(ChatMessage {
                    role: "tool".into(),
                    content: summary,
                    tool_calls: None,
                    tool_name: Some(call.name.clone()),
                    tool_call_id: Some(call_id),
                });
            }
        }
    }

    /// Persist the ledger bound to the event seq already written, so a crash
    /// between an event and the snapshot never double-counts or loses usage.
    fn save_ledger(&mut self) -> Result<()> {
        if let Some(dir) = self.run_dir {
            if let Some(seq) = self.events.last_seq() {
                self.ledger.settled_seq = seq;
            }
            self.ledger.save(dir)?;
        }
        Ok(())
    }

    /// Write the rendered tool-result text to `tool-results/<call_id>.txt`
    /// and return its digest. Failures to persist are surfaced: feedback
    /// auditing is a core guarantee, not best-effort.
    fn persist_tool_result(&mut self, call_id: &str, text: &str) -> Result<String> {
        let digest = crate::digest::digest_bytes(text.as_bytes());
        if let Some(dir) = self.run_dir {
            let results_dir = dir.join("tool-results");
            std::fs::create_dir_all(&results_dir).map_err(|e| {
                HarnessError::io(format!("{}: {e}", results_dir.display()))
            })?;
            let safe: String = call_id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let path = results_dir.join(format!("{safe}.txt"));
            std::fs::write(&path, text)
                .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        }
        Ok(digest)
    }

    fn execute_call(
        &self,
        call: &crate::agent::provider::ToolCall,
        tools: &[ToolSpec],
    ) -> Result<ToolOutcome> {
        if !tools.iter().any(|t| t.name == call.name) {
            return Err(HarnessError::policy(format!(
                "tool '{}' is not offered to this role",
                call.name
            ))
            .with_hint("roles cannot invoke tools outside their configured set"));
        }
        let args: serde_json::Value = serde_json::from_str(&call.arguments).map_err(|e| {
            HarnessError::usage(format!("tool '{}' arguments are not JSON: {e}", call.name))
        })?;
        self.registry.dispatch(&call.name, &args, self.ctx)
    }
}

fn artifact_pairs(o: &ToolOutcome) -> Vec<(String, String)> {
    o.artifact
        .as_ref()
        .map(|a| vec![(a.path.clone(), a.digest.clone())])
        .unwrap_or_default()
}

fn render_outcome(o: &ToolOutcome) -> String {
    let flag = if o.ok { "ok" } else { "failed" };
    let mut text = format!("[{flag}] {}", o.summary);
    if !o.data.is_null() {
        let compact = serde_json::to_string(&o.data).unwrap_or_default();
        if !compact.is_empty() && compact != "null" {
            text.push('\n');
            text.push_str(&compact);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::{FakeProvider, FakeTurn};
    use crate::agent::tools::{ToolDef, ToolOutcome};
    use crate::policy::{Permission, PermissionSet};
    use std::sync::Arc;

    fn ctx(attempt: &std::path::Path) -> ToolContext {
        let full = PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate);
        ToolContext {
            host: full.clone(),
            stage: full.clone(),
            role: full,
            attempt_dir: attempt.to_path_buf(),
            template_roots: vec![],
            genre: String::new(),
            design_dir: None,
            pxl: Arc::new(crate::adapters::PxlAdapter::unavailable()),
        }
    }

    #[test]
    fn loop_executes_tools_until_stop() {
        let dir = std::env::temp_dir().join(format!("pxl_h_loop_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut registry = ToolRegistry::new();
        registry.register(ToolDef {
            name: "echo".into(),
            description: "echo".into(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["msg"],
                "properties": {"msg": {"type": "string"}}
            }),
            needed: Permission::WriteCandidate,
            run: Box::new(|args, _| {
                let msg = args["msg"].as_str().unwrap_or_default();
                Ok(ToolOutcome::ok(format!("echo:{msg}")))
            }),
        });
        let tool_ctx = ctx(&dir);
        let specs = registry.specs(&tool_ctx.role);
        let mut provider = FakeProvider::new(vec![
            FakeTurn::Calls(vec![("echo".into(), r#"{"msg":"hi"}"#.into())]),
            FakeTurn::Calls(vec![("echo".into(), r#"{"msg":123}"#.into())]),
            FakeTurn::Text("all done".into()),
        ]);
        let mut ledger = BudgetLedger::default();
        let mut events = EventLog::open(&dir).unwrap();
        let task = AgentTask {
            stage: "design".into(),
            task_id: "t1".into(),
            attempt: 1,
            max_turns: 10,
            max_fixes: 3,
        };
        let mut runner = AgentRunner {
            provider: &mut provider,
            registry: &registry,
            ctx: &tool_ctx,
            ledger: &mut ledger,
            events: &mut events,
            run_dir: Some(&dir),
        };
        let result = runner
            .run(&task, "system", "user", &specs)
            .unwrap();
        assert_eq!(result.turns_used, 3);
        assert_eq!(result.final_text, "all done");
        assert_eq!(ledger.usage.requests, 3);
        let logged = EventLog::read_all(&dir).unwrap();
        assert!(logged.iter().any(|e| matches!(
            &e.event,
            Event::ToolFinished { ok: true, tool, result_digest, .. }
                if tool == "echo" && !result_digest.is_empty()
        )));
        // The exact rendered feedback text is persisted per call.
        let results_dir = dir.join("tool-results");
        assert!(results_dir.is_dir());
        let saved = std::fs::read_to_string(
            results_dir.join("call_echo_1_1.txt"),
        )
        .unwrap_or_else(|_| std::fs::read_to_string(results_dir.join("call_echo_1_2.txt")).unwrap());
        assert!(saved.contains("echo:hi"), "{saved}");
        // Bad arguments were reported as an error outcome, not a crash.
        assert!(logged.iter().any(|e| matches!(
            &e.event,
            Event::ToolFinished { ok: false, error_code: Some(c), .. } if c == "E_USAGE"
        )));
        // The ledger is persisted after every settled turn (crash consistency).
        assert!(dir.join("budget.json").is_file());
    }

    #[test]
    fn loop_stops_at_turn_cap() {
        let dir = std::env::temp_dir()
            .join(format!("pxl_h_loopcap_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let registry = ToolRegistry::new();
        let tool_ctx = ctx(&dir);
        let specs = registry.specs(&tool_ctx.role);
        let mut provider = FakeProvider::new(vec![]);
        let mut ledger = BudgetLedger::default();
        let mut events = EventLog::open(&dir).unwrap();
        let task = AgentTask {
            stage: "design".into(),
            task_id: "t1".into(),
            attempt: 1,
            max_turns: 2,
            max_fixes: 3,
        };
        let mut runner = AgentRunner {
            provider: &mut provider,
            registry: &registry,
            ctx: &tool_ctx,
            ledger: &mut ledger,
            events: &mut events,
            run_dir: Some(&dir),
        };
        // Scripted provider ends immediately when its turns run out; force
        // the cap by asking for 0 turns.
        let task0 = AgentTask { max_turns: 0, ..task };
        let result = runner.run(&task0, "s", "u", &specs).unwrap();
        assert_eq!(result.stop_reason, "max_turns");
    }

    #[test]
    fn budget_exhaustion_ends_loop_with_evidence() {
        let dir = std::env::temp_dir()
            .join(format!("pxl_h_loopbudget_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let registry = ToolRegistry::new();
        let tool_ctx = ctx(&dir);
        let specs = registry.specs(&tool_ctx.role);
        let mut provider = FakeProvider::new(vec![]);
        let mut ledger = BudgetLedger {
            request_limit: 1,
            ..Default::default()
        };
        ledger.settle(1, 1, 1);
        let mut events = EventLog::open(&dir).unwrap();
        let task = AgentTask {
            stage: "design".into(),
            task_id: "t1".into(),
            attempt: 1,
            max_turns: 10,
            max_fixes: 3,
        };
        let mut runner = AgentRunner {
            provider: &mut provider,
            registry: &registry,
            ctx: &tool_ctx,
            ledger: &mut ledger,
            events: &mut events,
            run_dir: Some(&dir),
        };
        let err = runner.run(&task, "s", "u", &specs).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Budget);
    }
}
