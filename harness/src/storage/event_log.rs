//! Append-only event log with monotonic sequence numbers. Every tool intent
//! (`started`) is persisted before the side effect runs; the matching
//! `finished` event records the real outcome.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::digest_bytes;
use crate::error::{HarnessError, Result};

pub const EVENTS_FILE: &str = "events.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    RunCreated {
        harness_version: String,
        config_digest: String,
    },
    StageStarted {
        stage: String,
        task_id: String,
    },
    ModelTurnStarted {
        stage: String,
        task_id: String,
        turn: u32,
    },
    ModelTurnFinished {
        stage: String,
        task_id: String,
        turn: u32,
        stop_reason: String,
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    ToolStarted {
        stage: String,
        task_id: String,
        attempt: u32,
        tool: String,
        arguments_digest: String,
    },
    ToolFinished {
        stage: String,
        task_id: String,
        attempt: u32,
        tool: String,
        ok: bool,
        exit_code: Option<i32>,
        artifact_digests: Vec<(String, String)>,
        error_code: Option<String>,
    },
    GateFinished {
        stage: String,
        task_id: String,
        passed: bool,
        failures: u32,
        report_digest: String,
    },
    StageStatus {
        stage: String,
        status: String,
    },
    RevisionCommitted {
        kind: String,
        revision: String,
        digest: String,
    },
    ApprovalRecorded {
        stage: String,
        revision: String,
        digest: String,
    },
    ReviewRequested {
        stage: String,
        task_id: String,
        digest: String,
    },
    RunStatus {
        status: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggedEvent {
    pub seq: u64,
    pub ts_ms: u64,
    #[serde(flatten)]
    pub event: Event,
}

pub struct EventLog {
    path: PathBuf,
    file: Option<File>,
    next_seq: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl EventLog {
    pub fn open(run_dir: &Path) -> Result<Self> {
        fs_create_dir_all(run_dir)?;
        let path = run_dir.join(EVENTS_FILE);
        let next_seq = if path.is_file() {
            Self::read_all(run_dir)?.len() as u64
        } else {
            0
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        Ok(Self {
            path,
            file: Some(file),
            next_seq,
        })
    }

    pub fn append(&mut self, event: Event) -> Result<LoggedEvent> {
        let logged = LoggedEvent {
            seq: self.next_seq,
            ts_ms: now_ms(),
            event,
        };
        let mut file = self.file.as_ref()
            .ok_or_else(|| HarnessError::io("event log is closed"))?;
        let line = serde_json::to_string(&logged)?;
        writeln!(file, "{}", line)
            .map_err(|e| HarnessError::io(format!("{}: {e}", self.path.display())))?;
        file.flush()
            .map_err(|e| HarnessError::io(format!("{}: {e}", self.path.display())))?;
        file.sync_all()
            .map_err(|e| HarnessError::io(format!("{}: {e}", self.path.display())))?;
        self.next_seq += 1;
        Ok(logged)
    }

    pub fn read_all(run_dir: &Path) -> Result<Vec<LoggedEvent>> {
        let path = run_dir.join(EVENTS_FILE);
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        for (i, line) in std::io::BufRead::lines(reader).enumerate() {
            let line = line.map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: LoggedEvent = serde_json::from_str(&line).map_err(|e| {
                HarnessError::integrity(format!(
                    "{} line {}: {e}",
                    path.display(),
                    i + 1
                ))
                .with_hint("the event log is append-only; a torn line means a crash mid-write")
            })?;
            out.push(ev);
        }
        for (i, ev) in out.iter().enumerate() {
            if ev.seq != i as u64 {
                return Err(HarnessError::integrity(format!(
                    "event sequence mismatch: expected {i}, found {}", ev.seq
                )));
            }
        }
        Ok(out)
    }

    pub fn close(&mut self) {
        self.file = None;
    }
}

fn fs_create_dir_all(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))
}

/// Digest of an event batch, used to bind approvals and checkpoints.
pub fn events_digest(events: &[LoggedEvent]) -> String {
    let mut text = String::new();
    for ev in events {
        text.push_str(&serde_json::to_string(ev).unwrap_or_default());
        text.push('\n');
    }
    digest_bytes(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corrupt_sequence_and_closed_log() {
        let dir = std::env::temp_dir().join(format!("pxl_h_events_corrupt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let event = LoggedEvent {
            seq: 9,
            ts_ms: 0,
            event: Event::RunStatus { status: "running".into() },
        };
        std::fs::write(dir.join(EVENTS_FILE), format!("{}\n", serde_json::to_string(&event).unwrap())).unwrap();
        assert!(EventLog::read_all(&dir).is_err());
        assert!(EventLog::open(&dir).is_err());
        std::fs::write(dir.join(EVENTS_FILE), "").unwrap();
        let mut log = EventLog::open(&dir).unwrap();
        log.close();
        assert!(log.append(Event::RunStatus { status: "failed".into() }).is_err());
    }

    #[test]
    fn append_and_reload_preserves_order() {
        let dir = std::env::temp_dir().join(format!("pxl_h_events_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut log = EventLog::open(&dir).unwrap();
            log.append(Event::RunCreated {
                harness_version: "0.1.0".into(),
                config_digest: "sha256:x".into(),
            })
            .unwrap();
            log.append(Event::StageStatus {
                stage: "design".into(),
                status: "running".into(),
            })
            .unwrap();
        }
        let read = EventLog::read_all(&dir).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].seq, 0);
        assert_eq!(read[1].seq, 1);
        assert!(matches!(read[0].event, Event::RunCreated { .. }));
        // Reopen continues the sequence.
        let mut log = EventLog::open(&dir).unwrap();
        log.append(Event::StageStatus {
            stage: "assets".into(),
            status: "running".into(),
        })
        .unwrap();
        assert_eq!(EventLog::read_all(&dir).unwrap().len(), 3);
    }
}
