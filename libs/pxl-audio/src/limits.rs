//! Frozen limits and profile constants for `audio-spec/v1`.
//!
//! These are engineering limits chosen in A0 (plan/v5.0/web-js-engine-plan.md
//! section 14.4), not limits of the underlying synthesizers. Exceeding one is
//! an error; nothing is silently truncated.

pub const SPEC_CONTRACT: &str = "audio-spec/v1";
pub const MANIFEST_CONTRACT: &str = "audio-manifest/v1";
pub const QA_PROFILE: &str = "audio-qa/v1";

pub const SAMPLE_RATE: u32 = 44_100;
pub const SFX_CHANNELS: u16 = 1;
pub const BGM_CHANNELS: u16 = 2;
pub const PCM_FORMAT: &str = "pcm_s16le";

/// recipe + every song file it references, in bytes.
pub const MAX_JSON_BYTES: usize = 256 * 1024;
pub const MAX_ASSETS: usize = 64;
pub const MAX_SFX_FRAMES: u64 = 2 * SAMPLE_RATE as u64;
pub const MAX_BGM_FRAMES: u64 = 60 * SAMPLE_RATE as u64;
pub const MAX_BGM_BARS: u64 = 32;
pub const MAX_VOICES: usize = 8;
pub const MAX_POLYPHONY: usize = 4;
pub const MAX_GAIN: f64 = 2.0;

/// BeepBox loop passes rendered and discarded before the kept pass.
pub const BGM_WARMUP_PASSES: u32 = 2;

pub const RENDER_TIMEOUT_SECS: u64 = 120;
pub const MAX_STDERR_BYTES: usize = 64 * 1024;
pub const MIN_NODE_MAJOR: u32 = 22;
