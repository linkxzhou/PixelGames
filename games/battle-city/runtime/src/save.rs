//! High score and best stage persistence.
//!
//! Writing goes through a temporary file plus rename so a crash cannot leave a
//! half-written save. A missing, unreadable or corrupt save falls back to the
//! defaults and the game keeps running.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Save {
    pub high_score: u32,
    pub best_stage: u32,
}

impl Default for Save {
    fn default() -> Self {
        Self {
            high_score: 0,
            best_stage: 1,
        }
    }
}

impl Save {
    pub fn to_json(self) -> String {
        format!(
            "{{\n  \"schema_version\": 1,\n  \"high_score\": {},\n  \"best_stage\": {}\n}}\n",
            self.high_score, self.best_stage
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        let doc: serde_json::Value = serde_json::from_str(text).ok()?;
        let high_score = doc["high_score"].as_u64().unwrap_or(0) as u32;
        let best_stage = doc["best_stage"].as_u64().unwrap_or(1).clamp(1, 35) as u32;
        Some(Self {
            high_score,
            best_stage,
        })
    }

    pub fn record(&mut self, score: u32, stage: usize) {
        self.high_score = self.high_score.max(score);
        let reached = (stage as u32 + 1).clamp(1, 35);
        self.best_stage = self.best_stage.max(reached);
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Atomic save: write the sibling temp file, then rename over the target.
pub fn write(path: &Path, save: Save) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let temp = temp_path(path);
    fs::write(&temp, save.to_json())?;
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn read(path: &Path) -> Save {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| Save::parse(&text))
        .unwrap_or_default()
}

/// Default location: next to the package's parent (`games/battle-city/save.json`).
pub fn default_path(package_root: &Path) -> PathBuf {
    package_root
        .parent()
        .map(|dir| dir.join("save.json"))
        .unwrap_or_else(|| PathBuf::from("save.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_temp_directory() {
        let dir = std::env::temp_dir().join(format!("bc-save-{}", std::process::id()));
        let path = dir.join("save.json");
        let save = Save {
            high_score: 12345,
            best_stage: 7,
        };
        write(&path, save).unwrap();
        assert_eq!(read(&path), save);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_save_falls_back() {
        assert_eq!(Save::parse("not json"), None);
        assert_eq!(
            read(Path::new("/definitely/missing/save.json")),
            Save::default()
        );
    }

    #[test]
    fn record_keeps_the_best_values() {
        let mut save = Save {
            high_score: 500,
            best_stage: 3,
        };
        save.record(400, 1);
        assert_eq!(save.high_score, 500);
        assert_eq!(save.best_stage, 3);
        save.record(900, 9);
        assert_eq!(save.high_score, 900);
        assert_eq!(save.best_stage, 10);
    }
}
