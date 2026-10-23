//! MCP registry: one child process per configured server, bridged into the
//! harness tool registry. Tool names are prefixed `mcp_<server>_<tool>`;
//! the `Mcp` permission must be granted by host policy before any bridge
//! tool is offered or executed.

use std::sync::{Arc, Mutex};

use crate::agent::mcp::client::McpClient;
use crate::agent::mcp::config::McpServerConfig;
use crate::agent::tools::{Tool, ToolContext, ToolOutcome, ToolSource};
use crate::error::{HarnessError, Result};
use crate::policy::Permission;

/// Metadata for one remote tool.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub server: String,
    pub tool: String,
    pub description: String,
    pub schema: serde_json::Value,
    /// Bridged name: `mcp_<server>_<tool>`.
    pub bridged_name: String,
}

impl McpToolInfo {
    pub fn bridged_name(server: &str, tool: &str) -> String {
        format!("mcp_{server}_{tool}")
    }
}

/// One running server and its discovered tools.
struct ServerHandle {
    client: McpClient,
    tools: Vec<McpToolInfo>,
    call_timeout_ms: u64,
}

pub struct McpRegistry {
    servers: Mutex<Vec<ServerHandle>>,
}

impl McpRegistry {
    /// Disabled registry (no servers configured).
    pub fn disabled() -> Self {
        Self {
            servers: Mutex::new(Vec::new()),
        }
    }

    /// Spawn every configured server, handshake, list tools. Server startup
    /// failures are surfaced immediately — a half-configured MCP setup must
    /// fail loudly, not silently drop tools.
    pub fn start(configs: &[McpServerConfig]) -> Result<Self> {
        let mut servers = Vec::new();
        for cfg in configs {
            let McpServerConfig::Stdio {
                name,
                command,
                args,
                env,
                startup_timeout_ms,
            } = cfg
            else {
                continue; // http already rejected at config load
            };
            let mut client =
                crate::agent::mcp::client::spawn_server(name, command, args, env, *startup_timeout_ms)?;
            let remote = client.list_tools(*startup_timeout_ms)?;
            let tools: Vec<McpToolInfo> = remote
                .into_iter()
                .map(|(tool, description, schema)| McpToolInfo {
                    server: name.clone(),
                    tool: tool.clone(),
                    description,
                    schema,
                    bridged_name: McpToolInfo::bridged_name(name, &tool),
                })
                .collect();
            servers.push(ServerHandle {
                client,
                tools,
                call_timeout_ms: 60_000,
            });
        }
        Ok(Self {
            servers: Mutex::new(servers),
        })
    }

    /// All discovered tool infos.
    pub fn tool_infos(&self) -> Vec<McpToolInfo> {
        self.servers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    /// Build `Tool` adapters for every discovered tool. Callers register
    /// them via `ToolRegistry::register_tool`; the `Mcp` permission gates
    /// visibility and execution.
    pub fn bridge_tools(self: &Arc<Self>) -> Vec<Arc<dyn Tool>> {
        let infos = self.tool_infos();
        infos
            .into_iter()
            .map(|info| {
                Arc::new(McpToolBridge {
                    registry: Arc::clone(self),
                    info,
                }) as Arc<dyn Tool>
            })
            .collect()
    }

    pub fn shutdown_all(&self) {
        let mut servers = self.servers.lock().unwrap_or_else(|e| e.into_inner());
        for server in servers.iter_mut() {
            server.client.shutdown();
        }
        servers.clear();
    }
}

impl Drop for McpRegistry {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

/// Adapter: one harness `Tool` per remote MCP tool.
struct McpToolBridge {
    registry: Arc<McpRegistry>,
    info: McpToolInfo,
}

impl Tool for McpToolBridge {
    fn name(&self) -> &str {
        &self.info.bridged_name
    }
    fn description(&self) -> &str {
        &self.info.description
    }
    fn parameters(&self) -> serde_json::Value {
        self.info.schema.clone()
    }
    fn needed(&self) -> Permission {
        Permission::Mcp
    }
    fn read_only(&self) -> bool {
        // Remote tools cannot be proven read-only; never parallelize them
        // and never map them onto read permissions.
        false
    }
    fn activity(&self) -> &str {
        "Calling MCP tool"
    }
    fn source(&self) -> ToolSource {
        ToolSource::Mcp
    }
    fn execute(&self, args: &serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutcome> {
        let mut servers = self
            .registry
            .servers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let server = servers
            .iter_mut()
            .find(|s| s.client.name() == self.info.server)
            .ok_or_else(|| {
                HarnessError::tool(format!(
                    "mcp server '{}' is no longer running",
                    self.info.server
                ))
            })?;
        // Clip remote output before it enters the transcript.
        const MAX_RESULT_CHARS: usize = 20_000;
        let result = server
            .client
            .call_tool(&self.info.tool, args, server.call_timeout_ms)?;
        let mut text = result["content"].as_str().unwrap_or_default().to_string();
        if text.len() > MAX_RESULT_CHARS {
            text.truncate(MAX_RESULT_CHARS);
            text.push_str("… [clipped]");
        }
        Ok(ToolOutcome::ok(format!(
            "mcp {} → {}",
            self.info.bridged_name,
            first_line(&text)
        ))
        .with_data(serde_json::json!({"content": text})))
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridged_names_are_prefixed() {
        assert_eq!(
            McpToolInfo::bridged_name("fs", "read_file"),
            "mcp_fs_read_file"
        );
    }

    #[test]
    fn disabled_registry_has_no_tools() {
        let reg = McpRegistry::disabled();
        assert!(reg.tool_infos().is_empty());
    }

    /// Bridge tools against a scripted python server and dispatch through a
    /// real ToolRegistry with the Mcp permission granted.
    #[test]
    fn bridge_tool_executes_through_registry() {
        use crate::agent::tools::ToolRegistry;
        use crate::policy::{Permission, PermissionSet};
        let python_ok = std::process::Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !python_ok {
            return;
        }
        let script = r#"
import sys, json
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    req = json.loads(line)
    if req.get("id") is None: continue
    if req["method"] == "initialize":
        send({"jsonrpc":"2.0","id":req["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"t","version":"1"}}})
    elif req["method"] == "tools/list":
        send({"jsonrpc":"2.0","id":req["id"],"result":{"tools":[{"name":"add","description":"add numbers","inputSchema":{"type":"object","required":["a","b"],"properties":{"a":{"type":"number"},"b":{"type":"number"}},"additionalProperties":False}}]}})
    elif req["method"] == "tools/call":
        a = req["params"]["arguments"]["a"]; b = req["params"]["arguments"]["b"]
        send({"jsonrpc":"2.0","id":req["id"],"result":{"content":[{"type":"text","text":str(a+b)}]}})
"#;
        let cfg = McpServerConfig::Stdio {
            name: "calc".into(),
            command: "python3".into(),
            args: vec!["-c".into(), script.into()],
            env: Default::default(),
            startup_timeout_ms: 15_000,
        };
        let registry = Arc::new(McpRegistry::start(&[cfg]).expect("start failed"));
        let infos = registry.tool_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].bridged_name, "mcp_calc_add");

        let mut tools = ToolRegistry::new();
        for tool in registry.bridge_tools() {
            tools.register_tool(tool);
        }
        let full = PermissionSet::default()
            .allow(Permission::Mcp)
            .allow(Permission::ReadProject);
        let ctx = ToolContext {
            host: full.clone(),
            stage: full.clone(),
            role: full,
            attempt_dir: std::env::temp_dir(),
            template_roots: vec![],
            genre: String::new(),
            design_dir: None,
            pxl: std::sync::Arc::new(crate::adapters::PxlAdapter::unavailable()),
            todos: None,
        };
        let specs = tools.specs(&ctx.role);
        assert_eq!(specs.len(), 1, "{specs:?}");
        let out = tools
            .dispatch("mcp_calc_add", &serde_json::json!({"a": 2, "b": 40}), &ctx)
            .unwrap();
        assert!(out.ok, "{}", out.summary);
        assert_eq!(out.data["content"].as_str().unwrap(), "42");
        // additionalProperties on the remote schema is enforced locally.
        assert!(tools
            .dispatch(
                "mcp_calc_add",
                &serde_json::json!({"a": 2, "b": 40, "evil": true}),
                &ctx
            )
            .is_err());
        registry.shutdown_all();
    }
}
