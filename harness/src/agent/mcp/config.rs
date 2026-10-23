//! MCP server configuration (`mcp_servers.json` next to the harness config).
//! Host-owned: the model cannot add or edit servers.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};

pub const MCP_CONFIG_FILE: &str = "mcp_servers.json";

/// Transport: stdio is the only supported transport (v3). `http` is parsed
/// but rejected at load time with a clear error, so configs fail fast
/// instead of silently ignoring a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "transport")]
pub enum McpServerConfig {
    Stdio {
        /// Server name used in tool prefixes (`mcp_<name>_<tool>`).
        name: String,
        /// Executable to launch. Resolved via PATH; must be absolute or a
        /// bare command name (no shell).
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables for the child. Values may reference host
        /// env vars with `${VAR}` — expanded at spawn, never persisted.
        #[serde(default)]
        env: std::collections::BTreeMap<String, String>,
        /// Startup timeout in milliseconds (initialize handshake).
        #[serde(default = "default_startup_timeout_ms")]
        startup_timeout_ms: u64,
    },
    Http {
        name: String,
        url: String,
    },
}

fn default_startup_timeout_ms() -> u64 {
    10_000
}

impl McpServerConfig {
    pub fn name(&self) -> &str {
        match self {
            McpServerConfig::Stdio { name, .. } => name,
            McpServerConfig::Http { name, .. } => name,
        }
    }

    /// Validate naming rules: server names become part of tool identifiers.
    pub fn validate(&self) -> Result<()> {
        let name = self.name();
        if name.is_empty() {
            return Err(HarnessError::schema("mcp server name must not be empty"));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(HarnessError::schema(format!(
                "mcp server name '{name}' must be alphanumeric/-/_ (it becomes part of tool names)"
            )));
        }
        if let McpServerConfig::Http { .. } = self {
            return Err(HarnessError::blocked(
                format!("mcp server '{name}' uses the http transport, which is not supported yet"),
            )
            .with_hint("v3 supports stdio servers only"));
        }
        Ok(())
    }
}

/// Load `mcp_servers.json` from the project directory. Missing file is an
/// empty list (MCP is opt-in per project).
pub fn load_mcp_config(project: &Path) -> Result<Vec<McpServerConfig>> {
    let path = project.join(MCP_CONFIG_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let servers: Vec<McpServerConfig> = serde_json::from_str(&text)
        .map_err(|e| {
            HarnessError::schema(format!("{MCP_CONFIG_FILE}: {e}"))
                .with_hint("expected a JSON array of server configs")
        })?;
    let mut seen = std::collections::BTreeSet::new();
    for server in &servers {
        server.validate()?;
        if !seen.insert(server.name().to_string()) {
            return Err(HarnessError::schema(format!(
                "duplicate mcp server name '{}'",
                server.name()
            )));
        }
    }
    Ok(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl_h_mcpcfg_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_means_disabled() {
        let dir = tmp("missing");
        assert!(load_mcp_config(&dir).unwrap().is_empty());
    }

    #[test]
    fn parses_and_validates_stdio_servers() {
        let dir = tmp("ok");
        std::fs::write(
            dir.join(MCP_CONFIG_FILE),
            r#"[{"transport":"stdio","name":"fs","command":"cat","args":["--help"]}]"#,
        )
        .unwrap();
        let servers = load_mcp_config(&dir).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name(), "fs");
        assert_eq!(servers[0].startup_timeout_ms_default(), 10_000);
    }

    #[test]
    fn rejects_http_duplicates_and_bad_names() {
        let dir = tmp("bad");
        std::fs::write(
            dir.join(MCP_CONFIG_FILE),
            r#"[{"transport":"http","name":"web","url":"http://x"}]"#,
        )
        .unwrap();
        assert!(load_mcp_config(&dir).is_err());
        std::fs::write(
            dir.join(MCP_CONFIG_FILE),
            r#"[
                {"transport":"stdio","name":"a b","command":"cat"},
                {"transport":"stdio","name":"a b","command":"cat"}
            ]"#,
        )
        .unwrap();
        let err = load_mcp_config(&dir).unwrap_err();
        assert!(err.message.contains("alphanumeric"), "{err}");
    }
}

impl McpServerConfig {
    #[cfg(test)]
    fn startup_timeout_ms_default(&self) -> u64 {
        match self {
            McpServerConfig::Stdio {
                startup_timeout_ms, ..
            } => *startup_timeout_ms,
            McpServerConfig::Http { .. } => 0,
        }
    }
}
