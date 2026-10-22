//! Run store: `run.json` state, per-stage status, tasks and attempts.
//! Stage lifecycle: pending → running → validating → waiting_approval →
//! succeeded; failures: failed / blocked / cancelled / budget_exhausted /
//! stale / not_requested.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::digest_value;
use crate::error::{HarnessError, Result};

pub const RUN_FILE: &str = "run.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    NotRequested,
    Pending,
    Running,
    Validating,
    WaitingApproval,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
    BudgetExhausted,
    Stale,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StageStatus::NotRequested => "not_requested",
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Validating => "validating",
            StageStatus::WaitingApproval => "waiting_approval",
            StageStatus::Succeeded => "succeeded",
            StageStatus::Failed => "failed",
            StageStatus::Blocked => "blocked",
            StageStatus::Cancelled => "cancelled",
            StageStatus::BudgetExhausted => "budget_exhausted",
            StageStatus::Stale => "stale",
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StageStatus::Succeeded
                | StageStatus::Failed
                | StageStatus::Cancelled
                | StageStatus::Blocked
                | StageStatus::BudgetExhausted
                | StageStatus::Stale
                | StageStatus::WaitingApproval
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskState {
    pub id: String,
    pub stage: String,
    pub status: StageStatus,
    pub attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_attempt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_digest: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failure_history: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_reason: Option<String>,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            id: String::new(),
            stage: String::new(),
            status: StageStatus::Pending,
            attempts: 0,
            current_attempt: None,
            input_digest: None,
            output_digest: None,
            failure_history: Vec::new(),
            stopped_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunState {
    pub run_id: String,
    pub harness_version: String,
    pub created_ms: u64,
    pub brief_digest: String,
    pub config_digest: String,
    pub stages: BTreeMap<String, StageStatus>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tasks: BTreeMap<String, TaskState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_revision: Option<String>,
    pub status: String,
}

impl Default for RunState {
    fn default() -> Self {
        Self {
            run_id: String::new(),
            harness_version: crate::VERSION.to_string(),
            created_ms: 0,
            brief_digest: String::new(),
            config_digest: String::new(),
            stages: BTreeMap::new(),
            tasks: BTreeMap::new(),
            until: None,
            design_revision: None,
            assets_revision: None,
            status: "created".into(),
        }
    }
}

pub const STAGES: &[&str] = &["design", "assets"];

impl RunState {
    pub fn new(run_id: String, brief_digest: String, config_digest: String, until: &str) -> Self {
        let mut stages = BTreeMap::new();
        let stop = STAGES.iter().position(|s| *s == until);
        for (i, stage) in STAGES.iter().enumerate() {
            stages.insert(
                (*stage).to_string(),
                if Some(i) <= stop {
                    StageStatus::Pending
                } else {
                    StageStatus::NotRequested
                },
            );
        }
        Self {
            run_id,
            created_ms: now_ms(),
            brief_digest,
            config_digest,
            stages,
            until: Some(until.to_string()),
            ..Default::default()
        }
    }

    pub fn digest(&self) -> Result<String> {
        digest_value(self)
    }

    pub fn task_mut(&mut self, stage: &str, id: &str) -> &mut TaskState {
        let key = format!("{stage}/{id}");
        self.tasks.entry(key).or_insert_with(|| TaskState {
            id: id.to_string(),
            stage: stage.to_string(),
            ..Default::default()
        })
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct RunStore {
    dir: PathBuf,
}

impl RunStore {
    pub fn create(dir: PathBuf, state: &RunState) -> Result<Self> {
        fs::create_dir_all(&dir)
            .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?;
        let store = Self { dir };
        store.save(state)?;
        Ok(store)
    }

    pub fn open(dir: PathBuf) -> Result<Self> {
        if !dir.is_dir() {
            return Err(HarnessError::not_found(format!(
                "run directory {} does not exist",
                dir.display()
            )));
        }
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self, state: &RunState) -> Result<()> {
        let path = self.dir.join(RUN_FILE);
        let text = serde_json::to_string_pretty(state)?;
        write_atomic(&path, text.as_bytes())
    }

    pub fn load(&self) -> Result<RunState> {
        let path = self.dir.join(RUN_FILE);
        if !path.is_file() {
            return Err(HarnessError::not_found(format!(
                "missing {}",
                path.display()
            )));
        }
        let text = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn attempt_dir(&self, stage: &str, task: &str, attempt: u32) -> Result<PathBuf> {
        let dir = self
            .dir
            .join("attempts")
            .join(stage)
            .join(task)
            .join(format!("a{attempt:03}"));
        fs::create_dir_all(&dir)
            .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?;
        Ok(dir)
    }

    pub fn reports_dir(&self) -> Result<PathBuf> {
        let dir = self.dir.join("reports");
        fs::create_dir_all(&dir)
            .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?;
        Ok(dir)
    }

    pub fn evidence_dir(&self) -> Result<PathBuf> {
        let dir = self.dir.join("evidence");
        fs::create_dir_all(&dir)
            .map_err(|e| HarnessError::io(format!("{}: {e}", dir.display())))?;
        Ok(dir)
    }
}

/// Write to a temp sibling then rename over the target. fsync before rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| HarnessError::usage("path has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|e| HarnessError::io(format!("{}: {e}", parent.display())))?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
    ));
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| HarnessError::io(format!("{}: {e}", tmp.display())))?;
        f.write_all(bytes)
            .map_err(|e| HarnessError::io(format!("{}: {e}", tmp.display())))?;
        f.sync_all()
            .map_err(|e| HarnessError::io(format!("{}: {e}", tmp.display())))?;
    }
    fs::rename(&tmp, path)
        .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
    Ok(())
}

/// Advisory project lock: `runs/` and revisions are only mutated while held.
pub struct ProjectLock {
    file: Option<fs::File>,
}

impl ProjectLock {
    pub fn acquire(project: &Path) -> Result<Self> {
        let path = project.join(".harness.lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| HarnessError::io(format!("{}: {e}", path.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fl = libc_lock_exclusive(file.as_raw_fd());
            if fl != 0 {
                return Err(HarnessError::lock(format!(
                    "another harness process holds {}",
                    path.display()
                ))
                .with_hint("wait for it to finish or remove a stale lock"));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = &file;
        }
        Ok(Self {
            file: Some(file),
        })
    }

    pub fn try_acquire(project: &Path) -> Result<Self> {
        Self::acquire(project)
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // Closing releases flock; never unlink an inode another waiter uses.
        self.file = None;
    }
}

#[cfg(unix)]
fn libc_lock_exclusive(fd: i32) -> i32 {
    // Minimal flock(2) via libc without adding a dependency: use
    // `extern "C"` linkage guarded to unix.
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    unsafe { flock(fd, LOCK_EX | LOCK_NB) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl_h_run_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn run_state_roundtrip_and_stage_defaults() {
        let dir = tmp("rt");
        let state = RunState::new(
            "r1".into(),
            "sha256:a".into(),
            "sha256:b".into(),
            "assets",
        );
        assert_eq!(state.stages["design"], StageStatus::Pending);
        assert_eq!(state.stages["assets"], StageStatus::Pending);
        let until_design = RunState::new("r1".into(), "d".into(), "c".into(), "design");
        assert_eq!(until_design.stages["assets"], StageStatus::NotRequested);
        let store = RunStore::create(dir.clone(), &state).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.run_id, "r1");
        assert!(store.attempt_dir("assets", "t1", 1).unwrap().is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn project_lock_keeps_stable_inode() {
        let dir = tmp("lock");
        let lock = ProjectLock::acquire(&dir).unwrap();
        assert!(ProjectLock::acquire(&dir).is_err());
        drop(lock);
        assert!(dir.join(".harness.lock").is_file());
        let lock = ProjectLock::acquire(&dir).unwrap();
        assert!(ProjectLock::acquire(&dir).is_err());
        drop(lock);
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = tmp("aw");
        let p = dir.join("x.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
    }
}
