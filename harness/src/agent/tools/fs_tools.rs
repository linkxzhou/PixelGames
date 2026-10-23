//! Generic filesystem tools (structural port of miniagent `tools/fs.rs`,
//! adapted to the harness sandbox): reads stay inside the attempt and the
//! approved template roots; writes stay inside the attempt write root.

use std::sync::Arc;

use crate::agent::tools::{Tool, ToolContext, ToolOutcome, ToolSource};
use crate::error::{HarnessError, Result};
use crate::policy::Permission;

/// Hard cap for one tool result payload rendered into the transcript.
const MAX_TEXT_BYTES: usize = 24_000;
const MAX_LIST_ENTRIES: usize = 200;

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}… [clipped {}/{} bytes]", &text[..end], end, text.len())
}

fn resolve_read(ctx: &ToolContext, rel: &str) -> Result<std::path::PathBuf> {
    let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
    root.resolve_read(rel, &ctx.template_roots)
}

struct ReadTool;

impl Tool for ReadTool {
    fn name(&self) -> &str {
        "fs.read"
    }
    fn description(&self) -> &str {
        "Read one text file inside the attempt directory or the approved template roots."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {"path": {"type": "string"}},
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::ReadProject
    }
    fn read_only(&self) -> bool {
        true
    }
    fn activity(&self) -> &str {
        "Reading file"
    }
    fn source(&self) -> ToolSource {
        ToolSource::Builtin
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome> {
        let rel = args["path"].as_str().unwrap_or_default();
        let path = resolve_read(ctx, rel)?;
        if !path.is_file() {
            return Ok(ToolOutcome::fail(format!("no such file {rel}")));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| HarnessError::io(format!("{rel}: {e}")))?;
        let clipped = clip(&text, MAX_TEXT_BYTES);
        Ok(ToolOutcome::ok(format!("read {rel} ({} bytes)", text.len()))
            .with_data(serde_json::json!({"path": rel, "content": clipped})))
    }
}

/// Minimal glob: `*` and `?` never cross `/`; `**` matches across segments.
fn glob_matches(pattern: &str, path: &str) -> bool {
    fn seg(pat: &[char], s: &[char], deep: bool) -> bool {
        if pat.is_empty() {
            return s.is_empty();
        }
        match pat[0] {
            '*' => {
                // `**` (possibly with a trailing segment start) crosses `/`.
                let double = pat.len() >= 2 && pat[1] == '*';
                let rest_start = if double { 2 } else { 1 };
                // `**/` collapses into a pure deep match.
                let (rest_start, require_sep) = if double && pat.len() > 2 && pat[2] == '/' {
                    (3, true)
                } else {
                    (rest_start, false)
                };
                let max = if double || deep { s.len() } else {
                    // single star stops at the first '/'
                    s.iter().position(|&c| c == '/').unwrap_or(s.len())
                };
                for i in 0..=max {
                    if require_sep && i > 0 && s[i - 1] != '/' {
                        continue;
                    }
                    if seg(&pat[rest_start..], &s[i..], double || deep) {
                        return true;
                    }
                }
                false
            }
            '?' => {
                if s.is_empty() || s[0] == '/' {
                    return false;
                }
                seg(&pat[1..], &s[1..], false)
            }
            c => {
                if s.is_empty() || s[0] != c {
                    return false;
                }
                seg(&pat[1..], &s[1..], false)
            }
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    seg(&p, &s, false)
}

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        if is_dir {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

struct GlobTool;

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "fs.glob"
    }
    fn description(&self) -> &str {
        "List files under the attempt directory matching a glob pattern (e.g. '*.recipe.json' or 'out/**')."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {"pattern": {"type": "string"}},
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::ReadProject
    }
    fn read_only(&self) -> bool {
        true
    }
    fn activity(&self) -> &str {
        "Listing files"
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome> {
        let pattern = args["pattern"].as_str().unwrap_or_default();
        if pattern.contains("..") || pattern.starts_with('/') {
            return Ok(ToolOutcome::fail("pattern must be relative to the attempt"));
        }
        let mut files = Vec::new();
        walk(&ctx.attempt_dir, &mut files);
        files.sort();
        let mut hits: Vec<String> = Vec::new();
        for path in files {
            let Ok(rel) = path.strip_prefix(&ctx.attempt_dir) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if glob_matches(pattern, &rel) {
                hits.push(rel);
                if hits.len() >= MAX_LIST_ENTRIES {
                    hits.push(format!("… [capped at {MAX_LIST_ENTRIES} entries]"));
                    break;
                }
            }
        }
        Ok(ToolOutcome::ok(format!("{} matches for {pattern}", hits.len()))
            .with_data(serde_json::json!({"matches": hits})))
    }
}

struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "fs.grep"
    }
    fn description(&self) -> &str {
        "Search for a substring in text files under the attempt directory. Returns up to 50 matching lines."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["needle"],
            "properties": {
                "needle": {"type": "string"},
                "glob": {"type": "string"}
            },
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::ReadProject
    }
    fn read_only(&self) -> bool {
        true
    }
    fn activity(&self) -> &str {
        "Searching files"
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome> {
        let needle = args["needle"].as_str().unwrap_or_default();
        let pattern = args["glob"].as_str().unwrap_or("*");
        if needle.is_empty() {
            return Ok(ToolOutcome::fail("needle must not be empty"));
        }
        let mut files = Vec::new();
        walk(&ctx.attempt_dir, &mut files);
        files.sort();
        let mut hits: Vec<String> = Vec::new();
        'outer: for path in files {
            let Ok(rel) = path.strip_prefix(&ctx.attempt_dir) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !glob_matches(pattern, &rel) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                if line.contains(needle) {
                    hits.push(format!("{rel}:{}: {}", i + 1, clip(line, 200)));
                    if hits.len() >= 50 {
                        break 'outer;
                    }
                }
            }
        }
        Ok(ToolOutcome::ok(format!("{} matches for '{needle}'", hits.len()))
            .with_data(serde_json::json!({"matches": hits})))
    }
}

struct WriteTool;

impl Tool for WriteTool {
    fn name(&self) -> &str {
        "fs.write"
    }
    fn description(&self) -> &str {
        "Write one text file inside the attempt directory. Parent directories are created."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::WriteCandidate
    }
    fn read_only(&self) -> bool {
        false
    }
    fn activity(&self) -> &str {
        "Writing file"
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome> {
        let rel = args["path"].as_str().unwrap_or_default();
        let content = args["content"].as_str().unwrap_or_default();
        let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
        let path = root.resolve_write(rel)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| HarnessError::io(e.to_string()))?;
        }
        std::fs::write(&path, content)
            .map_err(|e| HarnessError::io(format!("{rel}: {e}")))?;
        let digest = crate::digest::digest_bytes(content.as_bytes());
        Ok(ToolOutcome::ok(format!("wrote {rel}"))
            .with_data(serde_json::json!({"path": rel, "digest": digest}))
            .with_artifact(rel, digest, content.len() as u64))
    }
}

struct EditTool;

impl Tool for EditTool {
    fn name(&self) -> &str {
        "fs.edit"
    }
    fn description(&self) -> &str {
        "Replace the first exact occurrence of `find` with `replace` in a file inside the attempt."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "find", "replace"],
            "properties": {
                "path": {"type": "string"},
                "find": {"type": "string"},
                "replace": {"type": "string"}
            },
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::WriteCandidate
    }
    fn read_only(&self) -> bool {
        false
    }
    fn activity(&self) -> &str {
        "Editing file"
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome> {
        let rel = args["path"].as_str().unwrap_or_default();
        let find = args["find"].as_str().unwrap_or_default();
        let replace = args["replace"].as_str().unwrap_or_default();
        let root = crate::policy::path::WriteRoot::new(&ctx.attempt_dir);
        let path = root.resolve_write(rel)?;
        if !path.is_file() {
            return Ok(ToolOutcome::fail(format!("no such file {rel}")));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| HarnessError::io(format!("{rel}: {e}")))?;
        let Some(pos) = text.find(find) else {
            return Ok(ToolOutcome::fail(format!("'{find}' not found in {rel}")));
        };
        let updated = format!(
            "{}{}{}",
            &text[..pos],
            replace,
            &text[pos + find.len()..]
        );
        std::fs::write(&path, &updated)
            .map_err(|e| HarnessError::io(format!("{rel}: {e}")))?;
        let digest = crate::digest::digest_bytes(updated.as_bytes());
        Ok(ToolOutcome::ok(format!("edited {rel}"))
            .with_data(serde_json::json!({"path": rel, "digest": digest})))
    }
}

pub fn builtin_fs_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(ReadTool),
        Arc::new(GlobTool),
        Arc::new(GrepTool),
        Arc::new(WriteTool),
        Arc::new(EditTool),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::ToolRegistry;
    use crate::policy::{Permission, PermissionSet};

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
            pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
            todos: None,
        }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl_h_fs_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn glob_semantics() {
        assert!(glob_matches("*.json", "palette.json"));
        assert!(!glob_matches("*.json", "out/palette.json"), "single star stays in one segment");
        assert!(glob_matches("out/**", "out/a/b.png"));
        assert!(glob_matches("**", "a/b/c.txt"));
        assert!(glob_matches("**/*.recipe.json", "a/b/x.recipe.json"));
        assert!(glob_matches("a?c.txt", "abc.txt"));
        assert!(!glob_matches("a?c/*.txt", "ab/c.txt"), "? never matches /");
    }

    #[test]
    fn write_read_roundtrip_and_escape_rejected() {
        let dir = tmp("rw");
        let c = ctx(&dir);
        let mut reg = ToolRegistry::new();
        for t in builtin_fs_tools() {
            reg.register_tool(t);
        }
        let out = reg
            .dispatch(
                "fs.write",
                &serde_json::json!({"path": "sub/notes.md", "content": "hello"}),
                &c,
            )
            .unwrap();
        assert!(out.ok);
        let read = reg
            .dispatch("fs.read", &serde_json::json!({"path": "sub/notes.md"}), &c)
            .unwrap();
        assert!(read.ok);
        assert!(read.data["content"].as_str().unwrap().contains("hello"));
        // Escapes are hard errors, not tool failures.
        assert!(reg
            .dispatch("fs.write", &serde_json::json!({"path": "../x", "content": "x"}), &c)
            .is_err());
        assert!(reg
            .dispatch("fs.read", &serde_json::json!({"path": "../../../etc/passwd"}), &c)
            .is_err());
    }

    #[test]
    fn grep_finds_lines() {
        let dir = tmp("grep");
        std::fs::write(dir.join("a.json"), "{\n  \"seed\": 7\n}\n").unwrap();
        let c = ctx(&dir);
        let mut reg = ToolRegistry::new();
        for t in builtin_fs_tools() {
            reg.register_tool(t);
        }
        let out = reg
            .dispatch("fs.grep", &serde_json::json!({"needle": "seed"}), &c)
            .unwrap();
        assert!(out.ok);
        assert!(out.data["matches"][0].as_str().unwrap().contains("a.json:2"));
    }

    #[test]
    fn edit_requires_unique_find() {
        let dir = tmp("edit");
        std::fs::write(dir.join("f.txt"), "alpha beta gamma").unwrap();
        let c = ctx(&dir);
        let mut reg = ToolRegistry::new();
        for t in builtin_fs_tools() {
            reg.register_tool(t);
        }
        let out = reg
            .dispatch(
                "fs.edit",
                &serde_json::json!({"path": "f.txt", "find": "beta", "replace": "delta"}),
                &c,
            )
            .unwrap();
        assert!(out.ok);
        assert_eq!(
            std::fs::read_to_string(dir.join("f.txt")).unwrap(),
            "alpha delta gamma"
        );
        let miss = reg
            .dispatch(
                "fs.edit",
                &serde_json::json!({"path": "f.txt", "find": "missing", "replace": "x"}),
                &c,
            )
            .unwrap();
        assert!(!miss.ok);
    }
}
