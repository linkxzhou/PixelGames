//! Tool registry and typed tool results. Tools declare the permission they
//! need; the runner checks host ∩ stage ∩ role before executing.

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
        }
    }
    pub fn fail(summary: impl Into<String>) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
            artifact: None,
            data: serde_json::Value::Null,
        }
    }
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }
}

pub type ToolFn =
    Box<dyn Fn(&serde_json::Value, &ToolContext) -> Result<ToolOutcome> + Send + Sync>;

pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub needed: Permission,
    pub run: ToolFn,
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

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
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
    /// Frozen design revision directory, when the stage consumes one.
    pub design_dir: Option<std::path::PathBuf>,
    pub pxl: std::sync::Arc<crate::adapters::PxlAdapter>,
}

/// Minimal JSON Schema validation: type checks, required, properties,
/// enums, items, min/max on numbers.
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
    fn permission_intersection_blocks_execution() {
        let mut reg = ToolRegistry::new();
        reg.register(ToolDef {
            name: "pxl.build".into(),
            description: "build".into(),
            parameters: serde_json::json!({"type": "object"}),
            needed: Permission::Pxl,
            run: Box::new(|_, _| Ok(ToolOutcome::ok("built"))),
        });
        let ctx = ToolContext {
            host: full(),
            stage: full(),
            role: PermissionSet::default().allow(Permission::ReadProject),
            attempt_dir: std::path::PathBuf::from("/tmp"),
            template_roots: vec![],
            design_dir: None,
            pxl: std::sync::Arc::new(dummy_adapter()),
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
