//! Artifact store: immutable revisions and project identity.
//!
//! Layout (relative to the game project root):
//! - `project.json`                project identity + current revisions
//! - `brief.json`                  input snapshot taken at `init`
//! - `revisions/design/<rev>/`     immutable design package
//! - `revisions/assets/<rev>/`     immutable asset package + manifest
//! - `runs/<run_id>/`              run state, events, attempts, reports
//! - `cache/`                      rebuildable cache (not a source of truth)

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::digest::{digest_bytes, digest_file, digest_pairs};
use crate::error::{HarnessError, Result};
use crate::storage::run_store::write_atomic;

pub const PROJECT_FILE: &str = "project.json";
pub const BRIEF_FILE: &str = "brief.json";
pub const REVISIONS_DIR: &str = "revisions";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectIdentity {
    pub game_id: String,
    pub harness_version: String,
    pub created_ms: u64,
    pub genre_template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub design_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets_revision: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<Approval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub stage: String,
    pub revision: String,
    pub digest: String,
    pub approved_ms: u64,
}

impl Default for ProjectIdentity {
    fn default() -> Self {
        Self {
            game_id: String::new(),
            harness_version: crate::VERSION.to_string(),
            created_ms: 0,
            genre_template: String::new(),
            design_revision: None,
            assets_revision: None,
            approvals: Vec::new(),
        }
    }
}

pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn init(root: PathBuf, identity: &ProjectIdentity) -> Result<Self> {
        if root.join(PROJECT_FILE).is_file() {
            return Err(HarnessError::exists(format!(
                "{} already exists",
                root.join(PROJECT_FILE).display()
            ))
            .with_hint("pxl-harness init refuses to overwrite an existing project"));
        }
        fs::create_dir_all(root.join(REVISIONS_DIR).join("design"))
            .map_err(|e| HarnessError::io(format!("{}: {e}", root.display())))?;
        fs::create_dir_all(root.join(REVISIONS_DIR).join("assets"))
            .map_err(|e| HarnessError::io(format!("{}: {e}", root.display())))?;
        fs::create_dir_all(root.join("runs"))
            .map_err(|e| HarnessError::io(format!("{}: {e}", root.display())))?;
        let store = Self { root };
        store.save_identity(identity)?;
        Ok(store)
    }

    pub fn open(root: PathBuf) -> Result<Self> {
        if !root.join(PROJECT_FILE).is_file() {
            return Err(HarnessError::not_found(format!(
                "{} is not a harness project (missing {PROJECT_FILE})",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save_identity(&self, identity: &ProjectIdentity) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        write_atomic(&path, serde_json::to_string_pretty(identity)?.as_bytes())
    }

    pub fn identity(&self) -> Result<ProjectIdentity> {
        let text = fs::read_to_string(self.root.join(PROJECT_FILE))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn brief_path(&self) -> PathBuf {
        self.root.join(BRIEF_FILE)
    }

    pub fn snapshot_brief(&self, brief: &serde_json::Value) -> Result<String> {
        let text = serde_json::to_string_pretty(brief)?;
        write_atomic(&self.brief_path(), text.as_bytes())?;
        Ok(digest_bytes(text.as_bytes()))
    }

    pub fn revision_dir(&self, kind: &str, revision: &str) -> PathBuf {
        self.root
            .join(REVISIONS_DIR)
            .join(kind)
            .join(sanitize_component(revision))
    }

    /// Commit a candidate directory as an immutable revision: copy files into
    /// the revision dir, compute a digest over the sorted file list, then
    /// flip the current pointer atomically.
    pub fn commit_revision(
        &self,
        kind: &str,
        revision: &str,
        candidate: &Path,
    ) -> Result<(PathBuf, String)> {
        let rev_dir = self.revision_dir(kind, revision);
        if rev_dir.exists() {
            return Err(HarnessError::exists(format!(
                "revision {} already exists",
                rev_dir.display()
            ))
            .with_hint("revisions are immutable; bump the revision id"));
        }
        fs::create_dir_all(&rev_dir)
            .map_err(|e| HarnessError::io(format!("{}: {e}", rev_dir.display())))?;
        let mut files: Vec<(String, String)> = Vec::new();
        for entry in walk_files(candidate)? {
            let rel = entry
                .strip_prefix(candidate)
                .map_err(|e| HarnessError::io(e.to_string()))?
                .to_string_lossy()
                .to_string();
            let dest = rev_dir.join(&rel);
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)
                    .map_err(|e| HarnessError::io(format!("{}: {e}", p.display())))?;
            }
            fs::copy(&entry, &dest)
                .map_err(|e| HarnessError::io(format!("{}: {e}", dest.display())))?;
            files.push((rel, digest_file(&dest)?));
        }
        files.sort();
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let digest = digest_pairs(pairs);
        let manifest = serde_json::json!({
            "kind": kind,
            "revision": revision,
            "digest": digest,
            "files": files,
        });
        write_atomic(
            &rev_dir.join("revision.manifest.json"),
            serde_json::to_string_pretty(&manifest)?.as_bytes(),
        )?;
        Ok((rev_dir, digest))
    }

    pub fn revision_digest(&self, kind: &str, revision: &str) -> Result<String> {
        let dir = self.revision_dir(kind, revision);
        let manifest_path = dir.join("revision.manifest.json");
        if !manifest_path.is_file() {
            return Err(HarnessError::not_found(format!(
                "revision {} has no manifest",
                dir.display()
            )));
        }
        let text = fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&text)?;
        Ok(manifest
            .get("digest")
            .and_then(|d| d.as_str())
            .unwrap_or_default()
            .to_string())
    }

    /// Recompute the digest over the revision's current files. Mismatches
    /// with the recorded manifest mean the revision was tampered with.
    pub fn revision_live_digest(&self, kind: &str, revision: &str) -> Result<String> {
        let dir = self.revision_dir(kind, revision);
        if !dir.is_dir() {
            return Err(HarnessError::not_found(format!(
                "revision {} does not exist",
                dir.display()
            )));
        }
        let mut files: Vec<(String, String)> = Vec::new();
        for entry in walk_files(&dir)? {
            let name = entry
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name == "revision.manifest.json" {
                continue;
            }
            let rel = entry
                .strip_prefix(&dir)
                .map_err(|e| HarnessError::io(e.to_string()))?
                .to_string_lossy()
                .to_string();
            let _ = name;
            files.push((rel, digest_file(&entry)?));
        }
        files.sort();
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        Ok(digest_pairs(pairs))
    }

    pub fn record_approval(&self, stage: &str, revision: &str, digest: &str) -> Result<()> {
        let digest = if digest.is_empty() {
            self.revision_digest(stage, revision)?
        } else {
            digest.to_string()
        };
        let mut identity = self.identity()?;
        identity
            .approvals
            .retain(|a| !(a.stage == stage && a.revision == revision));
        identity.approvals.push(Approval {
            stage: stage.to_string(),
            revision: revision.to_string(),
            digest,
            approved_ms: now_ms(),
        });
        match stage {
            "design" => identity.design_revision = Some(revision.to_string()),
            "assets" => identity.assets_revision = Some(revision.to_string()),
            _ => {}
        }
        self.save_identity(&identity)
    }

    pub fn approval(&self, stage: &str, revision: &str) -> Option<Approval> {
        self.identity()
            .ok()?
            .approvals
            .into_iter()
            .find(|a| a.stage == stage && a.revision == revision)
    }

    /// True when the recorded approval still matches the revision digest.
    /// Integrity check: the manifest digest must also match the live files,
    /// so tampering with a committed revision invalidates its approval.
    pub fn approval_valid(&self, stage: &str, revision: &str) -> Result<bool> {
        let Some(approval) = self.approval(stage, revision) else {
            return Ok(false);
        };
        let recorded = self.revision_digest(stage, revision)?;
        if recorded.is_empty() || recorded != approval.digest {
            return Ok(false);
        }
        let live = self.revision_live_digest(stage, revision)?;
        Ok(live == recorded)
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn sanitize_component(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        for entry in fs::read_dir(&cur)
            .map_err(|e| HarnessError::io(format!("{}: {e}", cur.display())))?
        {
            let entry =
                entry.map_err(|e| HarnessError::io(format!("{}: {e}", cur.display())))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pxl_h_art_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn init_refuses_twice_and_commits_are_immutable() {
        let root = tmp("init");
        let identity = ProjectIdentity {
            game_id: "tank-slice".into(),
            genre_template: "top_down_tank".into(),
            ..Default::default()
        };
        let store = ArtifactStore::init(root.clone(), &identity).unwrap();
        assert!(ArtifactStore::init(root.clone(), &identity).is_err());
        // Commit a candidate.
        let cand = root.join("candidate_tmp");
        fs::create_dir_all(cand.join("sub")).unwrap();
        fs::write(cand.join("a.json"), b"{}").unwrap();
        fs::write(cand.join("sub/b.json"), b"[]").unwrap();
        let (dir, digest) = store.commit_revision("design", "d001", &cand).unwrap();
        assert!(dir.join("a.json").is_file());
        assert!(dir.join("revision.manifest.json").is_file());
        assert!(digest.starts_with("sha256:"));
        assert!(store
            .commit_revision("design", "d001", &cand)
            .is_err());
        assert_eq!(store.revision_digest("design", "d001").unwrap(), digest);
    }

    #[test]
    fn approval_binds_digest() {
        let root = tmp("appr");
        let identity = ProjectIdentity {
            game_id: "g".into(),
            ..Default::default()
        };
        let store = ArtifactStore::init(root, &identity).unwrap();
        let cand = store.root().join("c");
        fs::create_dir_all(&cand).unwrap();
        fs::write(cand.join("x"), b"1").unwrap();
        let (_dir, digest) = store.commit_revision("design", "d1", &cand).unwrap();
        assert!(!store.approval_valid("design", "d1").unwrap());
        store.record_approval("design", "d1", "").unwrap();
        assert!(store.approval_valid("design", "d1").unwrap());
        // Tamper with the revision: the approval must become invalid.
        let f = store.revision_dir("design", "d1").join("x");
        fs::write(&f, b"2").unwrap();
        assert!(!store.approval_valid("design", "d1").unwrap());
        let _ = digest;
    }
}
