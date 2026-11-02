use super::*;
use crate::agent::provider::{FakeProvider, FakeTurn};
use crate::agent::tools::{ToolDef, ToolOutcome, ToolSource};
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
        pxl: Arc::new(crate::pxl_adapter::PxlAdapter::unavailable()),
        todos: None,
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
        read_only: false,
        activity: "echo".into(),
        source: ToolSource::Stage,
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
    let result = runner.run(&task, "system", "user", &specs).unwrap();
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
    let saved =
        std::fs::read_to_string(results_dir.join("call_echo_1_1.txt")).unwrap_or_else(|_| {
            std::fs::read_to_string(results_dir.join("call_echo_1_2.txt")).unwrap()
        });
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
    let dir = std::env::temp_dir().join(format!("pxl_h_loopcap_{}", std::process::id()));
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
    let task0 = AgentTask {
        max_turns: 0,
        ..task
    };
    let result = runner.run(&task0, "s", "u", &specs).unwrap();
    assert_eq!(result.stop_reason, "max_turns");
}

#[test]
fn budget_exhaustion_ends_loop_with_evidence() {
    let dir = std::env::temp_dir().join(format!("pxl_h_loopbudget_{}", std::process::id()));
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
    assert_eq!(err.code, pxl_contract::error::ErrorCode::Budget);
}

#[test]
fn goal_loop_continues_until_evaluator_says_yes() {
    let dir = std::env::temp_dir().join(format!("pxl_h_loopgoal_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let registry = ToolRegistry::new();
    let tool_ctx = ctx(&dir);
    let specs = registry.specs(&tool_ctx.role);
    // Conversation model: stops after one empty turn. Evaluator: first
    // verdict "no" (one unmet item), second "yes".
    let mut provider = FakeProvider::new(vec![
        FakeTurn::Text("I am done".into()),
        FakeTurn::Text("really done now".into()),
    ]);
    let evaluator_provider = FakeProvider::new(vec![
        FakeTurn::Text("- checklist item".into()),
        FakeTurn::Text("no\n- checklist item".into()),
        FakeTurn::Text("yes".into()),
    ]);
    let evaluator = crate::agent::goal::GoalEvaluator::new(Box::new(evaluator_provider), 10);
    let mut goal = crate::agent::goal::GoalRuntime::new(
        crate::agent::goal::GoalRecord::new(
            "deliver the package",
            Some("all checklist items satisfied".into()),
        ),
        Some(evaluator),
        3,
        Some(dir.clone()),
    );
    let mut ledger = BudgetLedger::default();
    let mut events = EventLog::open(&dir).unwrap();
    let task = AgentTask {
        stage: "generic".into(),
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
        .run_with_goal(&task, "system", "user", &specs, &mut goal)
        .unwrap();
    // First stop was rejected (evaluator "no" → continuation), second
    // stop accepted (evaluator "yes").
    assert_eq!(result.stop_reason, "goal_complete");
    assert_eq!(result.turns_used, 2);
    assert_eq!(goal.continuation_count, 1);
    let logged = EventLog::read_all(&dir).unwrap();
    assert!(logged.iter().any(|e| matches!(
        &e.event,
        Event::GoalEvaluated {
            yes: true,
            evaluator_error: None,
            ..
        }
    )));
    assert!(logged.iter().any(|e| matches!(
        &e.event,
        Event::GoalEvaluated {
            yes: false,
            unmet,
            ..
        } if unmet.first().map(String::as_str) == Some("checklist item")
    )));
    // The continuation prompt reached the conversation.
    assert!(goal
        .record
        .history
        .iter()
        .any(|h| h["reason"] == "evaluator_yes"));
}

#[test]
fn parallel_read_only_tools_keep_transcript_order() {
    let dir = std::env::temp_dir().join(format!("pxl_h_looppar_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut registry = ToolRegistry::new();
    registry.register(ToolDef {
        name: "slow.read".into(),
        description: "slow read".into(),
        parameters: serde_json::json!({"type": "object"}),
        needed: Permission::ReadProject,
        read_only: true,
        activity: "reading".into(),
        source: ToolSource::Builtin,
        run: Box::new(|args, _| {
            // The calls run concurrently: total wall clock stays close
            // to one sleep, not three.
            let ms = args["ms"].as_u64().unwrap_or(50);
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(ToolOutcome::ok(format!("slept {ms}ms")))
        }),
    });
    registry.register(ToolDef {
        name: "write.one".into(),
        description: "write".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {"path": {"type": "string"}, "content": {"type": "string"}}
        }),
        needed: Permission::WriteCandidate,
        read_only: false,
        activity: "writing".into(),
        source: ToolSource::Builtin,
        run: Box::new(|args, ctx| {
            let rel = args["path"].as_str().unwrap_or_default();
            std::fs::write(
                ctx.attempt_dir.join(rel),
                args["content"].as_str().unwrap_or_default(),
            )
            .unwrap();
            Ok(ToolOutcome::ok(format!("wrote {rel}")))
        }),
    });
    let tool_ctx = ctx(&dir);
    let specs = registry.specs(&tool_ctx.role);
    assert_eq!(specs.len(), 2, "{specs:?}");
    let mut provider = FakeProvider::new(vec![
        FakeTurn::Calls(vec![
            ("slow.read".into(), r#"{"ms":120}"#.into()),
            ("slow.read".into(), r#"{"ms":120}"#.into()),
            ("slow.read".into(), r#"{"ms":120}"#.into()),
            (
                "write.one".into(),
                r#"{"path":"a.txt","content":"x"}"#.into(),
            ),
        ]),
        FakeTurn::Text("done".into()),
    ]);
    let mut ledger = BudgetLedger::default();
    let mut events = EventLog::open(&dir).unwrap();
    let task = AgentTask {
        stage: "generic".into(),
        task_id: "t1".into(),
        attempt: 1,
        max_turns: 5,
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
    let started = std::time::Instant::now();
    let result = runner.run(&task, "s", "u", &specs).unwrap();
    assert_eq!(result.final_text, "done");
    // Three 120ms sleeps ran in parallel: far below the serial 360ms.
    assert!(
        started.elapsed().as_millis() < 340,
        "read-only batch was not parallel: {}ms",
        started.elapsed().as_millis()
    );
    assert!(dir.join("a.txt").is_file());
    let logged = EventLog::read_all(&dir).unwrap();
    let finished: Vec<&Event> = logged
        .iter()
        .map(|e| &e.event)
        .filter(|e| matches!(e, Event::ToolFinished { .. }))
        .collect();
    assert_eq!(finished.len(), 4);
    // Mutating call keeps its declared position (after the reads).
    assert!(matches!(
        finished[3],
        Event::ToolFinished { tool, .. } if tool == "write.one"
    ));
}
