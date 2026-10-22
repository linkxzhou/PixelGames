//! Design stage runner (H1): drives the designer role, runs design gates,
//! repairs with bounded attempts, and stops at `waiting_approval`.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::agent::provider::ModelProvider;
use crate::agent::role::designer;
use crate::agent::runner::{AgentRunner, AgentTask};
use crate::agent::tools::{ToolContext, ToolDef, ToolOutcome, ToolRegistry};
use crate::config::Config;
use crate::contracts::design::{
    AcceptancePlan, AssetRequirements, GameDesign, LevelDef, StyleGuide,
};
use crate::error::{HarnessError, Result};
use crate::pipeline::state::set_stage;
use crate::policy::budget::BudgetLedger;
use crate::policy::{Permission, PermissionSet};
use crate::qa::{issues_from_report, GateCheck, GateReport, GateStatus, GATE_REPORT_VERSION};
use crate::storage::{ArtifactStore, Event, EventLog, RunState, RunStore, StageStatus};

pub const DESIGN_FILES: &[&str] = &[
    "brief.json",
    "game-design.json",
    "style-guide.json",
    "tuning.json",
    "levels",
    "asset-requirements.json",
    "acceptance-plan.json",
    "design-review.json",
];

pub fn registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(ToolDef {
        name: "design.read_brief".into(),
        description: "Read the user brief snapshot".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::ReadProject,
        run: Box::new(|_args, ctx| {
            // The brief snapshot is copied into the attempt before the
            // loop starts; prefer it, fall back to template roots.
            let attempt_brief = ctx.attempt_dir.join("brief.json");
            let brief = if attempt_brief.is_file() {
                fs::read_to_string(&attempt_brief)
            } else {
                let mut text = None;
                for root in &ctx.template_roots {
                    let p = root.join("brief.json");
                    if p.is_file() {
                        text = Some(fs::read_to_string(&p));
                        break;
                    }
                }
                text.unwrap_or_else(|| {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "brief.json not found",
                    ))
                })
            }
            .map_err(|e| HarnessError::io(e.to_string()))?;
            Ok(ToolOutcome::ok("brief loaded").with_data(serde_json::Value::String(brief)))
        }),
    });
    reg.register(ToolDef {
        name: "design.read_template".into(),
        description: "Read the genre template (defaults, entities, tiles, asset hints)".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::ReadProject,
        run: Box::new(|_args, ctx| {
            // Templates live under the approved template roots only; the
            // genre name comes from project identity, never from the model.
            let mut found = None;
            for root in &ctx.template_roots {
                // Preferred layout: <root>/templates/<genre>/template.json.
                let dir = root.join("templates").join(&ctx.genre);
                let path = dir.join("template.json");
                if path.is_file() {
                    found = Some((path, dir));
                    break;
                }
                // Harness-bundled layout: <root>/<genre>/template.json.
                let bundled = root.join(&ctx.genre);
                let bundled_path = bundled.join("template.json");
                if bundled_path.is_file() {
                    found = Some((bundled_path, bundled));
                    break;
                }
                // Also accept a template root that points straight at the
                // genre directory (used by tests and custom layouts).
                let direct = root.join("template.json");
                if direct.is_file() {
                    found = Some((direct, root.clone()));
                    break;
                }
            }
            let Some((path, dir)) = found else {
                return Ok(ToolOutcome::fail(format!(
                    "genre template '{}' was not found in the approved template roots",
                    ctx.genre
                ))
                .with_data(serde_json::json!({"genre": ctx.genre})));
            };
            let text = fs::read_to_string(&path)
                .map_err(|e| HarnessError::io(e.to_string()))?;
            let mut listing: Vec<String> = Vec::new();
            if let Ok(entries) = fs::read_dir(dir.join("assets")) {
                for e in entries.flatten() {
                    if let Some(n) = e.file_name().to_str() {
                        listing.push(format!("assets/{n}"));
                    }
                }
            }
            listing.sort();
            Ok(ToolOutcome::ok("template loaded")
                .with_data(serde_json::json!({
                    "template": serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text.clone())),
                    "available_assets": listing,
                })))
        }),
    });
    reg.register(ToolDef {
        name: "design.request_review".into(),
        description: "Submit the candidate package for human review with a short summary".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["summary"],
            "properties": {
                "summary": {"type": "string"},
                "risks": {"type": "array", "items": {"type": "string"}},
                "open_questions": {"type": "array", "items": {"type": "string"}}
            }
        }),
        needed: Permission::WriteCandidate,
        run: Box::new(|args, ctx| {
            // Record the request as a review request file inside the
            // candidate; the harness turns it into the Markdown view and
            // gates on its presence before waiting for approval.
            let summary = args["summary"].as_str().unwrap_or_default();
            if summary.trim().is_empty() {
                return Ok(ToolOutcome::fail("summary must not be empty"));
            }
            let risks: Vec<String> = args["risks"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let open_questions: Vec<String> = args["open_questions"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let review = serde_json::json!({
                "schema_version": crate::contracts::design::DESIGN_SCHEMA_VERSION,
                "requested_by": "designer",
                "summary": summary,
                "risks": risks,
                "open_questions": open_questions,
            });
            let text = serde_json::to_string_pretty(&review)?;
            let path = ctx.attempt_dir.join("design-review.json");
            fs::write(&path, &text).map_err(|e| HarnessError::io(e.to_string()))?;
            let digest = crate::digest::digest_bytes(text.as_bytes());
            Ok(ToolOutcome::ok("review requested")
                .with_data(serde_json::json!({"path": "design-review.json", "digest": digest})))
        }),
    });
    reg.register(ToolDef {
        name: "design.write_candidate".into(),
        description: "Write one candidate design file (game-design.json, levels/<id>.json, ...)".into(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
        needed: Permission::WriteCandidate,
        run: Box::new(|args, ctx| {
            let rel = args["path"].as_str().unwrap_or_default();
            let content = args["content"].as_str().unwrap_or_default();
            write_candidate(ctx, rel, content)
        }),
    });
    reg.register(ToolDef {
        name: "design.validate".into(),
        description: "Run design gates over the current candidate directory".into(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
        needed: Permission::WriteCandidate,
        run: Box::new(|_args, ctx| {
            let report = crate::pipeline::design_stage::gates(ctx)?;
            let issues = issues_from_report(&report);
            Ok(ToolOutcome::ok(format!(
                "{} checks, {} failures",
                report.checks.len(),
                issues.len()
            ))
            .with_data(serde_json::to_value(&report)?))
        }),
    });
    reg
}

pub fn write_candidate(ctx: &ToolContext, rel: &str, content: &str) -> Result<ToolOutcome> {
    // Path safety first: escapes are hard errors, not tool failures.
    let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
    let path = root.resolve_write(rel)?;
    // Files must land inside the candidate dir and use known names.
    let allowed = DESIGN_FILES.iter().any(|f| {
        rel == *f || (f.ends_with('/') && rel.starts_with(*f)) || (*f == "levels" && rel.starts_with("levels/"))
    }) || rel.ends_with(".md");
    if !allowed {
        return Ok(ToolOutcome::fail(format!(
            "path '{rel}' is not part of the design package"
        ))
        .with_data(serde_json::json!({"allowed": DESIGN_FILES})));
    }
    // Validate JSON files parse before writing.
    if rel.ends_with(".json") {
        serde_json::from_str::<Value>(content).map_err(|e| {
            HarnessError::usage(format!("{rel}: invalid JSON: {e}"))
        })?;
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| HarnessError::io(e.to_string()))?;
    }
    fs::write(&path, content).map_err(|e| HarnessError::io(e.to_string()))?;
    let digest = crate::digest::digest_bytes(content.as_bytes());
    Ok(ToolOutcome::ok(format!("wrote {rel}")).with_data(serde_json::json!({
        "path": rel,
        "digest": digest,
    })))
}

/// Hard design gates (D0). JSON contracts are the fact source.
pub fn gates(ctx: &ToolContext) -> Result<GateReport> {
    let dir = &ctx.attempt_dir;
    let mut checks: Vec<GateCheck> = Vec::new();

    // A file that exists but does not match the contract must report the
    // serde error. Treating that as "missing" leaves the designer with
    // nothing to repair.
    enum Loaded<T> {
        Missing,
        Invalid(String),
        Ready(T),
    }
    fn load_contract<T: serde::de::DeserializeOwned>(dir: &std::path::Path, name: &str) -> Loaded<T> {
        let path = dir.join(name);
        if !path.is_file() {
            return Loaded::Missing;
        }
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<T>(&text) {
                Ok(value) => Loaded::Ready(value),
                Err(e) => Loaded::Invalid(format!("{name}: {e}")),
            },
            Err(e) => Loaded::Invalid(format!("{name}: {e}")),
        }
    }
    fn push_file_check<T>(checks: &mut Vec<GateCheck>, id: &str, limit: &str, loaded: &Loaded<T>) {
        match loaded {
            Loaded::Ready(_) => checks.push(GateCheck {
                id: id.into(),
                passed: true,
                value: "parsed".into(),
                limit: limit.into(),
                failures: vec![],
            }),
            Loaded::Missing => checks.push(GateCheck {
                id: id.into(),
                passed: false,
                value: "missing".into(),
                limit: limit.into(),
                failures: vec![],
            }),
            Loaded::Invalid(err) => checks.push(GateCheck {
                id: id.into(),
                passed: false,
                value: "invalid".into(),
                limit: limit.into(),
                failures: vec![err.clone()],
            }),
        }
    }

    // 1. Required files parse.
    let game = load_contract::<GameDesign>(dir, "game-design.json");
    let style = load_contract::<StyleGuide>(dir, "style-guide.json");
    let reqs = load_contract::<AssetRequirements>(dir, "asset-requirements.json");
    let accept = load_contract::<AcceptancePlan>(dir, "acceptance-plan.json");
    push_file_check(&mut checks, "files.game_design", "game-design.json parses", &game);
    push_file_check(&mut checks, "files.style_guide", "style-guide.json parses", &style);
    push_file_check(
        &mut checks,
        "files.asset_requirements",
        "asset-requirements.json parses",
        &reqs,
    );
    push_file_check(
        &mut checks,
        "files.acceptance_plan",
        "acceptance-plan.json parses",
        &accept,
    );

    let mut failures = Vec::new();
    let mut levels: Vec<LevelDef> = Vec::new();
    let game = match game {
        Loaded::Ready(g) => Some(g),
        _ => None,
    };
    let style = match style {
        Loaded::Ready(s) => Some(s),
        _ => None,
    };
    let reqs = match reqs {
        Loaded::Ready(r) => Some(r),
        _ => None,
    };
    let accept = match accept {
        Loaded::Ready(a) => Some(a),
        _ => None,
    };
    if let Some(g) = &game {
        match crate::contracts::design::validate_ids(g) {
            Ok(()) => checks.push(GateCheck {
                id: "ids.references".into(),
                passed: true,
                value: "closed".into(),
                limit: "rule/entity/action references close".into(),
                failures: vec![],
            }),
            Err(errs) => {
                checks.push(GateCheck {
                    id: "ids.references".into(),
                    passed: false,
                    value: format!("{} problems", errs.len()),
                    limit: "rule/entity/action references close".into(),
                    failures: errs.clone(),
                });
                failures.extend(errs);
            }
        }
        if g.win_condition.is_none() {
            failures.push("win_condition missing".into());
        }
    }
    // Levels.
    let levels_dir = dir.join("levels");
    let mut level_files: Vec<PathBuf> = Vec::new();
    if levels_dir.is_dir() {
        for entry in fs::read_dir(&levels_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                level_files.push(p);
            }
        }
        level_files.sort();
    }
    for path in &level_files {
        let text = fs::read_to_string(path)?;
        match serde_json::from_str::<LevelDef>(&text) {
            Ok(level) => levels.push(level),
            Err(e) => {
                failures.push(format!("{}: {e}", path.display()));
            }
        }
    }
    checks.push(GateCheck {
        id: "levels.count".into(),
        passed: !levels.is_empty(),
        value: levels.len().to_string(),
        limit: ">= 1 level".into(),
        failures: vec![],
    });
    for level in &levels {
        let mut level_failures = Vec::new();
        if level.grid.len() as u32 != level.grid_h {
            level_failures.push(format!(
                "grid rows {} != grid_h {}",
                level.grid.len(),
                level.grid_h
            ));
        }
        let tile_ids: Vec<&str> = level.tile_semantics.iter().map(|t| t.tile_id.as_str()).collect();
        for (y, row) in level.grid.iter().enumerate() {
            if row.len() as u32 != level.grid_w {
                level_failures.push(format!("row {y} width {} != {}", row.len(), level.grid_w));
            }
            for (x, ch) in row.chars().enumerate() {
                let tile_id = char_to_tile_id(ch);
                if !tile_ids.contains(&tile_id.as_str()) {
                    level_failures.push(format!(
                        "cell ({x},{y}) '{ch}' has no tile_semantics entry"
                    ));
                }
            }
        }
        let in_bounds = |p: &[u32; 2]| {
            p[0] < level.grid_w && p[1] < level.grid_h
        };
        if !in_bounds(&level.spawn) {
            level_failures.push(format!("spawn {:?} outside grid", level.spawn));
        }
        if !in_bounds(&level.exit) {
            level_failures.push(format!("exit {:?} outside grid", level.exit));
        }
        for pl in &level.placements {
            if !in_bounds(&pl.pos) {
                level_failures.push(format!(
                    "placement {} {:?} outside grid",
                    pl.entity_id, pl.pos
                ));
            }
            for step in &pl.path {
                if !in_bounds(step) {
                    level_failures.push(format!(
                        "path step {:?} of {} outside grid",
                        step, pl.entity_id
                    ));
                }
            }
        }
        // Spawn must not sit on a blocking tile.
        if let Some(spawn_tile) = tile_at(level, level.spawn) {
            if level
                .tile_semantics
                .iter()
                .any(|t| t.tile_id == spawn_tile && t.blocks)
            {
                level_failures.push(format!(
                    "spawn {:?} is on blocking tile '{}'",
                    level.spawn, spawn_tile
                ));
            }
        }
        // Static reachability: flood-fill over non-blocking tiles.
        let reach = reachable_cells(level);
        if !reach.contains(&(level.exit[0], level.exit[1])) {
            level_failures.push("exit not reachable from spawn (static flood-fill)".into());
        }
        checks.push(GateCheck {
            id: format!("levels.{}.static", level.id),
            passed: level_failures.is_empty(),
            value: if level_failures.is_empty() {
                "ok".into()
            } else {
                format!("{} problems", level_failures.len())
            },
            limit: "grid/tiles/spawn/exit/placements valid; exit reachable".into(),
            failures: level_failures,
        });
    }
    // Asset coverage: every visible entity and logical tile maps to a requirement.
    if let (Some(g), Some(r)) = (&game, &reqs) {
        let mut coverage_failures = Vec::new();
        let covered: Vec<&String> = r.requirements.iter().map(|x| &x.asset_id).collect();
        for e in &g.entities {
            if !covered.contains(&&e.id) {
                coverage_failures.push(format!("entity '{}' has no asset requirement", e.id));
            }
        }
        for level in &levels {
            for t in &level.tile_semantics {
                if t.behaviour != crate::contracts::design::TileBehaviour::Floor
                    && t.behaviour != crate::contracts::design::TileBehaviour::Decor
                    && !covered.contains(&&t.tile_id)
                {
                    coverage_failures.push(format!(
                        "tile '{}' has no asset requirement",
                        t.tile_id
                    ));
                }
            }
        }
        checks.push(GateCheck {
            id: "coverage.assets".into(),
            passed: coverage_failures.is_empty(),
            value: if coverage_failures.is_empty() {
                "covered".into()
            } else {
                format!("{} gaps", coverage_failures.len())
            },
            limit: "visible entities and semantic tiles map to requirements".into(),
            failures: coverage_failures,
        });
        // Expressibility: directions and sizes must fit the style guide.
        if let Some(s) = &style {
            let mut style_failures = Vec::new();
            for r in &r.requirements {
                for d in &r.directions {
                    if !s.directions.contains(d) {
                        style_failures.push(format!(
                            "asset '{}' wants direction '{}' not in style guide",
                            r.asset_id, d
                        ));
                    }
                }
                if r.size_px[0] > 64 || r.size_px[1] > 64 {
                    style_failures.push(format!(
                        "asset '{}' size {:?} exceeds 64px DSL limit; use tiles/objects",
                        r.asset_id, r.size_px
                    ));
                }
            }
            checks.push(GateCheck {
                id: "style.expressible".into(),
                passed: style_failures.is_empty(),
                value: if style_failures.is_empty() {
                    "ok".into()
                } else {
                    format!("{} conflicts", style_failures.len())
                },
                limit: "requirements expressible with current pxl capabilities".into(),
                failures: style_failures,
            });
        }
    }
    // Acceptance coverage: every required asset maps to an acceptance item.
    if let (Some(r), Some(a)) = (&reqs, &accept) {
        let accepted: Vec<&String> = a.items.iter().map(|i| &i.requirement_id).collect();
        let gaps: Vec<String> = r
            .requirements
            .iter()
            .filter(|x| x.required && !accepted.contains(&&x.asset_id))
            .map(|x| format!("requirement '{}' has no acceptance item", x.asset_id))
            .collect();
        checks.push(GateCheck {
            id: "coverage.acceptance".into(),
            passed: gaps.is_empty(),
            value: if gaps.is_empty() { "covered".into() } else { format!("{} gaps", gaps.len()) },
            limit: "required requirements have acceptance items".into(),
            failures: gaps,
        });
    }

    checks.push(GateCheck {
        id: "design.semantic".into(),
        passed: failures.is_empty(),
        value: format!("{} problems", failures.len()),
        limit: "all design files and semantic constraints valid".into(),
        failures,
    });
    let passed = checks.iter().all(|c| c.passed);
    Ok(GateReport {
        report_version: GATE_REPORT_VERSION,
        gate_version: 1,
        stage: "design".into(),
        gate_id: "design.gates".into(),
        status: if passed { GateStatus::Passed } else { GateStatus::Failed },
        input_digest: crate::digest::digest_bytes(dir.to_string_lossy().as_bytes()),
        checks,
        metrics: [("levels".into(), levels.len() as u64)].into_iter().collect(),
        evidence: vec![],
        tool_versions: [("harness".into(), crate::VERSION.into())].into_iter().collect(),
        executed_ms: 0,
    })
}

fn char_to_tile_id(c: char) -> String {
    if c.is_ascii_alphanumeric() {
        c.to_string()
    } else {
        format!("tile_{c}")
    }
}

fn tile_at(level: &LevelDef, pos: [u32; 2]) -> Option<String> {
    let row = level.grid.get(pos[1] as usize)?;
    let c = row.chars().nth(pos[0] as usize)?;
    Some(char_to_tile_id(c))
}

fn reachable_cells(level: &LevelDef) -> std::collections::BTreeSet<(u32, u32)> {
    use std::collections::BTreeSet;
    let mut seen = BTreeSet::new();
    let blocked = |tile: &str| {
        level
            .tile_semantics
            .iter()
            .any(|t| t.tile_id == tile && t.blocks)
    };
    let mut frontier: Vec<[u32; 2]> = vec![level.spawn];
    seen.insert((level.spawn[0], level.spawn[1]));
    while let Some([x, y]) = frontier.pop() {
        for (dx, dy) in [(0u32, 1u32), (0, u32::MAX), (1, 0), (u32::MAX, 0)] {
            let nx = dx.wrapping_add(x);
            let ny = dy.wrapping_add(y);
            if nx >= level.grid_w || ny >= level.grid_h {
                continue;
            }
            if seen.contains(&(nx, ny)) {
                continue;
            }
            if let Some(row) = level.grid.get(ny as usize) {
                if let Some(c) = row.chars().nth(nx as usize) {
                    if !blocked(&char_to_tile_id(c)) {
                        seen.insert((nx, ny));
                        frontier.push([nx, ny]);
                    }
                }
            }
        }
    }
    seen
}

#[allow(clippy::too_many_arguments)]
pub fn run_design_stage(
    provider: &mut dyn ModelProvider,
    cfg: &Config,
    artifacts: &ArtifactStore,
    run: &RunStore,
    state: &mut RunState,
    ledger: &mut BudgetLedger,
    events: &mut EventLog,
    template_roots: &[PathBuf],
) -> Result<StageStatus> {
    set_stage(state, "design", StageStatus::Running)?;
    run.save(state)?;
    let attempt_dir = run.attempt_dir("design", "design_package", 1)?;
    let registry = registry();
    let role = designer();
    let ctx = ToolContext {
        host: PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate),
        stage: PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate),
        role: role.permission_set(),
        attempt_dir: attempt_dir.clone(),
        template_roots: template_roots.to_vec(),
        genre: artifacts.identity()?.genre_template.clone(),
        design_dir: None,
        pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
    };
    // Copy the brief into the attempt so the designer can read it.
    let brief_text = fs::read_to_string(artifacts.brief_path())?;
    fs::write(attempt_dir.join("brief.json"), &brief_text)?;
    let specs = registry.specs(&ctx.role);
    let base_prompt = format!(
        "Produce the design package for game '{}'. Genre template: {}. Follow the JSON contracts in your role prompt exactly (schema_version 1, single-character tile ids). Write one file per tool call, call design.validate, fix every failing check, then call design.request_review. Brief:\n{}",
        artifacts.identity()?.game_id,
        artifacts.identity()?.genre_template,
        brief_text
    );
    // The role prompt promises bounded repair. One agent loop often stops
    // before the gates pass; feed the gate report back up to max_fixes.
    let mut repair_note = String::new();
    let mut report = gates(&ctx)?;
    let mut passed_with_review = false;
    for round in 0..=cfg.max_fixes_per_task {
        let user_prompt = if round == 0 {
            base_prompt.clone()
        } else {
            format!(
                "The design package is not acceptable yet. Repair the candidate files already in the attempt directory, then call design.validate and design.request_review.\n{repair_note}"
            )
        };
        let task = AgentTask {
            stage: "design".into(),
            task_id: "design_package".into(),
            attempt: round + 1,
            max_turns: cfg.max_model_turns_per_task,
            max_fixes: cfg.max_fixes_per_task,
        };
        let outcome = {
            let mut runner = AgentRunner {
                provider,
                registry: &registry,
                ctx: &ctx,
                ledger,
                events,
            };
            runner.run(&task, &role.prompt, &user_prompt, &specs)
        }?;
        set_stage(state, "design", StageStatus::Validating)?;
        state
            .task_mut("design", "design_package")
            .stopped_reason = Some(outcome.stop_reason.clone());
        run.save(state)?;
        report = gates(&ctx)?;
        let report_path = run.reports_dir()?.join("design.gates.json");
        fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
        events.append(Event::GateFinished {
            stage: "design".into(),
            task_id: "design_package".into(),
            passed: report.passed(),
            failures: report.failed_checks().len() as u32,
            report_digest: report.digest()?,
        })?;
        let review_ready = attempt_dir.join("design-review.json").is_file();
        if report.passed() && review_ready {
            passed_with_review = true;
            break;
        }
        let issues = crate::qa::issues_from_report(&report);
        let mut lines: Vec<String> = issues
            .iter()
            .map(|i| format!("- {}: {}", i.check_id, i.message))
            .collect();
        if report.passed() && !review_ready {
            lines.push(
                "- review: gates passed but design-review.json is missing; call design.request_review"
                    .into(),
            );
        }
        if lines.is_empty() {
            lines.push("- unknown failure; call design.validate and read the report".into());
        }
        repair_note = lines.join("\n");
        set_stage(state, "design", StageStatus::Running)?;
        run.save(state)?;
    }
    if !report.passed() {
        set_stage(state, "design", StageStatus::Failed)?;
        run.save(state)?;
        return Ok(StageStatus::Failed);
    }
    // The review request must exist before the package can wait for
    // approval; the Markdown view is generated from the JSON contracts.
    if !passed_with_review || !attempt_dir.join("design-review.json").is_file() {
        set_stage(state, "design", StageStatus::Failed)?;
        state
            .task_mut("design", "design_package")
            .stopped_reason = Some("review_not_requested".into());
        run.save(state)?;
        return Ok(StageStatus::Failed);
    }
    let review_path = crate::pipeline::review_view::render_review(&attempt_dir, &state.run_id)?;
    let review_digest = crate::digest::digest_bytes(
        fs::read_to_string(&review_path)?.as_bytes(),
    );
    fs::copy(&review_path, run.reports_dir()?.join("design-review.md"))?;
    events.append(Event::ReviewRequested {
        stage: "design".into(),
        task_id: "design_package".into(),
        digest: review_digest,
    })?;
    // Commit the candidate before waiting. `approve` binds this digest;
    // without a revision the approval command has nothing to record and a
    // later assets run cannot reuse the package.
    let revision = format!("d{}", chrono_like_id(state.created_ms));
    let (_dir, digest) = artifacts.commit_revision("design", &revision, &attempt_dir)?;
    let mut identity = artifacts.identity()?;
    identity.design_revision = Some(revision.clone());
    artifacts.save_identity(&identity)?;
    state.design_revision = Some(revision.clone());
    events.append(Event::RevisionCommitted {
        kind: "design".into(),
        revision: revision.clone(),
        digest: digest.clone(),
    })?;
    if cfg.require_design_approval {
        set_stage(state, "design", StageStatus::WaitingApproval)?;
        run.save(state)?;
        return Ok(StageStatus::WaitingApproval);
    }
    set_stage(state, "design", StageStatus::Succeeded)?;
    run.save(state)?;
    Ok(StageStatus::Succeeded)
}

fn chrono_like_id(ms: u64) -> String {
    // Deterministic-ish revision suffix from the run creation time.
    ms.to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn ctx_with(dir: &Path) -> ToolContext {
        ToolContext {
            host: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            stage: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            role: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            attempt_dir: dir.to_path_buf(),
            template_roots: vec![dir.to_path_buf()],
            genre: "top_down_tank".into(),
            design_dir: None,
            pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
        }
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn valid_package(dir: &Path) {
        write(
            dir,
            "game-design.json",
            r#"{
              "schema_version": 1, "game_id": "tank_slice", "genre": "top_down_tank",
              "core_loop": "move, shoot, reach exit",
              "actions": [{"id": "move", "name": "Move", "input": "wasd", "desc": ""}],
              "entities": [{"id": "player_tank", "kind": "player", "desc": "", "actions": ["move"], "speed": 3.0}],
              "rules": [{"id": "r1", "statement": "s", "entities": ["player_tank"], "actions": ["move"]}],
              "win_condition": {"id": "win", "statement": "reach exit", "refs": ["player_tank"]},
              "requirement_ids": [], "scope": {}
            }"#,
        );
        write(
            dir,
            "style-guide.json",
            r#"{
              "schema_version": 1, "logical_resolution": [320, 240],
              "tile_size_px": 16, "sprite_size_px": 16,
              "palette": "palette.json", "transparent_index": 0,
              "outline": "always", "directions": ["N", "E", "S", "W"],
              "anim_fps_min": 2, "anim_fps_max": 16, "nearest_filtering": true
            }"#,
        );
        write(
            dir,
            "levels/level_1.json",
            r#"{
              "schema_version": 1, "id": "level_1", "name": "First Steps",
              "tile_size_px": 16, "grid_w": 8, "grid_h": 8,
              "grid": ["ffffffff", "ffffbfff", "ffffffff", "ffwbbwff", "ffffffff", "fbffffbf", "ffffffff", "ffffffff"],
              "tile_semantics": [
                {"tile_id": "f", "behaviour": "floor", "blocks": false},
                {"tile_id": "b", "behaviour": "wall", "blocks": true},
                {"tile_id": "w", "behaviour": "water", "blocks": true}
              ],
              "placements": [{"entity_id": "enemy_tank", "pos": [6, 1], "dir": "W"}],
              "spawn": [1, 1], "exit": [6, 6],
              "objective": "reach the exit", "difficulty": 1, "seed": 1, "notes": ""
            }"#,
        );
        write(
            dir,
            "asset-requirements.json",
            r#"{
              "schema_version": 1,
              "requirements": [
                {"asset_id": "player_tank", "kind": "sprite", "required": true,
                 "consumer_refs": ["level_1"], "style_ref": "palette.json",
                 "size_px": [16, 16], "directions": ["N","E","S","W"],
                 "anims": [{"name": "move", "fps": 10, "looped": true, "frames": 4, "dirs": ["N","E","S","W"]}],
                 "semantics": "player", "strategy": "shapes", "seamless": false, "priority": 1},
                {"asset_id": "ground", "kind": "tile", "required": true,
                 "consumer_refs": ["level_1"], "style_ref": "palette.json",
                 "size_px": [16, 16], "directions": [], "anims": [],
                 "semantics": "floor", "strategy": "tile_gen", "seamless": true, "priority": 1},
                {"asset_id": "b", "kind": "tile", "required": true,
                 "consumer_refs": ["level_1"], "style_ref": "palette.json",
                 "size_px": [16, 16], "directions": [], "anims": [],
                 "semantics": "wall", "strategy": "tile_gen", "seamless": true, "priority": 1},
                {"asset_id": "w", "kind": "tile", "required": true,
                 "consumer_refs": ["level_1"], "style_ref": "palette.json",
                 "size_px": [16, 16], "directions": [], "anims": [],
                 "semantics": "water", "strategy": "tile_gen", "seamless": true, "priority": 1},
                {"asset_id": "enemy_tank", "kind": "sprite", "required": true,
                 "consumer_refs": ["level_1"], "style_ref": "palette.json",
                 "size_px": [16, 16], "directions": ["N","E","S","W"], "anims": [],
                 "semantics": "enemy", "strategy": "shapes", "seamless": false, "priority": 1}
              ]
            }"#,
        );
        write(
            dir,
            "acceptance-plan.json",
            r#"{
              "schema_version": 1,
              "items": [
                {"requirement_id": "player_tank", "description": "delivered", "gate_id": "assets.coverage", "stage": "assets", "evidence": "manifest"},
                {"requirement_id": "ground", "description": "delivered", "gate_id": "assets.coverage", "stage": "assets", "evidence": "manifest"},
                {"requirement_id": "b", "description": "delivered", "gate_id": "assets.coverage", "stage": "assets", "evidence": "manifest"},
                {"requirement_id": "w", "description": "delivered", "gate_id": "assets.coverage", "stage": "assets", "evidence": "manifest"},
                {"requirement_id": "enemy_tank", "description": "delivered", "gate_id": "assets.coverage", "stage": "assets", "evidence": "manifest"}
              ]
            }"#,
        );
    }

    #[test]
    fn gates_pass_valid_package_and_catch_breaks() {
        let dir = std::env::temp_dir().join(format!("pxl_h_design_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        valid_package(&dir);
        let ctx = ctx_with(&dir);
        let report = gates(&ctx).unwrap();
        assert!(report.passed(), "{:?}", report.failed_checks());

        // Missing victory conditions and malformed levels must fail closed.
        let game_path = dir.join("game-design.json");
        let original = fs::read_to_string(&game_path).unwrap();
        let mut game: Value = serde_json::from_str(&original).unwrap();
        game["win_condition"] = Value::Null;
        fs::write(&game_path, serde_json::to_string(&game).unwrap()).unwrap();
        assert!(!gates(&ctx).unwrap().passed());
        fs::write(&game_path, original).unwrap();
        fs::write(dir.join("levels/broken.json"), "{broken").unwrap();
        assert!(!gates(&ctx).unwrap().passed());
        fs::remove_file(dir.join("levels/broken.json")).unwrap();

        // Break: spawn outside the grid.
        let bad = dir.join("levels/level_2.json");
        fs::write(
            &bad,
            r#"{
              "schema_version": 1, "id": "level_2", "name": "Bad", "tile_size_px": 16,
              "grid_w": 4, "grid_h": 2, "grid": ["ffff", "ffff"],
              "tile_semantics": [{"tile_id": "f", "behaviour": "floor", "blocks": false}],
              "placements": [], "spawn": [9, 9], "exit": [1, 1],
              "objective": "", "difficulty": 1, "seed": 1, "notes": ""
            }"#,
        )
        .unwrap();
        let report = gates(&ctx).unwrap();
        assert!(!report.passed());
        assert!(report
            .failed_checks()
            .iter()
            .any(|c| c.id.contains("level_2")));
        fs::remove_file(&bad).unwrap();

        // Break: unreachable exit behind walls.
        fs::write(
            dir.join("levels/level_1.json"),
            r#"{
              "schema_version": 1, "id": "level_1", "name": "Blocked", "tile_size_px": 16,
              "grid_w": 4, "grid_h": 3, "grid": ["ffff", "bbbb", "ffff"],
              "tile_semantics": [
                {"tile_id": "f", "behaviour": "floor", "blocks": false},
                {"tile_id": "b", "behaviour": "wall", "blocks": true}
              ],
              "placements": [], "spawn": [0, 0], "exit": [2, 2],
              "objective": "", "difficulty": 1, "seed": 1, "notes": ""
            }"#,
        )
        .unwrap();
        let report = gates(&ctx).unwrap();
        assert!(!report.passed());
        assert!(report
            .failed_checks()
            .iter()
            .any(|c| c.failures.iter().any(|f| f.contains("reachable"))));
    }

    #[test]
    fn invalid_contract_reports_serde_error_not_missing() {
        let dir = std::env::temp_dir().join(format!("pxl_h_design_bad_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("game-design.json"), r#"{"schema_version":1,"game_id":"x"}"#).unwrap();
        let report = gates(&ctx_with(&dir)).unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.id == "files.game_design")
            .unwrap();
        assert!(!check.passed);
        assert_eq!(check.value, "invalid");
        assert!(
            check.failures.iter().any(|f| f.contains("game-design.json")),
            "{:?}",
            check.failures
        );
        assert!(
            !check.failures.iter().any(|f| f == "missing"),
            "{:?}",
            check.failures
        );
    }

    #[test]
    fn write_candidate_rejects_escape_and_bad_json() {
        let dir = std::env::temp_dir()
            .join(format!("pxl_h_design_wc_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ctx = ctx_with(&dir);
        assert!(write_candidate(&ctx, "../evil.json", "{}").is_err());
        assert!(write_candidate(&ctx, "game-design.json", "{nope").is_err());
        assert!(write_candidate(&ctx, "levels/l1.json", "{}").is_ok());
        // Unknown file names are refused.
        let out = write_candidate(&ctx, "random.json", "{}").unwrap();
        assert!(!out.ok);
    }

    #[test]
    fn read_template_and_request_review_tools_work() {
        let root = std::env::temp_dir()
            .join(format!("pxl_h_design_tpl_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        // Layout mirrors harness/templates/: <root>/templates/<genre>/.
        let genre_dir = root.join("templates").join("top_down_tank");
        fs::create_dir_all(genre_dir.join("assets")).unwrap();
        fs::write(genre_dir.join("template.json"), r#"{"genre":"top_down_tank","core_loop":"move, shoot"}"#).unwrap();
        fs::write(genre_dir.join("assets/palette.json"), "{}").unwrap();
        let attempt = root.join("attempt");
        fs::create_dir_all(&attempt).unwrap();
        let ctx = ToolContext {
            host: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            stage: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            role: PermissionSet::default()
                .allow(Permission::ReadProject)
                .allow(Permission::WriteCandidate),
            attempt_dir: attempt.clone(),
            template_roots: vec![root.clone()],
            genre: "top_down_tank".into(),
            design_dir: None,
            pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
        };
        let reg = registry();
        // read_template finds the genre template and lists its assets.
        let out = reg
            .dispatch("design.read_template", &serde_json::json!({}), &ctx)
            .unwrap();
        assert!(out.ok, "{}", out.summary);
        assert!(out.data["template"]["genre"] == "top_down_tank");
        assert!(out.data["available_assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "assets/palette.json"));
        // Missing genre fails cleanly.
        let missing = ToolContext { genre: "nope".into(), ..ctx };
        let out = reg
            .dispatch("design.read_template", &serde_json::json!({}), &missing)
            .unwrap();
        assert!(!out.ok);
        // request_review writes the review request into the candidate.
        let out = reg
            .dispatch(
                "design.request_review",
                &serde_json::json!({
                    "summary": "3 levels complete",
                    "risks": ["water readability"],
                    "open_questions": ["enemy density"]
                }),
                &missing,
            )
            .unwrap();
        assert!(out.ok, "{}", out.summary);
        let review: Value = serde_json::from_str(
            &fs::read_to_string(attempt.join("design-review.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(review["summary"], "3 levels complete");
        assert_eq!(review["risks"][0], "water readability");
        // Empty summaries are refused.
        let out = reg
            .dispatch("design.request_review", &serde_json::json!({"summary": " "}), &missing)
            .unwrap();
        assert!(!out.ok);
    }
}
