//! MCP client subsystem (structural port of miniagent `mcp/`, batch-mode
//! adaptation): JSON-RPC 2.0 over stdio with std-only process I/O, a
//! registry that keeps one child per configured server, and a tool bridge
//! that adapts remote tools into the harness permission pipeline.
//!
//! Security invariants:
//! - servers are configured only in `harness.config.json` (host policy);
//!   the model can never spawn a process
//! - tool names are prefixed `mcp_<server>_<tool>` so audits can attribute
//!   every call
//! - remote tool results are clipped before entering the transcript

pub mod client;
pub mod config;
pub mod registry;

pub use client::{spawn_server, McpClient};
pub use config::{McpServerConfig, MCP_CONFIG_FILE};
pub use registry::{McpRegistry, McpToolInfo};
