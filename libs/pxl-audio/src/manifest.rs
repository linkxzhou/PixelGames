//! `audio-manifest/v1`: the record of one render. It holds no timestamps, host
//! names or absolute paths, so identical inputs on the same platform give an
//! identical file.

use serde::{Deserialize, Serialize};

use crate::qa::QaReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub contract: String,
    pub asset: String,
    pub root_seed: u64,
    pub sample_rate: u32,
    pub pcm_format: String,
    pub spec_sha256: String,
    pub toolchain: Toolchain,
    pub qa_profile: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Toolchain {
    pub node: String,
    pub adapter_version: String,
    pub adapter_sha256: String,
    pub beepbox: String,
    pub zzfx: String,
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub engine: String,
    pub seed: u32,
    pub channels: u16,
    pub frames: u64,
    pub file_sha256: String,
    pub pcm_sha256: String,
    pub deps_sha256: String,
    /// Source song file for BGM, relative to the spec directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "loop")]
    pub loop_frames: Option<LoopFrames>,
    pub qa: EntryQa,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopFrames {
    pub start_frame: u64,
    pub end_frame_exclusive: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryQa {
    pub status: String,
    pub peak_dbfs: f64,
    pub rms_dbfs: f64,
    pub clipped_samples: u64,
    pub failed: Vec<String>,
}

impl EntryQa {
    pub fn from_report(report: &QaReport) -> Self {
        let failed = report
            .checks
            .iter()
            .filter(|c| !c.pass)
            .map(|c| c.id.to_string())
            .collect();
        Self {
            status: report.status().to_string(),
            peak_dbfs: report.peak_dbfs,
            rms_dbfs: report.rms_dbfs,
            clipped_samples: report.clipped_samples,
            failed,
        }
    }
}

impl Manifest {
    pub fn to_json(&self) -> serde_json::Result<String> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}
