//! Markdown review view over the design package. JSON contracts stay the
//! source of truth; this module renders them for human review. It must
//! never invent facts that are absent from the JSON files.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::Result;

/// Render `design-review.md` from the candidate package. Returns the path
/// of the written file.
pub fn render_review(dir: &Path, run_id: &str) -> Result<String> {
    let mut md = String::new();
    md.push_str("# Design package review\n\n");
    md.push_str(&format!("- run: `{run_id}`\n"));
    let game = read_json(dir, "game-design.json");
    let style = read_json(dir, "style-guide.json");
    let reqs = read_json(dir, "asset-requirements.json");
    let accept = read_json(dir, "acceptance-plan.json");
    let review = read_json(dir, "design-review.json");

    if let Some(g) = &game {
        md.push_str(&format!(
            "\n## Game: {} ({})\n\n{}\n\n",
            get_owned(g, "game_id"),
            get_owned(g, "genre"),
            get_owned(g, "core_loop")
        ));
        md.push_str("### Actions\n\n");
        md.push_str("| id | name | input |\n|---|---|---|\n");
        for a in items(g, "actions") {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                get_owned(a, "id"),
                get_owned(a, "name"),
                get_owned(a, "input")
            ));
        }
        md.push_str("\n### Entities\n\n");
        md.push_str("| id | kind | actions |\n|---|---|---|\n");
        for e in items(g, "entities") {
            let actions = e
                .get("actions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                get_owned(e, "id"),
                get_owned(e, "kind"),
                actions
            ));
        }
        md.push_str("\n### Rules\n\n");
        for r in items(g, "rules") {
            md.push_str(&format!(
                "- **{}**: {}\n",
                get_owned(r, "id"),
                get_owned(r, "statement")
            ));
        }
        for name in ["win_condition", "lose_condition"] {
            if let Some(c) = g.get(name) {
                if !c.is_null() {
                    md.push_str(&format!(
                        "\n**{}**: {}\n",
                        name.replace('_', " "),
                        get_owned(c, "statement")
                    ));
                }
            }
        }
    } else {
        md.push_str("\n> `game-design.json` is missing or unreadable.\n");
    }

    if let Some(s) = &style {
        md.push_str("\n## Style guide\n\n");
        let res = s
            .get("logical_resolution")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join("×")
            })
            .unwrap_or_else(|| "?".into());
        md.push_str(&format!(
            "- logical resolution: {res}\n- tile size: {}px\n- sprite size: {}px\n- palette: `{}`\n- directions: {}\n- outline: {}\n",
            get_owned(s, "tile_size_px"),
            get_owned(s, "sprite_size_px"),
            get_owned(s, "palette"),
            join(s, "directions"),
            get_owned(s, "outline"),
        ));
    }

    // Levels: list each level file with its grid rendered as a code block.
    let levels_dir = dir.join("levels");
    let mut level_files: Vec<String> = Vec::new();
    if levels_dir.is_dir() {
        for entry in fs::read_dir(&levels_dir)?.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(n) = entry.file_name().to_str() {
                    level_files.push(n.to_string());
                }
            }
        }
        level_files.sort();
    }
    if !level_files.is_empty() {
        md.push_str("\n## Levels\n\n");
    }
    for name in &level_files {
        let level = read_json(&levels_dir, name).unwrap_or(Value::Null);
        md.push_str(&format!(
            "### {} — {}\n\n",
            get(&level, "id").unwrap_or_else(|| name.trim_end_matches(".json")),
            get(&level, "name").unwrap_or("?")
        ));
        if let Some(objective) = get(&level, "objective") {
            if !objective.is_empty() {
                md.push_str(&format!("Objective: {objective}\n\n"));
            }
        }
        if let Some(notes) = get(&level, "notes") {
            if !notes.is_empty() {
                md.push_str(&format!("Notes: {notes}\n\n"));
            }
        }
        if let Some(grid) = level.get("grid").and_then(|v| v.as_array()) {
            md.push_str("```text\n");
            for row in grid {
                if let Some(r) = row.as_str() {
                    md.push_str(r);
                    md.push('\n');
                }
            }
            md.push_str("```\n\n");
        }
    }

    if let Some(r) = &reqs {
        md.push_str("## Asset requirements\n\n");
        md.push_str("| asset | kind | required | size | directions | anims | strategy |\n");
        md.push_str("|---|---|---|---|---|---|---|\n");
        for req in items(r, "requirements") {
            let anims = req
                .get("anims")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                get_owned(req, "asset_id"),
                get_owned(req, "kind"),
                if req.get("required").and_then(|v| v.as_bool()).unwrap_or(false) { "yes" } else { "no" },
                pair(req, "size_px"),
                join(req, "directions"),
                anims,
                get_owned(req, "strategy"),
            ));
        }
        md.push('\n');
    }

    if let Some(a) = &accept {
        md.push_str("## Acceptance plan\n\n");
        md.push_str("| requirement | gate | stage | evidence |\n|---|---|---|---|\n");
        for item in items(a, "items") {
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                get_owned(item, "requirement_id"),
                get_owned(item, "gate_id"),
                get_owned(item, "stage"),
                get_owned(item, "evidence")
            ));
        }
        md.push('\n');
    }

    if let Some(rv) = &review {
        md.push_str("## Designer review request\n\n");
        if let Some(s) = get(rv, "summary") {
            md.push_str(s);
            md.push_str("\n\n");
        }        let risks = join_list(rv, "risks");
        if !risks.is_empty() {
            md.push_str("### Risks\n\n");
            for r in &risks {
                md.push_str(&format!("- {r}\n"));
            }
            md.push('\n');
        }
        let questions = join_list(rv, "open_questions");
        if !questions.is_empty() {
            md.push_str("### Open questions\n\n");
            for q in &questions {
                md.push_str(&format!("- {q}\n"));
            }
            md.push('\n');
        }
    } else {
        md.push_str("> `design-review.json` is missing: the designer never called `design.request_review`.\n");
    }

    let out = dir.join("design-review.md");
    fs::write(&out, &md)?;
    Ok(out.to_string_lossy().to_string())
}

fn read_json(dir: &Path, name: &str) -> Option<Value> {
    let p = dir.join(name);
    fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    match v.get(key) {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn get_owned(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "?".into(),
    }
}

fn items<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

fn join(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn join_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn pair(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_u64())
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join("×")
        })
        .unwrap_or_else(|| "?".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_markdown_from_json_contracts() {
        let dir = std::env::temp_dir().join(format!("pxl_h_review_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("levels")).unwrap();
        fs::write(
            dir.join("game-design.json"),
            r#"{"schema_version":1,"game_id":"tank_slice","genre":"top_down_tank","core_loop":"move, shoot","actions":[{"id":"move","name":"Move","input":"wasd"}],"entities":[{"id":"player_tank","kind":"player","actions":["move"]}],"rules":[{"id":"r1","statement":"player moves"}],"win_condition":{"id":"win","statement":"reach exit"},"lose_condition":null}"#,
        )
        .unwrap();
        fs::write(
            dir.join("levels/level_1.json"),
            r#"{"schema_version":1,"id":"level_1","name":"First","grid":["ffff"],"objective":"reach exit","notes":"teach movement","spawn":[0,0],"exit":[3,0]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("design-review.json"),
            r#"{"schema_version":1,"summary":"3 levels, tank template","risks":["water rendering"],"open_questions":["enemy count?"]}"#,
        )
        .unwrap();
        let path = render_review(&dir, "r1").unwrap();
        let md = fs::read_to_string(&path).unwrap();
        assert!(md.contains("# Design package review"));
        assert!(md.contains("## Game: tank_slice (top_down_tank)"));
        assert!(md.contains("### level_1 — First"));
        assert!(md.contains("teach movement"));
        assert!(md.contains("### Risks"));
        assert!(md.contains("- water rendering"));
        assert!(md.contains("### Open questions"));
        assert!(md.contains("- enemy count?"));
        assert!(md.contains("```text"));
        // Nothing invented: missing style guide simply isn't rendered.
        assert!(!md.contains("Style guide"));
    }
}
