//! Structured task list (structural port of miniagent `tools/todo.rs`).
//! The todo list is the model's own scratch space; harness gates never read
//! it — `asset_queue` remains the source of truth for stage progress.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::agent::tools::{Tool, ToolContext, ToolOutcome, ToolSource};
use crate::error::{HarnessError, Result};
use crate::policy::Permission;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "pending",
            TodoStatus::InProgress => "in_progress",
            TodoStatus::Completed => "completed",
            TodoStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

/// Shared todo state. Interior-mutable so the tool closure and the runner
/// can both hold handles.
#[derive(Default)]
pub struct TodoManager {
    items: Mutex<Vec<TodoItem>>,
}

impl TodoManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the whole list (the `todo.write` contract: full-list writes,
    /// no incremental patching — keeps the model honest about state).
    pub fn set(&self, items: Vec<TodoItem>) {
        let mut guard = self.items.lock().unwrap_or_else(|e| e.into_inner());
        *guard = items;
    }

    pub fn items(&self) -> Vec<TodoItem> {
        self.items
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Render the block injected into the next turn's context.
    pub fn render(&self) -> String {
        let items = self.items();
        if items.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Todo\n");
        for item in &items {
            let mark = match item.status {
                TodoStatus::Completed => "[x]",
                TodoStatus::InProgress => "[>]",
                TodoStatus::Cancelled => "[-]",
                TodoStatus::Pending => "[ ]",
            };
            out.push_str(&format!("{} {}\n", mark, item.content));
        }
        out
    }
}

fn parse_items(args: &serde_json::Value) -> Result<Vec<TodoItem>> {
    let list = args["todos"]
        .as_array()
        .ok_or_else(|| HarnessError::usage("todos must be an array"))?;
    let mut items = Vec::new();
    for (i, entry) in list.iter().enumerate() {
        let content = entry["content"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        if content.is_empty() {
            return Err(HarnessError::usage(format!("todos[{i}].content must not be empty")));
        }
        if content.len() > 200 {
            return Err(HarnessError::usage(format!(
                "todos[{i}].content exceeds 200 chars"
            )));
        }
        let status = match entry["status"].as_str().unwrap_or("pending") {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            "cancelled" => TodoStatus::Cancelled,
            other => {
                return Err(HarnessError::usage(format!(
                    "todos[{i}].status '{other}' is not one of pending/in_progress/completed/cancelled"
                )))
            }
        };
        items.push(TodoItem {
            id: format!("t{i}"),
            content,
            status,
        });
    }
    if items.len() > 50 {
        return Err(HarnessError::usage("todo list exceeds 50 items"));
    }
    Ok(items)
}

struct TodoWriteTool {
    manager: Arc<TodoManager>,
}

impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo.write"
    }
    fn description(&self) -> &str {
        "Replace the structured task list (full list each call). Use it to plan multi-step work and track progress."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["content"],
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string", "enum": [
                                "pending", "in_progress", "completed", "cancelled"
                            ]}
                        },
                        "additionalProperties": false
                    }
                }
            },
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::Todo
    }
    fn read_only(&self) -> bool {
        false
    }
    fn activity(&self) -> &str {
        "Updating todo list"
    }
    fn source(&self) -> ToolSource {
        ToolSource::Builtin
    }
    fn execute(&self, args: &serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutcome> {
        let items = parse_items(args)?;
        self.manager.set(items.clone());
        let done = items
            .iter()
            .filter(|i| i.status == TodoStatus::Completed)
            .count();
        Ok(ToolOutcome::ok(format!(
            "{} todos ({} completed)",
            items.len(),
            done
        ))
        .with_data(serde_json::json!({"count": items.len(), "completed": done})))
    }
}

struct TodoReadTool {
    manager: Arc<TodoManager>,
}

impl Tool for TodoReadTool {
    fn name(&self) -> &str {
        "todo.read"
    }
    fn description(&self) -> &str {
        "Read the current structured task list."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    fn needed(&self) -> Permission {
        Permission::Todo
    }
    fn read_only(&self) -> bool {
        true
    }
    fn activity(&self) -> &str {
        "Reading todo list"
    }
    fn source(&self) -> ToolSource {
        ToolSource::Builtin
    }
    fn execute(&self, _args: &serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutcome> {
        let items = self.manager.items();
        let data = serde_json::json!({
            "todos": items
                .iter()
                .map(|i| serde_json::json!({
                    "id": i.id,
                    "content": i.content,
                    "status": i.status.as_str(),
                }))
                .collect::<Vec<_>>(),
        });
        Ok(ToolOutcome::ok(format!("{} todos", items.len())).with_data(data))
    }
}

pub fn builtin_todo_tools() -> Vec<Arc<dyn Tool>> {
    // Both tools share one manager handle; the runner passes the same
    // Arc into ToolContext so the state is loop-scoped.
    let manager = Arc::new(TodoManager::new());
    vec![
        Arc::new(TodoWriteTool {
            manager: manager.clone(),
        }),
        Arc::new(TodoReadTool { manager }),
    ]
}

/// Fresh manager + tools pair when the caller wants to own the state handle.
/// Used by future embedding call sites; the stage runners currently build
/// their own `TodoManager` and pass it via `ToolContext::todos`.
#[cfg(test)]
#[allow(dead_code)]
pub fn todo_tools_with_handle() -> (Arc<TodoManager>, Vec<Arc<dyn Tool>>) {
    let manager = Arc::new(TodoManager::new());
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(TodoWriteTool {
            manager: manager.clone(),
        }),
        Arc::new(TodoReadTool { manager: manager.clone() }),
    ];
    (manager, tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::ToolRegistry;
    use crate::policy::{Permission, PermissionSet};

    fn ctx() -> ToolContext {
        let full = PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate)
            .allow(Permission::Todo);
        ToolContext {
            host: full.clone(),
            stage: full.clone(),
            role: full,
            attempt_dir: std::env::temp_dir(),
            template_roots: vec![],
            genre: String::new(),
            design_dir: None,
            pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
            todos: None,
        }
    }

    #[test]
    fn write_then_read_roundtrip() {
        let c = ctx();
        let mut reg = ToolRegistry::new();
        for t in builtin_todo_tools() {
            reg.register_tool(t);
        }
        let out = reg
            .dispatch(
                "todo.write",
                &serde_json::json!({"todos": [
                    {"content": "write palette", "status": "in_progress"},
                    {"content": "build tank"}
                ]}),
                &c,
            )
            .unwrap();
        assert!(out.ok);
        let read = reg.dispatch("todo.read", &serde_json::json!({}), &c).unwrap();
        assert_eq!(read.data["todos"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_bad_status_and_requires_handle_sharing() {
        let (manager, tools) = todo_tools_with_handle();
        let c = ctx();
        let mut reg = ToolRegistry::new();
        for t in tools {
            reg.register_tool(t);
        }
        assert!(reg
            .dispatch(
                "todo.write",
                &serde_json::json!({"todos": [{"content": "x", "status": "bogus"}]}),
                &c
            )
            .is_err());
        let ok = reg
            .dispatch(
                "todo.write",
                &serde_json::json!({"todos": [{"content": "only item"}]}),
                &c,
            )
            .unwrap();
        assert!(ok.ok);
        let rendered = manager.render();
        assert!(rendered.contains("[ ] only item"), "{rendered}");
        assert!(rendered.starts_with("## Todo"));
    }

    #[test]
    fn render_empty_is_blank() {
        assert_eq!(TodoManager::new().render(), "");
    }
}
