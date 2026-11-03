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
        pxl: std::sync::Arc::new(crate::pxl_adapter::PxlAdapter::unavailable()),
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
    assert!(
        !glob_matches("*.json", "out/palette.json"),
        "single star stays in one segment"
    );
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
        .dispatch(
            "fs.write",
            &serde_json::json!({"path": "../x", "content": "x"}),
            &c
        )
        .is_err());
    assert!(reg
        .dispatch(
            "fs.read",
            &serde_json::json!({"path": "../../../etc/passwd"}),
            &c
        )
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
    assert!(out.data["matches"][0]
        .as_str()
        .unwrap()
        .contains("a.json:2"));
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
