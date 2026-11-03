//! Design gates, absorbed from `harness/src/pipeline/design_stage.rs` in M3
//! (dual-track: the harness keeps its own copy until M6). The signature takes
//! a plain directory so `step-100 gate` runs without a model or a run store.

use std::fs;
use std::path::{Path, PathBuf};

use crate::contracts::{AcceptancePlan, AssetRequirements, GameDesign, LevelDef, StyleGuide};
use pxl_contract::error::Result;
use pxl_contract::visual::VisualIntent;
use pxl_store::qa::{GateCheck, GateReport, GateStatus, GATE_REPORT_VERSION};

pub const DESIGN_FILES: &[&str] = &[
    "brief.json",
    "game-design.json",
    "style-guide.json",
    "visual-intent.json",
    "tuning.json",
    "levels",
    "asset-requirements.json",
    "audio-requirements.json",
    "acceptance-plan.json",
    "design-review.json",
];

/// Hard design gates (D0). JSON contracts are the fact source.
pub fn gates(dir: &Path) -> Result<GateReport> {
    let mut checks: Vec<GateCheck> = Vec::new();

    // A file that exists but does not match the contract must report the
    // serde error. Treating that as "missing" leaves the designer with
    // nothing to repair.
    enum Loaded<T> {
        Missing,
        Invalid(String),
        Ready(T),
    }
    fn load_contract<T: serde::de::DeserializeOwned>(
        dir: &std::path::Path,
        name: &str,
    ) -> Loaded<T> {
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
    let visual = load_contract::<VisualIntent>(dir, "visual-intent.json");
    let reqs = load_contract::<AssetRequirements>(dir, "asset-requirements.json");
    let accept = load_contract::<AcceptancePlan>(dir, "acceptance-plan.json");
    push_file_check(
        &mut checks,
        "files.game_design",
        "game-design.json parses",
        &game,
    );
    push_file_check(
        &mut checks,
        "files.style_guide",
        "style-guide.json parses",
        &style,
    );
    push_file_check(
        &mut checks,
        "files.visual_intent",
        "visual-intent.json parses with visual-intent/v1 or visual-intent/v2",
        &visual,
    );
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
    let visual = match visual {
        Loaded::Ready(v) => Some(v),
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
    let contract_versions = [
        game.as_ref().map(|v| v.header.schema_version),
        style.as_ref().map(|v| v.header.schema_version),
        reqs.as_ref().map(|v| v.header.schema_version),
        accept.as_ref().map(|v| v.header.schema_version),
    ];
    let versions_ok = contract_versions
        .into_iter()
        .flatten()
        .all(|v| v == crate::contracts::DESIGN_SCHEMA_VERSION);
    checks.push(GateCheck {
        id: "contract.design_v3".into(),
        passed: versions_ok,
        value: "design/v3".into(),
        limit: "all primary design documents use schema_version 3".into(),
        failures: if versions_ok {
            vec![]
        } else {
            vec!["regenerate design documents with the design/v3 workflow".into()]
        },
    });
    let mut visual_errors = visual
        .as_ref()
        .map(|intent| intent.validate())
        .unwrap_or_else(|| vec!["visual-intent.json is required".into()]);
    if let Some(intent) = &visual {
        let profile_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../knowledge/styles")
            .join(format!("{}.json", intent.profile.id.replace('_', "-")));
        match fs::read(&profile_path) {
            Ok(bytes) => {
                if pxl_store::digest::digest_bytes(&bytes) != intent.profile.digest {
                    visual_errors.push(format!(
                        "profile '{}' digest does not match {}",
                        intent.profile.id,
                        profile_path.display()
                    ));
                }
                if serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|profile| profile["id"].as_str().map(str::to_string))
                    .as_deref()
                    != Some(intent.profile.id.as_str())
                {
                    visual_errors.push(format!(
                        "profile '{}' id does not match its knowledge file",
                        intent.profile.id
                    ));
                }
            }
            Err(error) => visual_errors.push(format!(
                "profile '{}' cannot be read at {}: {error}",
                intent.profile.id,
                profile_path.display()
            )),
        }
    }
    if let (Some(intent), Some(requirements)) = (&visual, &reqs) {
        for requirement in &requirements.requirements {
            let Some(policy) = intent
                .asset_policies
                .iter()
                .find(|policy| policy.asset_id == requirement.asset_id)
            else {
                visual_errors.push(format!(
                    "asset '{}' has no visual policy",
                    requirement.asset_id
                ));
                continue;
            };
            let native = policy.native_size.unwrap_or(intent.pixel_grid.native_size);
            if [
                u32::from(native[0]),
                u32::from(native[1]),
            ] != requirement.size_px
            {
                visual_errors.push(format!(
                    "asset '{}' native size {}x{} does not match requirement {}x{}",
                    requirement.asset_id,
                    native[0],
                    native[1],
                    requirement.size_px[0],
                    requirement.size_px[1]
                ));
            }
        }
    }
    if let (Some(intent), Some(s)) = (&visual, &style) {
        if let Some(modern) = &intent.modern {
            let canvas = [u32::from(modern.logical_canvas[0]), u32::from(modern.logical_canvas[1])];
            if canvas != s.logical_resolution {
                visual_errors.push(format!("modern logical_canvas {}x{} does not match style-guide logical_resolution {}x{}", canvas[0], canvas[1], s.logical_resolution[0], s.logical_resolution[1]));
            }
            if modern.tile_size[0] != modern.tile_size[1] || u32::from(modern.tile_size[0]) != s.tile_size_px {
                visual_errors.push(format!("modern tile_size {}x{} must be square and equal style-guide tile_size_px {}", modern.tile_size[0], modern.tile_size[1], s.tile_size_px));
            }
        }
    }
    checks.push(GateCheck {
        id: "visual.intent".into(),
        passed: visual_errors.is_empty(),
        value: visual
            .as_ref()
            .map(|intent| format!("{} asset policies", intent.asset_policies.len()))
            .unwrap_or_else(|| "missing".into()),
        limit: "visual intent, material/region references, scenes and native sizes close".into(),
        failures: visual_errors.clone(),
    });
    failures.extend(visual_errors);
    if let Some(g) = &game {
        match crate::contracts::validate_ids(g) {
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
        let tile_ids: Vec<&str> = level
            .tile_semantics
            .iter()
            .map(|t| t.tile_id.as_str())
            .collect();
        for (y, row) in level.grid.iter().enumerate() {
            if row.len() as u32 != level.grid_w {
                level_failures.push(format!("row {y} width {} != {}", row.len(), level.grid_w));
            }
            for (x, ch) in row.chars().enumerate() {
                let tile_id = char_to_tile_id(ch);
                if !tile_ids.contains(&tile_id.as_str()) {
                    level_failures
                        .push(format!("cell ({x},{y}) '{ch}' has no tile_semantics entry"));
                }
            }
        }
        let in_bounds = |p: &[u32; 2]| p[0] < level.grid_w && p[1] < level.grid_h;
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
                if t.behaviour != crate::contracts::TileBehaviour::Floor
                    && t.behaviour != crate::contracts::TileBehaviour::Decor
                    && !covered.contains(&&t.tile_id)
                {
                    coverage_failures
                        .push(format!("tile '{}' has no asset requirement", t.tile_id));
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
                if r.required && r.acceptance.is_empty() {
                    style_failures.push(format!(
                        "required asset '{}' has no executable acceptance items",
                        r.asset_id
                    ));
                }
                for d in &r.directions {
                    if !s.directions.contains(d) {
                        style_failures.push(format!(
                            "asset '{}' wants direction '{}' not in style guide",
                            r.asset_id, d
                        ));
                    }
                }
                if r.size_px.contains(&0) || r.size_px.iter().any(|v| *v > pxl_core::MAX_DIM as u32)
                {
                    style_failures.push(format!(
                        "asset '{}' size {:?} is outside the supported 1..=256 canvas",
                        r.asset_id, r.size_px
                    ));
                }
                if matches!(
                    r.strategy,
                    crate::contracts::AssetStrategy::Svg | crate::contracts::AssetStrategy::Ascii
                ) && (r.size_px[0] != r.size_px[1]
                    || !pxl_core::SIZE_TIERS.contains(&(r.size_px[0] as u16)))
                {
                    style_failures.push(format!(
                        "asset '{}' {:?} requires one square 32/64/128/256 size tier",
                        r.asset_id, r.strategy
                    ));
                }
                if r.anims
                    .iter()
                    .any(|a| a.frames == 0 || a.frames as usize > pxl_core::MAX_ANIM_FRAMES)
                {
                    style_failures.push(format!(
                        "asset '{}' animation frame counts must be 1..=16",
                        r.asset_id
                    ));
                }
                let mut acceptance_ids = std::collections::BTreeSet::new();
                for item in &r.acceptance {
                    if !acceptance_ids.insert(&item.id) {
                        style_failures.push(format!(
                            "asset '{}' has duplicate acceptance id '{}'",
                            r.asset_id, item.id
                        ));
                    }
                    if !crate::contracts::ACCEPTANCE_CHECKS.contains(&item.check.as_str()) {
                        style_failures.push(format!(
                            "asset '{}' uses unknown acceptance check '{}'",
                            r.asset_id, item.check
                        ));
                    }
                    let machine = matches!(item.kind, crate::contracts::AcceptanceType::Machine);
                    if machine != item.min_score.is_none() {
                        style_failures.push(format!(
                            "asset '{}' acceptance '{}' mixes human and machine fields",
                            r.asset_id, item.id
                        ));
                    }
                    if item.check == "size.equals"
                        && item
                            .expected
                            .as_ref()
                            .and_then(|v| v.as_array())
                            .map(|v| v.len() == 2)
                            != Some(true)
                    {
                        style_failures.push(format!(
                            "asset '{}' size.equals needs expected [w,h]",
                            r.asset_id
                        ));
                    } else if item.check == "size.equals"
                        && item.expected.as_ref() != Some(&serde_json::json!(r.size_px))
                    {
                        style_failures.push(format!(
                            "asset '{}' size acceptance differs from size_px",
                            r.asset_id
                        ));
                    }
                    if machine
                        && crate::contracts::ACCEPTANCE_CHECKS[7..].contains(&item.check.as_str())
                        || !machine
                            && crate::contracts::ACCEPTANCE_CHECKS[..7]
                                .contains(&item.check.as_str())
                    {
                        style_failures.push(format!(
                            "asset '{}' acceptance '{}' uses the wrong human/machine type",
                            r.asset_id, item.id
                        ));
                    }
                    if matches!(
                        item.check.as_str(),
                        "anim.frames" | "anim.dirs" | "anim.anchor"
                    ) {
                        let Some(name) = item.anim.as_deref() else {
                            style_failures.push(format!(
                                "asset '{}' '{}' needs anim",
                                r.asset_id, item.check
                            ));
                            continue;
                        };
                        let Some(anim) = r.anims.iter().find(|anim| anim.name == name) else {
                            style_failures.push(format!(
                                "asset '{}' references unknown anim '{name}'",
                                r.asset_id
                            ));
                            continue;
                        };
                        if item.check == "anim.frames"
                            && item.expected.as_ref() != Some(&serde_json::json!(anim.frames))
                        {
                            style_failures.push(format!(
                                "asset '{}' anim.frames differs from the requirement",
                                r.asset_id
                            ));
                        }
                        if item.check == "anim.dirs"
                            && item.expected.as_ref() != Some(&serde_json::json!(anim.dirs))
                        {
                            style_failures.push(format!(
                                "asset '{}' anim.dirs differs from the requirement",
                                r.asset_id
                            ));
                        }
                        if item.check == "anim.anchor" && r.anchor.is_none() {
                            style_failures.push(format!(
                                "asset '{}' anim.anchor requires an asset anchor",
                                r.asset_id
                            ));
                        }
                    }
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
            value: if gaps.is_empty() {
                "covered".into()
            } else {
                format!("{} gaps", gaps.len())
            },
            limit: "required requirements have acceptance items".into(),
            failures: gaps,
        });
    }

    // Audio requirements are optional. When the file exists it must parse,
    // pass its own rules, and use ids that do not collide with asset ids.
    let audio_path = dir.join(crate::contracts::AUDIO_REQUIREMENTS_FILE);
    if audio_path.is_file() {
        let loaded = load_contract::<crate::contracts::AudioRequirements>(
            dir,
            crate::contracts::AUDIO_REQUIREMENTS_FILE,
        );
        push_file_check(
            &mut checks,
            "files.audio_requirements",
            "audio-requirements.json parses",
            &loaded,
        );
        if let Loaded::Ready(audio) = loaded {
            let mut problems = crate::contracts::validate_audio_requirements(&audio);
            if let Some(r) = &reqs {
                for a in &audio.requirements {
                    if r.requirements.iter().any(|x| x.asset_id == a.audio_id) {
                        problems.push(format!(
                            "audio id '{}' is also an asset id; ids must be unique across both",
                            a.audio_id
                        ));
                    }
                }
            }
            checks.push(GateCheck {
                id: "audio.requirements".into(),
                passed: problems.is_empty(),
                value: if problems.is_empty() {
                    format!("{} sounds", audio.requirements.len())
                } else {
                    format!("{} problems", problems.len())
                },
                limit: "audio-requirements.json is valid with unique stable ids".into(),
                failures: problems,
            });
        }
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
        status: if passed {
            GateStatus::Passed
        } else {
            GateStatus::Failed
        },
        input_digest: design_input_digest(dir),
        checks,
        metrics: [("levels".into(), levels.len() as u64)]
            .into_iter()
            .collect(),
        evidence: vec![],
        tool_versions: [("harness".into(), crate::VERSION.into())]
            .into_iter()
            .collect(),
        executed_ms: 0,
    })
}

fn design_input_digest(dir: &Path) -> String {
    let files = crate::run::revision_files(dir).unwrap_or_default();
    let pairs = files
        .iter()
        .map(|(path, digest)| (path.as_str(), digest.as_str()))
        .collect::<Vec<_>>();
    pxl_store::digest::digest_pairs(pairs)
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
