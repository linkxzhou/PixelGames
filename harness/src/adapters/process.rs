//! Controlled process execution: parameter arrays (never a shell string),
//! drained stdout/stderr with size caps, timeouts and process-group cleanup.

#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcOutput {
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub elapsed_ms: u64,
}

impl ProcOutput {
    pub fn ok(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

#[derive(Debug, Clone)]
pub struct ProcSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
}

pub fn run(spec: &ProcSpec) -> Result<ProcOutput> {
    let argv = format_argv(&spec.program, &spec.args);
    let started = std::time::Instant::now();
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .current_dir(spec.cwd.as_deref().unwrap_or(std::path::Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // setpgid after exec fails with EACCES, so a shell grandchild (sleep) stays
    // in our group, survives kill(pid), and holds the pipes open until it exits.
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| HarnessError::tool(format!("cannot spawn {}: {e}", spec.program)))?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| HarnessError::tool("child stdout was not captured"))?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| HarnessError::tool("child stderr was not captured"))?;
    // Drain both pipes on threads so a large stream cannot block the child.
    let max = spec.max_output_bytes;
    let out_handle = std::thread::spawn(move || read_capped(&mut stdout_pipe, max));
    let err_handle = std::thread::spawn(move || read_capped(&mut stderr_pipe, max));
    let deadline = std::time::Duration::from_secs(spec.timeout_secs.max(1));
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= deadline {
                    timed_out = true;
                    kill_group(&mut child);
                    break child
                        .wait()
                        .map_err(|e| HarnessError::tool(format!("wait failed: {e}")))?;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(HarnessError::tool(format!("wait failed: {e}"))),
        }
    };
    let (stdout, stdout_truncated) = out_handle.join().unwrap_or((String::new(), false));
    let (stderr, stderr_truncated) = err_handle.join().unwrap_or((String::new(), false));
    Ok(ProcOutput {
        argv,
        exit_code: status.code(),
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        timed_out,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn format_argv(program: &str, args: &[String]) -> Vec<String> {
    let mut v = vec![program.to_string()];
    v.extend(args.iter().cloned());
    v
}

fn read_capped<R: std::io::Read>(r: &mut R, max: usize) -> (String, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = max.saturating_sub(buf.len());
                if room == 0 {
                    truncated = true;
                    // Keep draining but discard, so the pipe never fills.
                    continue;
                }
                let take = n.min(room);
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&buf).into_owned(), truncated)
}

#[cfg(unix)]
fn kill_group(child: &mut std::process::Child) {
    extern "C" {
        fn kill(pgid: i32, sig: i32) -> i32;
    }
    const SIGKILL: i32 = 9;
    let pid = child.id() as i32;
    // The child was spawned with process_group(0), so its pgid is its pid.
    let _ = unsafe { kill(-pid, SIGKILL) };
    let _ = unsafe { kill(pid, SIGKILL) };
    let _ = child.wait();
}

#[cfg(not(unix))]
fn kill_group(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(program: &str, args: &[&str]) -> ProcSpec {
        ProcSpec {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            timeout_secs: 10,
            max_output_bytes: 64 * 1024,
        }
    }

    #[test]
    fn captures_exit_code_and_output() {
        let out = run(&spec("sh", &["-c", "echo hi; echo err 1>&2; exit 3"])).unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stdout.trim(), "hi");
        assert_eq!(out.stderr.trim(), "err");
        assert!(!out.ok());
    }

    #[test]
    fn timeout_kills_process_group() {
        let mut s = spec("sh", &["-c", "sleep 30"]);
        s.timeout_secs = 1;
        let start = std::time::Instant::now();
        let out = run(&s).unwrap();
        assert!(out.timed_out);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn large_output_is_capped_and_drained() {
        let mut s = spec("sh", &["-c", "yes 0123456789 | head -c 100000"]);
        s.max_output_bytes = 4096;
        let out = run(&s).unwrap();
        assert!(out.stdout_truncated);
        assert_eq!(out.stdout.len(), 4096);
    }
}
