//! `audio-spec/v1` document model. Every struct denies unknown fields so a typo
//! is an error instead of a silently ignored parameter.

use serde::{Deserialize, Serialize};

use crate::error::{AudioError, AudioResult};
use crate::limits::SAMPLE_RATE;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSpec {
    pub contract: String,
    pub asset: String,
    pub seed: u64,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default)]
    pub sfx: Vec<SfxSpec>,
    #[serde(default)]
    pub bgm: Vec<BgmSpec>,
}

fn default_sample_rate() -> u32 {
    SAMPLE_RATE
}

fn default_gain() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SfxSpec {
    pub id: String,
    pub engine: String,
    pub params: SfxParams,
    #[serde(default = "default_gain")]
    pub gain: f64,
}

/// Named ZzFX parameters. Units are fixed: Hz, seconds, linear amounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SfxParams {
    pub waveform: Option<String>,
    pub volume: Option<f64>,
    pub randomness: Option<f64>,
    pub frequency_hz: Option<f64>,
    pub attack_sec: Option<f64>,
    pub decay_sec: Option<f64>,
    pub sustain_sec: Option<f64>,
    pub release_sec: Option<f64>,
    pub shape_curve: Option<f64>,
    pub slide: Option<f64>,
    pub delta_slide: Option<f64>,
    pub pitch_jump_hz: Option<f64>,
    pub pitch_jump_time_sec: Option<f64>,
    pub repeat_time_sec: Option<f64>,
    pub noise: Option<f64>,
    pub modulation: Option<f64>,
    pub bit_crush: Option<f64>,
    pub delay_sec: Option<f64>,
    pub sustain_volume: Option<f64>,
    pub tremolo: Option<f64>,
    pub filter_hz: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BgmSpec {
    pub id: String,
    pub engine: String,
    pub song_file: String,
    #[serde(rename = "loop")]
    pub loop_range: LoopRange,
    #[serde(default = "default_gain")]
    pub gain: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LoopRange {
    pub start_bar: u32,
    pub end_bar_exclusive: u32,
}

pub fn parse_spec(bytes: &[u8]) -> AudioResult<AudioSpec> {
    serde_json::from_slice::<AudioSpec>(bytes)
        .map_err(|e| AudioError::schema(format!("audio spec: {e}")))
}

/// Largest finite value any numeric spec field may take.
pub fn require_finite(name: &str, value: f64) -> AudioResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AudioError::semantic(format!("{name} must be finite")))
    }
}
