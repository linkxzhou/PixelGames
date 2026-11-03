//! Engineering QA on rendered PCM (`audio-qa/v1`). These are machine checks for
//! defects, not a judgement of taste: passing them says nothing about whether
//! the audio sounds good. That needs a human listen and stays NOT_ASSESSED.
//!
//! Severity: `error` fails the render, `warning` is reported (and fails under
//! `--strict`), `info` is recorded only.

use serde::Serialize;

use crate::limits::SAMPLE_RATE;

/// Samples at full scale before quantization. Any is an error: the encoder
/// would clamp them and distort the waveform.
pub const MAX_CLIPPED_SAMPLES: u64 = 0;
/// Peak ceiling in dBFS. -1 dBFS leaves headroom for decoder resampling.
pub const PEAK_CEILING_DBFS: f64 = -1.0;
/// Below this peak the asset is treated as silent.
pub const SILENCE_PEAK_DBFS: f64 = -60.0;
/// Mean sample value above this fraction of full scale is a DC offset.
pub const DC_OFFSET_LIMIT: f64 = 0.02;
/// Loop seam: sample-to-sample jump across the wrap, as a fraction of the
/// largest jump found inside the loop. A seam far above the music's own
/// movement is an audible click.
pub const SEAM_JUMP_RATIO: f64 = 4.0;
/// Absolute floor so a near-silent loop does not trip the ratio test.
pub const SEAM_ABS_FLOOR: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaCheck {
    pub id: &'static str,
    pub pass: bool,
    pub severity: Severity,
    pub value: f64,
    pub limit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaReport {
    pub profile: &'static str,
    pub checks: Vec<QaCheck>,
    pub peak_dbfs: f64,
    pub rms_dbfs: f64,
    pub clipped_samples: u64,
}

impl QaReport {
    pub fn errors(&self) -> impl Iterator<Item = &QaCheck> {
        self.checks
            .iter()
            .filter(|c| !c.pass && c.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &QaCheck> {
        self.checks
            .iter()
            .filter(|c| !c.pass && c.severity == Severity::Warning)
    }

    pub fn ok(&self) -> bool {
        self.errors().next().is_none()
    }

    /// `pass`, `warning` or `fail`.
    pub fn status(&self) -> &'static str {
        if !self.ok() {
            "fail"
        } else if self.warnings().next().is_some() {
            "warning"
        } else {
            "pass"
        }
    }
}

pub struct QaInput<'a> {
    /// Final 16-bit samples, interleaved.
    pub samples: &'a [i16],
    pub channels: u16,
    /// Samples that exceeded full scale before clamping.
    pub clipped: u64,
    pub is_loop: bool,
    pub expected_frames: Option<u64>,
    pub max_frames: u64,
}

fn dbfs(linear: f64) -> f64 {
    if linear <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * linear.log10()
    }
}

pub fn run(input: &QaInput) -> QaReport {
    let channels = input.channels as usize;
    let frames = (input.samples.len() / channels) as u64;
    let mut peak = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut sums = vec![0.0f64; channels];
    for (i, &s) in input.samples.iter().enumerate() {
        let v = s as f64 / 32768.0;
        peak = peak.max(v.abs());
        sum_sq += v * v;
        sums[i % channels] += v;
    }
    let n = input.samples.len().max(1) as f64;
    let rms = (sum_sq / n).sqrt();
    let dc = sums
        .iter()
        .map(|s| (s / frames.max(1) as f64).abs())
        .fold(0.0, f64::max);
    let peak_db = dbfs(peak);
    let rms_db = dbfs(rms);

    let mut checks = vec![
        QaCheck {
            id: "frames.nonempty",
            pass: frames > 0,
            severity: Severity::Error,
            value: frames as f64,
            limit: 1.0,
        },
        QaCheck {
            id: "frames.within_limit",
            pass: frames <= input.max_frames,
            severity: Severity::Error,
            value: frames as f64,
            limit: input.max_frames as f64,
        },
        QaCheck {
            id: "clipping.none",
            pass: input.clipped == MAX_CLIPPED_SAMPLES,
            severity: Severity::Error,
            value: input.clipped as f64,
            limit: MAX_CLIPPED_SAMPLES as f64,
        },
        QaCheck {
            id: "peak.below_ceiling",
            pass: peak_db <= PEAK_CEILING_DBFS,
            severity: Severity::Warning,
            value: peak_db,
            limit: PEAK_CEILING_DBFS,
        },
        QaCheck {
            id: "silence.audible",
            pass: peak_db > SILENCE_PEAK_DBFS,
            severity: Severity::Error,
            value: peak_db,
            limit: SILENCE_PEAK_DBFS,
        },
        QaCheck {
            id: "dc_offset.small",
            pass: dc <= DC_OFFSET_LIMIT,
            severity: Severity::Warning,
            value: dc,
            limit: DC_OFFSET_LIMIT,
        },
    ];
    if let Some(expected) = input.expected_frames {
        checks.push(QaCheck {
            id: "frames.match_spec",
            pass: frames == expected,
            severity: Severity::Error,
            value: frames as f64,
            limit: expected as f64,
        });
    }
    if input.is_loop && frames > 1 {
        let (seam, interior) = seam_metrics(input.samples, channels);
        let limit = (interior * SEAM_JUMP_RATIO).max(SEAM_ABS_FLOOR);
        checks.push(QaCheck {
            id: "loop.seam_continuous",
            pass: seam <= limit,
            severity: Severity::Warning,
            value: seam,
            limit,
        });
    }
    QaReport {
        profile: "audio-qa/v1",
        checks,
        peak_dbfs: round3(peak_db),
        rms_dbfs: round3(rms_db),
        clipped_samples: input.clipped,
    }
}

/// Largest per-channel jump across the wrap (last frame to first frame) and the
/// largest jump between adjacent frames inside the loop.
fn seam_metrics(samples: &[i16], channels: usize) -> (f64, f64) {
    let frames = samples.len() / channels;
    let at = |frame: usize, ch: usize| samples[frame * channels + ch] as f64 / 32768.0;
    let mut seam = 0.0f64;
    let mut interior = 0.0f64;
    for ch in 0..channels {
        seam = seam.max((at(0, ch) - at(frames - 1, ch)).abs());
        for f in 1..frames {
            interior = interior.max((at(f, ch) - at(f - 1, ch)).abs());
        }
    }
    (seam, interior)
}

fn round3(v: f64) -> f64 {
    if v.is_finite() {
        (v * 1000.0).round() / 1000.0
    } else {
        -999.0
    }
}

/// Convert a frame count to seconds for display; never used for contracts.
pub fn seconds(frames: u64) -> f64 {
    frames as f64 / SAMPLE_RATE as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(samples: &'a [i16], channels: u16, clipped: u64, is_loop: bool) -> QaInput<'a> {
        QaInput {
            samples,
            channels,
            clipped,
            is_loop,
            expected_frames: None,
            max_frames: 1_000_000,
        }
    }

    fn sine(frames: usize, amp: f64, hz: f64) -> Vec<i16> {
        (0..frames)
            .map(|i| {
                let t = i as f64 / SAMPLE_RATE as f64;
                (amp * (2.0 * std::f64::consts::PI * hz * t).sin() * 32768.0).round() as i16
            })
            .collect()
    }

    #[test]
    fn healthy_signal_passes() {
        let s = sine(4410, 0.5, 440.0);
        let r = run(&input(&s, 1, 0, false));
        assert!(r.ok(), "{:?}", r.checks);
        assert_eq!(r.status(), "pass");
    }

    #[test]
    fn silence_is_an_error() {
        let r = run(&input(&vec![0i16; 1000], 1, 0, false));
        assert!(r.errors().any(|c| c.id == "silence.audible"));
    }

    #[test]
    fn clipping_is_an_error_even_when_the_output_is_clamped() {
        let r = run(&input(&sine(1000, 0.5, 440.0), 1, 3, false));
        assert!(r.errors().any(|c| c.id == "clipping.none"));
    }

    #[test]
    fn hot_peak_warns_and_dc_offset_warns() {
        let hot = sine(1000, 0.99, 440.0);
        let r = run(&input(&hot, 1, 0, false));
        assert!(r.ok());
        assert!(r.warnings().any(|c| c.id == "peak.below_ceiling"));
        let dc: Vec<i16> = sine(1000, 0.2, 440.0)
            .iter()
            .map(|s| s.saturating_add(3000))
            .collect();
        assert!(run(&input(&dc, 1, 0, false))
            .warnings()
            .any(|c| c.id == "dc_offset.small"));
    }

    #[test]
    fn loop_seam_detects_a_click_but_accepts_a_whole_period() {
        // 441 Hz at 44100 Hz repeats every 100 frames, so 1000 frames wrap cleanly.
        let clean = sine(1000, 0.5, 441.0);
        assert!(run(&input(&clean, 1, 0, true)).ok());
        assert!(run(&input(&clean, 1, 0, true)).warnings().next().is_none());
        // A slow tone that ends at +0.8 and restarts at -0.8 wraps with a hard
        // step many times larger than any movement inside the loop.
        let mut click: Vec<i16> = (0..1000)
            .map(|i| ((i as f64 / 1000.0 * 1.6 - 0.8) * 32768.0).round() as i16)
            .collect();
        click[0] = (-0.8f64 * 32768.0).round() as i16;
        let r = run(&input(&click, 1, 0, true));
        assert!(
            r.warnings().any(|c| c.id == "loop.seam_continuous"),
            "{:?}",
            r.checks
        );
    }

    #[test]
    fn expected_frame_count_is_enforced() {
        let s = sine(100, 0.5, 440.0);
        let mut i = input(&s, 1, 0, false);
        i.expected_frames = Some(101);
        assert!(run(&i).errors().any(|c| c.id == "frames.match_spec"));
    }

    #[test]
    fn stereo_channels_are_measured_per_channel() {
        let mut s = Vec::new();
        for v in sine(500, 0.3, 440.0) {
            s.push(v);
            s.push(v.saturating_add(4000));
        }
        let r = run(&input(&s, 2, 0, false));
        assert!(r.warnings().any(|c| c.id == "dc_offset.small"));
    }
}
