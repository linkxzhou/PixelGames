//! MCP stdio client: spawns the server process, performs the `initialize`
//! handshake, lists tools and calls them. JSON-RPC 2.0 framing is
//! newline-delimited JSON on stdin/stdout (LSP-style Content-Length
//! framing is NOT used — MCP stdio uses newline-delimited JSON-RPC).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::error::{HarnessError, Result};

pub struct McpClient {
    name: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

/// Maximum accepted size of one JSON-RPC response line.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// Expand `${VAR}` references against the host environment at spawn time.
fn expand_env(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let var = &after[..end];
                out.push_str(&std::env::var(var).unwrap_or_default());
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Spawn a stdio MCP server and perform the initialize handshake.
pub fn spawn_server(
    name: &str,
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    startup_timeout_ms: u64,
) -> Result<McpClient> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, expand_env(v));
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| HarnessError::tool(format!("mcp server '{name}' failed to start: {e}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| HarnessError::tool("mcp stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HarnessError::tool("mcp stdout unavailable"))?;
    let mut client = McpClient {
        name: name.to_string(),
        child,
        stdin,
        stdout: BufReader::new(stdout),
        next_id: 1,
    };
    // initialize → initialized notification. Protocol version: "2025-06-18"
    // is requested; servers may respond with an older supported version.
    let _init = client.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "pxl-harness", "version": crate::VERSION},
        }),
        startup_timeout_ms,
    )?;
    client.notify("notifications/initialized", serde_json::json!({}))?;
    Ok(client)
}

impl McpClient {
    pub fn name(&self) -> &str {
        &self.name
    }

    fn write_line(&mut self, payload: &serde_json::Value) -> Result<()> {
        let line = serde_json::to_string(payload)?;
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| HarnessError::tool(format!("mcp write failed: {e}")))
    }

    /// Read one line (bounded) and parse it as JSON.
    fn read_line_json(&mut self, timeout_ms: u64) -> Result<serde_json::Value> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut line = Vec::new();
        loop {
            // Non-blocking-ish read: check bytes available via a buffered
            // read with poll semantics. std does not expose poll; use small
            // reads with deadline checks between retries.
            match self.stdout.fill_buf() {
                Ok(buf) if buf.is_empty() => {
                    // EOF
                    return Err(HarnessError::tool(format!(
                        "mcp server '{}' closed its stdout",
                        self.name
                    )));
                }
                Ok(buf) => {
                    if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                        line.extend_from_slice(&buf[..=nl]);
                        let len = line.len();
                        self.stdout.consume(len);
                        let text = String::from_utf8_lossy(&line).trim().to_string();
                        if text.is_empty() {
                            line.clear();
                            continue;
                        }
                        return serde_json::from_str(&text).map_err(|e| {
                            HarnessError::tool(format!("mcp response is not JSON: {e}"))
                        });
                    }
                    // No newline yet: take everything buffered.
                    let len = buf.len();
                    line.extend_from_slice(buf);
                    if line.len() > MAX_LINE_BYTES {
                        return Err(HarnessError::tool("mcp response line exceeds 4MiB"));
                    }
                    self.stdout.consume(len);
                }
                Err(e) => {
                    return Err(HarnessError::io(format!("mcp read failed: {e}")));
                }
            }
            if std::time::Instant::now() > deadline {
                return Err(HarnessError::tool(format!(
                    "mcp server '{}' timed out after {timeout_ms}ms",
                    self.name
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_line(&payload)?;
        // Skip notifications until the matching response id arrives.
        loop {
            let response = self.read_line_json(timeout_ms)?;
            if response.get("id") != Some(&serde_json::json!(id)) {
                continue;
            }
            if let Some(err) = response.get("error") {
                return Err(HarnessError::tool(format!(
                    "mcp '{method}' failed: {}",
                    err.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                )));
            }
            return Ok(response.get("result").cloned().unwrap_or(serde_json::Value::Null));
        }
    }

    fn notify(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_line(&payload)
    }

    /// `tools/list` → (name, description, input_schema) triples.
    pub fn list_tools(&mut self, timeout_ms: u64) -> Result<Vec<(String, String, serde_json::Value)>> {
        let result = self.request("tools/list", serde_json::json!({}), timeout_ms)?;
        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for tool in tools {
            let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let description = tool
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or_default();
            let schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or(serde_json::json!({"type": "object"}));
            out.push((name.to_string(), description.to_string(), schema));
        }
        Ok(out)
    }

    /// `tools/call` → content items rendered into a compact string.
    pub fn call_tool(
        &mut self,
        tool: &str,
        arguments: &serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value> {
        let result = self.request(
            "tools/call",
            serde_json::json!({"name": tool, "arguments": arguments}),
            timeout_ms,
        )?;
        let is_error = result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false);
        let content = result
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        let mut parts: Vec<String> = Vec::new();
        for item in &content {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("text") => parts.push(
                    item.get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default()
                        .to_string(),
                ),
                Some("image") => parts.push("[image content omitted]".into()),
                Some("resource") => {
                    let uri = item
                        .pointer("/resource/uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("unknown");
                    parts.push(format!("[resource {uri}]"));
                }
                _ => parts.push("[unsupported content item]".into()),
            }
        }
        let text = parts.join("\n");
        if is_error {
            Err(HarnessError::tool(format!(
                "mcp tool '{tool}' reported an error: {text}"
            )))
        } else {
            Ok(serde_json::json!({"content": text}))
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.notify("shutdown", serde_json::json!({}));
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_expansion() {
        std::env::set_var("PXL_MCP_TEST_VAR", "hello");
        assert_eq!(expand_env("x_${PXL_MCP_TEST_VAR}_y"), "x_hello_y");
        assert_eq!(expand_env("${PXL_MCP_TEST_VAR}"), "hello");
        assert_eq!(expand_env("no refs"), "no refs");
        assert_eq!(expand_env("${MISSING_VAR_XYZ}"), "");
        assert_eq!(expand_env("literal ${unclosed"), "literal ${unclosed");
    }

    /// Scripted stdio server built with `python3 -c` that answers
    /// initialize and tools/list. Skipped when python3 is unavailable.
    fn python_available() -> bool {
        Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn handshake_and_list_tools_against_scripted_server() {
        if !python_available() {
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
        send({"jsonrpc":"2.0","id":req["id"],"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1"}}})
    elif req["method"] == "tools/list":
        send({"jsonrpc":"2.0","id":req["id"],"result":{"tools":[{"name":"echo","description":"echo text","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":False}}]}})
    elif req["method"] == "tools/call":
        send({"jsonrpc":"2.0","id":req["id"],"result":{"content":[{"type":"text","text":"echo:"+req["params"]["arguments"]["text"]}]}})
    else:
        send({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32601,"message":"unknown"}})
"#;
        let mut client = spawn_server(
            "test",
            "python3",
            &["-c".to_string(), script.to_string()],
            &BTreeMap::new(),
            15_000,
        )
        .expect("spawn failed");
        let tools = client.list_tools(15_000).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].0, "echo");
        let out = client
            .call_tool("echo", &serde_json::json!({"text": "hi"}), 15_000)
            .unwrap();
        assert_eq!(out["content"].as_str().unwrap(), "echo:hi");
        client.shutdown();
    }

    #[test]
    fn unknown_tool_reports_error() {
        if !python_available() {
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
    send({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32601,"message":"no such method"}})
"#;
        // The scripted server answers every request with a JSON-RPC error,
        // so the initialize handshake itself must fail loudly.
        let spawned = spawn_server(
            "err",
            "python3",
            &["-c".to_string(), script.to_string()],
            &BTreeMap::new(),
            15_000,
        );
        let err = match spawned {
            Err(e) => e,
            Ok(mut client) => {
                client.shutdown();
                panic!("handshake against an always-error server must fail");
            }
        };
        assert!(err.message.contains("initialize"), "{err}");
    }
}
