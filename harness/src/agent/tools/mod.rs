//! Tool registry and typed tool results. Tools declare the permission they
//! need; the runner checks host ∩ stage ∩ role before executing.
//!
//! v3: `ToolDef` additionally carries `read_only`, `activity` and `source`
//! metadata (structural port of miniagent's `Tool` trait), and the schema
//! validator understands `additionalProperties: false`.

mod fs_tools;
mod todo;

pub use fs_tools::builtin_fs_tools;
pub use todo::{builtin_todo_tools, TodoItem, TodoManager};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};
use crate::policy::{check_tool_allowed, Permission, PermissionSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutcome {
    pub ok: bool,
    /// Compact result for the model transcript.
    pub summary: String,
    /// Full payload stored next to the attempt, referenced by digest.
    #[serde(default)]
    pub artifact: Option<ArtifactRef>,
    #[serde(default)]
    pub data: serde_json::Value,
    /// pxl error code (e.g. `E_QA`) for normal-return failures, so events can
    /// audit what the tool actually reported instead of only HarnessErrors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Underlying process exit code (e.g. pxl exit 5 for E_QA).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub path: String,
    pub digest: String,
    pub bytes: u64,
}

impl ToolOutcome {
    pub fn ok(summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            artifact: None,
            data: serde_json::Value::Null,
            error_code: None,
            exit_code: None,
        }
    }
    pub fn fail(summary: impl Into<String>) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
            artifact: None,
            data: serde_json::Value::Null,
            error_code: None,
            exit_code: None,
        }
    }
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
    /// Attach a pxl outcome's error code and exit code so the event log and
    /// diagnostics can audit normal-return failures, not just HarnessErrors.
    pub fn with_pxl_result(mut self, result: &crate::adapters::pxl::PxlResult_) -> Self {
        if !result.ok {
            self.error_code = Some(result.error_code.clone());
            self.exit_code = result.exit_code;
        }
        self
    }
    pub fn with_artifact(mut self, path: impl Into<String>, digest: impl Into<String>, bytes: u64) -> Self {
        self.artifact = Some(ArtifactRef {
            path: path.into(),
            digest: digest.into(),
            bytes,
        });
        self
    }
}

pub type ToolFn =
    Box<dyn Fn(&serde_json::Value, &ToolContext) -> Result<ToolOutcome> + Send + Sync>;

/// Where a tool came from; used for audits and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    /// Generic built-in (fs/todo) registered for every agent context.
    Builtin,
    /// Stage-specific tool owned by a pipeline stage.
    Stage,
    /// Bridged from an MCP server (`mcp_<server>_<tool>`).
    Mcp,
    /// Goal inspection/creation tools.
    Goal,
}

pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub needed: Permission,
    pub run: ToolFn,
    /// Read-only tools never mutate attempt state and may run concurrently.
    #[allow(dead_code)]
    pub read_only: bool,
    /// Short human-readable activity label for the event log.
    #[allow(dead_code)]
    pub activity: String,
    /// Provenance tag for audits.
    #[allow(dead_code)]
    pub source: ToolSource,
}

impl ToolDef {
    /// v2-compatible constructor: a stage-style mutating tool without
    /// metadata. Existing call sites keep working unchanged.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
        needed: Permission,
        run: ToolFn,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            needed,
            run,
            read_only: false,
            activity: String::new(),
            source: ToolSource::Stage,
        }
    }
}

/// The v3 tool trait (structural port of miniagent `core::tools::Tool`).
/// Implement this for object-style tools; `ToolRegistry::register_tool`
/// adapts them into a `ToolDef` with the same permission pipeline.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema of the arguments object.
    fn parameters(&self) -> serde_json::Value;
    /// Permission this tool needs (host ∩ stage ∩ role still applies).
    fn needed(&self) -> Permission;
    /// Read-only tools may run in parallel within one turn.
    fn read_only(&self) -> bool;
    /// Short activity label for the event log.
    fn activity(&self) -> &str;
    fn source(&self) -> ToolSource {
        ToolSource::Builtin
    }
    fn execute(&self, args: &serde_json::Value, ctx: &ToolContext) -> Result<ToolOutcome>;
}

pub struct ToolRegistry {
    tools: BTreeMap<String, ToolDef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, def: ToolDef) {
        self.tools.insert(def.name.clone(), def);
    }

    /// Adapt a `Tool` trait object into a registry entry.
    pub fn register_tool(&mut self, tool: std::sync::Arc<dyn Tool>) {
        let spec = crate::agent::provider::ToolSpec {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters(),
        };
        let read_only = tool.read_only();
        let activity = tool.activity().to_string();
        let source = tool.source();
        let needed = tool.needed();
        let def = ToolDef {
            name: spec.name,
            description: spec.description,
            parameters: spec.parameters,
            needed,
            run: Box::new(move |args, ctx| tool.execute(args, ctx)),
            read_only,
            activity,
            source,
        };
        self.register(def);
    }

    /// Register the generic built-in tools (fs + todo). The permission sets
    /// handed in decide what actually becomes visible.
    pub fn with_builtin(mut self) -> Self {
        for tool in builtin_fs_tools() {
            self.register_tool(tool);
        }
        for tool in builtin_todo_tools() {
            self.register_tool(tool);
        }
        self
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name)
    }

    pub fn specs(&self, allowed: &PermissionSet) -> Vec<crate::agent::provider::ToolSpec> {
        self.tools
            .values()
            .filter(|t| allowed.has(&t.needed))
            .map(|t| crate::agent::provider::ToolSpec {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Validate arguments against the declared JSON Schema subset, check
    /// permission intersection, then execute.
    pub fn dispatch(
        &self,
        name: &str,
        arguments: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutcome> {
        let def = self
            .tools
            .get(name)
            .ok_or_else(|| HarnessError::usage(format!("unknown tool '{name}'")))?;
        check_tool_allowed(&ctx.host, &ctx.stage, &ctx.role, &def.needed)?;
        validate_arguments(&def.parameters, arguments)?;
        (def.run)(arguments, ctx)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Environment handed to every tool execution.
pub struct ToolContext {
    pub host: PermissionSet,
    pub stage: PermissionSet,
    pub role: PermissionSet,
    /// Attempt-scoped write root.
    pub attempt_dir: std::path::PathBuf,
    /// Approved template roots (read-only).
    pub template_roots: Vec<std::path::PathBuf>,
    /// Genre template name resolved from project identity (read-only for
    /// the model: it cannot pick arbitrary directories to read).
    pub genre: String,
    /// Frozen design revision directory, when the stage consumes one.
    pub design_dir: Option<std::path::PathBuf>,
    pub pxl: std::sync::Arc<crate::adapters::PxlAdapter>,
    /// v3: shared todo state for the `todo.write` built-in tool. `None`
    /// keeps older ToolContext builders compile-compatible.
    pub todos: Option<std::sync::Arc<TodoManager>>,
}

impl ToolContext {
    /// v3: render the todo block that gets injected into the next turn's
    /// user-visible context. Empty when no todos exist.
    pub fn todo_block(&self) -> String {
        self.todos
            .as_ref()
            .map(|t| t.render())
            .unwrap_or_default()
    }
}

/// Minimal JSON Schema validation: type checks, required, properties,
/// enums, items, min/max on numbers, and `additionalProperties: false`.
pub fn validate_arguments(schema: &serde_json::Value, args: &serde_json::Value) -> Result<()> {
    validate_node(schema, args, "$")
}

fn validate_node(schema: &serde_json::Value, v: &serde_json::Value, path: &str) -> Result<()> {
    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        let ok = match (ty, v) {
            ("object", serde_json::Value::Object(_)) => true,
            ("array", serde_json::Value::Array(_)) => true,
            ("string", serde_json::Value::String(_)) => true,
            ("number", serde_json::Value::Number(n)) => n.is_f64() || n.is_u64() || n.is_i64(),
            ("integer", serde_json::Value::Number(n)) => n.is_u64() || n.is_i64(),
            ("boolean", serde_json::Value::Bool(_)) => true,
            ("null", serde_json::Value::Null) => true,
            _ => false,
        };
        if !ok {
            return Err(HarnessError::usage(format!(
                "{path}: expected type {ty}"
            )));
        }
    }
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        if !enum_vals.contains(v) {
            return Err(HarnessError::usage(format!(
                "{path}: value {v} not in enum"
            )));
        }
    }
    if let (Some(props), serde_json::Value::Object(map)) =
        (schema.get("properties").and_then(|p| p.as_object()), v)
    {
        // v3: honor additionalProperties: false — unknown fields are a
        // usage error, mirroring miniagent's schema contracts. The check is
        // enabled when the key exists and is exactly `false` (JSON Schema
        // semantics: absent / true / a schema value means "allow").
        let rejects_extra = matches!(
            schema.get("additionalProperties"),
            Some(serde_json::Value::Bool(false))
        );
        if rejects_extra {
            for key in map.keys() {
                if !props.contains_key(key) {
                    return Err(HarnessError::usage(format!(
                        "{path}: unexpected property '{key}' (schema declares additionalProperties: false)"
                    )));
                }
            }
        }
        for (key, sub) in props {
            if let Some(value) = map.get(key) {
                validate_node(sub, value, &format!("{path}.{key}"))?;
            }
        }
    }
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        if let serde_json::Value::Object(map) = v {
            for r in required {
                let name = r.as_str().unwrap_or_default();
                if !map.contains_key(name) {
                    return Err(HarnessError::usage(format!(
                        "{path}: missing required '{name}'"
                    )));
                }
            }
        }
    }
    if let (Some(items), serde_json::Value::Array(list)) =
        (schema.get("items"), v)
    {
        for (i, item) in list.iter().enumerate() {
            validate_node(items, item, &format!("{path}[{i}]"))?;
        }
    }
    if let (serde_json::Value::Number(n), Some(min)) =
        (v, schema.get("minimum").and_then(|m| m.as_f64()))
    {
        if n.as_f64().unwrap_or(0.0) < min {
            return Err(HarnessError::usage(format!("{path}: below minimum {min}")));
        }
    }
    if let (serde_json::Value::Number(n), Some(max)) =
        (v, schema.get("maximum").and_then(|m| m.as_f64()))
    {
        if n.as_f64().unwrap_or(0.0) > max {
            return Err(HarnessError::usage(format!("{path}: above maximum {max}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> PermissionSet {
        PermissionSet::default()
            .allow(Permission::ReadProject)
            .allow(Permission::WriteCandidate)
            .allow(Permission::Pxl)
            .allow(Permission::Todo)
    }

    #[test]
    fn schema_validation_catches_bad_arguments() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string"},
                "times": {"type": "integer", "minimum": 1, "maximum": 3},
                "kind": {"enum": ["sprite", "tile"]},
            }
        });
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": "a", "times": 2, "kind": "tile"}),
            "$"
        )
        .is_ok());
        assert!(validate_node(
            &schema,
            &serde_json::json!({"times": 2}),
            "$"
        )
        .is_err());
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": 1}),
            "$"
        )
        .is_err());
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": "a", "times": 9}),
            "$"
        )
        .is_err());
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": "a", "kind": "nope"}),
            "$"
        )
        .is_err());
    }

    #[test]
    fn additional_properties_rejected() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {"path": {"type": "string"}},
            "additionalProperties": false
        });
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": "a"}),
            "$"
        )
        .is_ok());
        assert!(validate_node(
            &schema,
            &serde_json::json!({"path": "a", "extra": 1}),
            "$"
        )
        .is_err());
        // Without the flag, unknown fields pass (v2 behavior).
        let open = serde_json::json!({
            "type": "object",
            "properties": {"path": {"type": "string"}}
        });
        assert!(validate_node(
            &open,
            &serde_json::json!({"path": "a", "extra": 1}),
            "$"
        )
        .is_ok());
    }

    #[test]
    fn permission_intersection_blocks_execution() {
        let mut reg = ToolRegistry::new();
        reg.register(ToolDef {
            name: "pxl.build".into(),
            description: "build".into(),
            parameters: serde_json::json!({"type": "object"}),
            needed: Permission::Pxl,
            run: Box::new(|_, _| Ok(ToolOutcome::ok("built"))),
            read_only: false,
            activity: "building".into(),
            source: ToolSource::Stage,
        });
        let ctx = ToolContext {
            host: full(),
            stage: full(),
            role: PermissionSet::default().allow(Permission::ReadProject),
            attempt_dir: std::path::PathBuf::from("/tmp"),
            template_roots: vec![],
            genre: String::new(),
            design_dir: None,
            pxl: std::sync::Arc::new(dummy_adapter()),
            todos: None,
        };
        assert!(reg.dispatch("pxl.build", &serde_json::json!({}), &ctx).is_err());
        let allowed = ToolContext {
            role: full(),
            ..ctx
        };
        // ToolContext isn't Clone; rebuild a permissive one.
        drop(allowed);
    }

    fn dummy_adapter() -> crate::adapters::PxlAdapter {
        crate::adapters::PxlAdapter::discover(&crate::config::Config::default())
            .unwrap_or_else(|_| crate::adapters::PxlAdapter::unavailable())
    }
}
