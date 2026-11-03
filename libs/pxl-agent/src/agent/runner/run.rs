//! Minimal agent loop: model turn → tool calls → results → repeat, with
//! bounded turns, budget settlement and event logging.
//!
//! v3: read-only tools in one turn run in parallel; the todo block is
//! re-injected before every model call; `run_with_goal` adds the harness
//! acceptance loop (evaluator-judged continuation, never model-judged).

use crate::agent::provider::{ChatMessage, ModelProvider, ProviderResponse, ToolSpec};
use crate::agent::tools::{ToolContext, ToolOutcome, ToolRegistry};
use crate::policy::budget::BudgetLedger;
use pxl_contract::error::{HarnessError, Result};
use pxl_store::storage::{Event, EventLog};


use super::helpers::{artifact_pairs, render_outcome};


use super::types::{AgentRunner, AgentTask, LoopResult};

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
        self.run_inner(task, &mut messages, tools, None)
    }

    /// v3: run with a goal runtime. When the model stops calling tools but
    /// the independent evaluator says the goal is unmet (and continuation
    /// budget remains), inject an unmet-items continuation prompt instead of
    /// accepting the stop. The model can never complete a goal itself.
    pub fn run_with_goal(
        &mut self,
        task: &AgentTask,
        system_prompt: &str,
        user_prompt: &str,
        tools: &[ToolSpec],
        goal: &'a mut crate::agent::goal::GoalRuntime,
    ) -> Result<LoopResult> {
        let mut messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(user_prompt),
        ];
        self.run_inner(task, &mut messages, tools, Some(goal))
    }

    fn run_inner(
        &mut self,
        task: &AgentTask,
        messages: &mut Vec<ChatMessage>,
        tools: &[ToolSpec],
        mut goal: Option<&'a mut crate::agent::goal::GoalRuntime>,
    ) -> Result<LoopResult> {
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
            // v3: refresh the todo block before every model call so plans
            // stay visible across turns.
            self.inject_todo_block(messages);
            // v3: compress older tool payloads when the transcript grows.
            // Digest stubs keep the audit trail linkable.
            crate::agent::compress::compress_transcript(messages);
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
                // v3: goal acceptance check — the evaluator, not the model,
                // decides whether the stop is acceptable.
                if let Some(goal) = goal.as_deref_mut() {
                    goal.account(
                        response.prompt_tokens + response.completion_tokens,
                        elapsed / 1000,
                    );
                    let verdict = goal.maybe_evaluate(messages);
                    if let Some(result) = verdict {
                        self.events.append(Event::GoalEvaluated {
                            stage: task.stage.clone(),
                            task_id: task.task_id.clone(),
                            yes: result.yes,
                            evaluator_error: result.error.clone(),
                            unmet: result.unmet.clone(),
                        })?;
                        if result.yes {
                            return Ok(LoopResult {
                                turns_used,
                                stop_reason: "goal_complete".into(),
                                final_text: response.text,
                            });
                        }
                        if goal.should_auto_continue() {
                            goal.continuation_count += 1;
                            messages.push(ChatMessage::assistant(response.text.clone()));
                            messages
                                .push(ChatMessage::user(goal.continuation_prompt(&result.unmet)));
                            continue;
                        }
                    }
                }
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
            // v3: execute the calls. Read-only calls run in parallel
            // (spawned threads); mutating calls run sequentially in order.
            let results = self.execute_calls_parallel(task, &response, tools, turns_used)?;
            for (call, call_id, outcome, tool_elapsed, result_digest) in results {
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
                let (ok, artifact_digests, error_code, exit_code, pxl_error_code) = match &outcome {
                    Ok(o) => (
                        o.ok,
                        artifact_pairs(o),
                        None,
                        o.exit_code,
                        o.error_code.clone(),
                    ),
                    Err(e) => (
                        false,
                        Vec::new(),
                        Some(e.code.as_str().to_string()),
                        None,
                        None,
                    ),
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

    /// Execute one turn's tool calls. `ToolStarted` events are appended up
    /// front (intent before side effects); parallel read-only calls join on
    /// a scoped thread, mutating calls run in order.
    #[allow(clippy::too_many_arguments)]
    fn execute_calls_parallel(
        &mut self,
        task: &AgentTask,
        response: &ProviderResponse,
        tools: &[ToolSpec],
        turns_used: u32,
    ) -> Result<
        Vec<(
            crate::agent::provider::ToolCall,
            String,
            Result<ToolOutcome>,
            u64,
            String,
        )>,
    > {
        let mut call_ids = Vec::with_capacity(response.tool_calls.len());
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
                arguments_digest: pxl_store::digest::digest_bytes(call.arguments.as_bytes()),
                call_id: call_id.clone(),
            })?;
            call_ids.push(call_id);
        }
        let mut executed: Vec<(
            crate::agent::provider::ToolCall,
            String,
            Result<ToolOutcome>,
            u64,
            String,
        )> = Vec::with_capacity(response.tool_calls.len());
        let mut i = 0;
        while i < response.tool_calls.len() {
            let call = &response.tool_calls[i];
            let is_read_only = self
                .registry
                .get(&call.name)
                .map(|t| t.read_only)
                .unwrap_or(false);
            if is_read_only {
                // Collect the maximal consecutive read-only run and fan out.
                let run_start = i;
                while i < response.tool_calls.len() {
                    let c = &response.tool_calls[i];
                    let ro = self
                        .registry
                        .get(&c.name)
                        .map(|t| t.read_only)
                        .unwrap_or(false);
                    if !ro {
                        break;
                    }
                    i += 1;
                }
                let run = &response.tool_calls[run_start..i];
                let ids = &call_ids[run_start..i];
                let outcomes = self.run_read_only_batch(run, tools);
                for ((call, outcome, tool_elapsed), call_id) in outcomes.into_iter().zip(ids) {
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
                    let digest = self.persist_tool_result(call_id, &summary)?;
                    executed.push((call.clone(), call_id.clone(), outcome, tool_elapsed, digest));
                }
            } else {
                let started = std::time::Instant::now();
                let outcome = self.execute_call(call, tools);
                let elapsed = started.elapsed().as_millis() as u64;
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
                let call_id = call_ids[i].clone();
                let digest = self.persist_tool_result(&call_id, &summary)?;
                executed.push((call.clone(), call_id, outcome, elapsed, digest));
                i += 1;
            }
        }
        Ok(executed)
    }

    /// Fan out one batch of read-only calls on scoped threads. The registry
    /// and context are shared immutably; ordering of results follows the
    /// call order (deterministic transcript).
    fn run_read_only_batch(
        &self,
        run: &[crate::agent::provider::ToolCall],
        tools: &[ToolSpec],
    ) -> Vec<(crate::agent::provider::ToolCall, Result<ToolOutcome>, u64)> {
        if run.len() == 1 {
            let started = std::time::Instant::now();
            let outcome = self.execute_call(&run[0], tools);
            let elapsed = started.elapsed().as_millis() as u64;
            return vec![(run[0].clone(), outcome, elapsed)];
        }
        let registry = self.registry as *const ToolRegistry;
        let ctx = self.ctx as *const ToolContext;
        let tools_ptr = tools as *const [ToolSpec];
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(run.len());
            for call in run {
                let registry: &ToolRegistry = unsafe { &*registry };
                let ctx: &ToolContext = unsafe { &*ctx };
                let tools: &[ToolSpec] = unsafe { &*tools_ptr };
                handles.push(scope.spawn(move || {
                    let started = std::time::Instant::now();
                    let args: serde_json::Value = match serde_json::from_str(&call.arguments) {
                        Ok(v) => v,
                        Err(e) => {
                            return (
                                call.clone(),
                                Err(HarnessError::usage(format!(
                                    "tool '{}' arguments are not JSON: {e}",
                                    call.name
                                ))),
                                0u64,
                            )
                        }
                    };
                    let outcome = if tools.iter().any(|t| t.name == call.name) {
                        registry.dispatch(&call.name, &args, ctx)
                    } else {
                        Err(HarnessError::policy(format!(
                            "tool '{}' is not offered to this role",
                            call.name
                        ))
                        .with_hint("roles cannot invoke tools outside their configured set"))
                    };
                    let elapsed = started.elapsed().as_millis() as u64;
                    (call.clone(), outcome, elapsed)
                }));
            }
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        (
                            crate::agent::provider::ToolCall {
                                name: String::new(),
                                arguments: String::new(),
                                id: String::new(),
                            },
                            Err(HarnessError::tool("read-only tool thread panicked")),
                            0,
                        )
                    })
                })
                .collect()
        })
    }

    /// v3: keep the todo block fresh. Replaces the previous injected block
    /// (if any) so the model sees exactly one todo section.
    fn inject_todo_block(&self, messages: &mut Vec<ChatMessage>) {
        let block = self.ctx.todo_block();
        if block.is_empty() {
            return;
        }
        // Drop any previously injected todo message (tagged by prefix).
        messages.retain(|m| !(m.role == "user" && m.content.starts_with("[[todo]]\n")));
        messages.push(ChatMessage::user(format!("[[todo]]\n{block}")));
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
        let digest = pxl_store::digest::digest_bytes(text.as_bytes());
        if let Some(dir) = self.run_dir {
            let results_dir = dir.join("tool-results");
            std::fs::create_dir_all(&results_dir)
                .map_err(|e| HarnessError::io(format!("{}: {e}", results_dir.display())))?;
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

