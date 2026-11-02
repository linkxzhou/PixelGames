//! Runs the Node adapter for exactly one asset.
//!
//! The adapter is started with an argument vector (never a shell), under
//! `node --permission` with read access to its own directory only, so it cannot
//! write files or spawn processes. Output sizes and wall time are bounded.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use crate::error::{AudioError, AudioResult};
use crate::limits::{MAX_STDERR_BYTES, MIN_NODE_MAJOR, RENDER_TIMEOUT_SECS};

#[derive(Debug, Clone)]
pub struct Backend {
    pub node: PathBuf,
    pub adapter_dir: PathBuf,
    pub node_version: String,
    pub adapter_version: String,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub channels: u16,
    pub frames: u64,
    pub pcm: Vec<f32>,
    pub node_version: String,
    pub adapter_version: String,
}

#[derive(Deserialize)]
struct Header {
    adapter_version: String,
    node: String,
    channels: u16,
    frames: u64,
    bytes: u64,
}

/// Locate Node and the adapter. `PXL_AUDIO_NODE` and `PXL_AUDIO_ADAPTER` are the
/// explicit overrides; otherwise Node comes from `PATH` and the adapter from an
/// `audio-toolchain/` next to the binary or, in a checkout, the source tree.
pub fn locate(adapter_override: Option<&Path>) -> AudioResult<Backend> {
    let node = find_node()?;
    let adapter_dir = find_adapter(adapter_override)?;
    let node_version = node_version(&node)?;
    let adapter_version = adapter_version(&node, &adapter_dir)?;
    Ok(Backend {
        node,
        adapter_dir,
        node_version,
        adapter_version,
    })
}

fn find_node() -> AudioResult<PathBuf> {
    if let Some(explicit) = std::env::var_os("PXL_AUDIO_NODE") {
        let path = PathBuf::from(explicit);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(AudioError::blocked(format!(
                "PXL_AUDIO_NODE {} is not a file",
                path.display()
            )))
        };
    }
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(AudioError::blocked("Node.js was not found on PATH")
        .with_hint("install Node 22 or newer, or set PXL_AUDIO_NODE; audio render needs it"))
}

fn find_adapter(adapter_override: Option<&Path>) -> AudioResult<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = adapter_override {
        candidates.push(path.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PXL_AUDIO_ADAPTER") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("audio-toolchain"));
            // skill layout: bin/<platform>/step-300 next to ../../assets/audio-toolchain
            candidates.push(dir.join("../../assets/audio-toolchain"));
        }
    }
    #[cfg(debug_assertions)]
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../audio-adapter"));
    for candidate in &candidates {
        if candidate.join("cli.mjs").is_file() {
            return std::fs::canonicalize(candidate)
                .map_err(|e| AudioError::io(format!("cannot resolve adapter: {e}")));
        }
    }
    Err(
        AudioError::blocked("the audio adapter (cli.mjs) was not found")
            .with_hint("set PXL_AUDIO_ADAPTER to the audio-toolchain directory"),
    )
}

fn node_version(node: &Path) -> AudioResult<String> {
    let out = Command::new(node)
        .arg("--version")
        .env_remove("NODE_OPTIONS")
        .output()
        .map_err(|e| AudioError::blocked(format!("cannot run node: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let major = text
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|m| m.parse::<u32>().ok())
        .ok_or_else(|| AudioError::blocked(format!("unrecognised node version '{text}'")))?;
    if major < MIN_NODE_MAJOR {
        return Err(AudioError::blocked(format!(
            "node {text} is older than the required {MIN_NODE_MAJOR}"
        ))
        .with_hint("--permission needs Node 22 or newer"));
    }
    Ok(text)
}

fn adapter_version(node: &Path, dir: &Path) -> AudioResult<String> {
    let out = permissioned(node, dir)
        .arg(dir.join("cli.mjs"))
        .arg("--version")
        .output()
        .map_err(|e| AudioError::blocked(format!("cannot run the adapter: {e}")))?;
    if !out.status.success() {
        return Err(AudioError::blocked(format!(
            "the adapter failed its version check: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn permissioned(node: &Path, dir: &Path) -> Command {
    let mut cmd = Command::new(node);
    // A parent NODE_OPTIONS (for example an IDE preload) would need read access
    // the sandbox deliberately does not grant.
    cmd.env_remove("NODE_OPTIONS");
    cmd.arg("--permission")
        .arg(format!("--allow-fs-read={}", dir.display()));
    cmd
}

pub fn render(backend: &Backend, request: &Value) -> AudioResult<Rendered> {
    let body = serde_json::to_vec(request)
        .map_err(|e| AudioError::backend(format!("cannot encode request: {e}")))?;
    let mut child = permissioned(&backend.node, &backend.adapter_dir)
        .arg(backend.adapter_dir.join("cli.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AudioError::blocked(format!("cannot start the adapter: {e}")))?;

    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&body);
    });

    // 64 MiB covers the largest legal BGM (60 s stereo float32 is 21 MiB).
    let max_stdout: u64 = 64 * 1024 * 1024;
    let (out_tx, out_rx) = mpsc::channel();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = (&mut stdout)
            .take(max_stdout + 1)
            .read_to_end(&mut buf)
            .map(|_| buf);
        let _ = out_tx.send(result);
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stderr)
            .take(MAX_STDERR_BYTES as u64)
            .read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + Duration::from_secs(RENDER_TIMEOUT_SECS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(AudioError::backend(format!(
                        "the adapter did not finish within {RENDER_TIMEOUT_SECS} seconds"
                    )));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(AudioError::backend(format!(
                    "cannot wait for the adapter: {e}"
                )))
            }
        }
    };
    let _ = writer.join();
    let stdout_bytes = out_rx
        .recv()
        .map_err(|_| AudioError::backend("adapter output thread vanished"))?
        .map_err(|e| AudioError::backend(format!("cannot read adapter output: {e}")))?;
    let _ = out_thread.join();
    let stderr_bytes = err_thread.join().unwrap_or_default();

    if !status.success() {
        let message = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(AudioError::backend(if message.is_empty() {
            format!("the adapter exited with {status}")
        } else {
            message
        }));
    }
    if stdout_bytes.len() as u64 > max_stdout {
        return Err(AudioError::limit(
            "the adapter produced more than 64 MiB of output",
        ));
    }
    parse_reply(&stdout_bytes)
}

fn parse_reply(bytes: &[u8]) -> AudioResult<Rendered> {
    let newline = bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or_else(|| AudioError::backend("adapter reply has no header line"))?;
    let header: Header = serde_json::from_slice(&bytes[..newline])
        .map_err(|e| AudioError::backend(format!("adapter header is invalid: {e}")))?;
    let body = &bytes[newline + 1..];
    if header.bytes as usize != body.len() {
        return Err(AudioError::backend(format!(
            "adapter promised {} bytes but sent {}",
            header.bytes,
            body.len()
        )));
    }
    if header.channels == 0 || header.channels > 2 {
        return Err(AudioError::backend(
            "adapter reported an unsupported channel count",
        ));
    }
    if body.len() as u64 != header.frames * header.channels as u64 * 4 {
        return Err(AudioError::backend(
            "adapter frame count does not match its payload",
        ));
    }
    let pcm: Vec<f32> = body
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    if pcm.iter().any(|v| !v.is_finite()) {
        return Err(AudioError::backend("adapter returned a non-finite sample"));
    }
    Ok(Rendered {
        channels: header.channels,
        frames: header.frames,
        pcm,
        node_version: header.node,
        adapter_version: header.adapter_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(channels: u16, frames: u64, floats: &[f32]) -> Vec<u8> {
        let body: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut out = format!(
            "{{\"adapter_version\":\"1.0.0\",\"node\":\"v22.0.0\",\"channels\":{channels},\"frames\":{frames},\"bytes\":{}}}\n",
            body.len()
        )
        .into_bytes();
        out.extend(body);
        out
    }

    #[test]
    fn parses_a_well_formed_reply() {
        let r = parse_reply(&reply(1, 2, &[0.5, -0.5])).unwrap();
        assert_eq!((r.channels, r.frames, r.pcm.len()), (1, 2, 2));
    }

    #[test]
    fn rejects_malformed_replies() {
        assert!(parse_reply(b"no newline").is_err());
        assert!(parse_reply(&reply(1, 3, &[0.1, 0.2])).is_err());
        let mut short = reply(1, 2, &[0.1, 0.2]);
        short.pop();
        assert!(parse_reply(&short).is_err());
        assert!(parse_reply(&reply(1, 1, &[f32::NAN])).is_err());
        assert!(parse_reply(&reply(3, 1, &[0.0, 0.0, 0.0])).is_err());
    }
}
