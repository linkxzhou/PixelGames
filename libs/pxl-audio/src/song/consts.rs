//! Strict validator for the frozen BeepBox JSON subset (A0, section 14.4).
//!
//! BeepBox's own loader clamps and silently drops anything it dislikes: notes
//! past the end of a bar, overlapping notes, unknown instrument fields. That is
//! fine for an editor and wrong for a build, so everything here is an error.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::error::{AudioError, AudioResult};
use crate::limits::{MAX_BGM_BARS, MAX_BGM_FRAMES, MAX_POLYPHONY, MAX_VOICES, SAMPLE_RATE};


pub const BEEPBOX_FORMAT: &str = "BeepBox";
pub const BEEPBOX_VERSION: u64 = 9;
/// partsPerBeat in beepbox 4.2.0 Config; notes are written in `ticksPerBeat`.
pub(crate) const ALLOWED_TICKS_PER_BEAT: &[u64] = &[3, 4, 6, 8];
pub(crate) const PITCH_MAX: i64 = 84;
pub(crate) const DRUM_MAX: i64 = 11;

pub(crate) const INSTRUMENT_TYPES: &[&str] = &[
    "chip",
    "noise",
    "PWM",
    "harmonics",
    "Picked String",
    "spectrum",
    "drumset",
    "supersaw",
];
/// FM is excluded: it rendered silent in the A0 probe with default operators.
pub(crate) const SONG_KEYS: &[&str] = &[
    "format",
    "version",
    "scale",
    "key",
    "introBars",
    "loopBars",
    "beatsPerBar",
    "ticksPerBeat",
    "beatsPerMinute",
    "layeredInstruments",
    "patternInstruments",
    "channels",
];
pub(crate) const CHANNEL_KEYS: &[&str] = &[
    "type",
    "instruments",
    "patterns",
    "sequence",
    "octaveScrollBar",
];
pub(crate) const INSTRUMENT_KEYS: &[&str] = &[
    "type",
    "volume",
    "eqFilter",
    "effects",
    "transition",
    "chord",
    "fadeInSeconds",
    "fadeOutTicks",
    "wave",
    "unison",
    "pulseWidth",
    "envelopes",
    "harmonics",
    "spectrum",
    "drums",
    "preset",
    "dynamism",
    "spread",
    "shape",
    "stringSustain",
    "stringSustainType",
];
pub(crate) const ALLOWED_EFFECTS: &[&str] = &["transition type", "chord type"];
pub(crate) const CHIP_WAVES: &[&str] = &[
    "rounded",
    "triangle",
    "square",
    "1/4 pulse",
    "1/8 pulse",
    "sawtooth",
    "double saw",
    "double pulse",
    "spiky",
];
pub(crate) const NOISE_WAVES: &[&str] = &["retro", "white", "clang", "buzz", "hollow"];
pub(crate) const UNISONS: &[&str] = &[
    "none",
    "shimmer",
    "hum",
    "honky tonk",
    "dissonant",
    "fifth",
    "octave",
    "bowed",
    "piano",
];
pub(crate) const CHORDS: &[&str] = &["simultaneous", "strum", "arpeggio"];
pub(crate) const TRANSITIONS: &[&str] = &[
    "normal",
    "interrupt",
    "continue",
    "slide",
    "slide in pattern",
];
pub(crate) const ENVELOPES: &[&str] = &[
    "none",
    "note size",
    "punch",
    "flare 1",
    "flare 2",
    "flare 3",
    "twang 1",
    "twang 2",
    "twang 3",
    "swell 1",
    "swell 2",
    "swell 3",
    "decay 1",
    "decay 2",
    "decay 3",
];
pub(crate) const SCALES: &[&str] = &[
    "easy :)",
    "easy :(",
    "island :)",
    "island :(",
    "blues :)",
    "blues :(",
    "normal :)",
    "normal :(",
    "double harmonic :)",
    "double harmonic :(",
    "strange",
    "expert",
];
pub(crate) const KEYS: &[&str] = &[
    "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
];
pub(crate) const FADE_OUT_TICKS: &[i64] = &[-24, -12, -6, -3, -1, 6, 12, 24, 48, 72, 96];

/// What the Rust side needs to know about a validated song.
#[derive(Debug, Clone, PartialEq)]
pub struct SongFacts {
    pub bpm: u64,
    pub beats_per_bar: u64,
    pub bars: u64,
    pub ticks_per_beat: u64,
    pub voices: usize,
    pub max_polyphony: usize,
    pub loop_frames: u64,
}

pub(crate) fn obj<'a>(value: &'a Value, what: &str) -> AudioResult<&'a serde_json::Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| AudioError::semantic(format!("{what} must be an object")))
}

pub(crate) fn arr<'a>(value: &'a Value, what: &str) -> AudioResult<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| AudioError::semantic(format!("{what} must be an array")))
}

pub(crate) fn uint(value: &Value, what: &str) -> AudioResult<u64> {
    value
        .as_u64()
        .ok_or_else(|| AudioError::semantic(format!("{what} must be a non-negative integer")))
}

pub(crate) fn int(value: &Value, what: &str) -> AudioResult<i64> {
    value
        .as_i64()
        .ok_or_else(|| AudioError::semantic(format!("{what} must be an integer")))
}

pub(crate) fn text<'a>(value: &'a Value, what: &str) -> AudioResult<&'a str> {
    value
        .as_str()
        .ok_or_else(|| AudioError::semantic(format!("{what} must be a string")))
}

pub(crate) fn one_of(value: &Value, what: &str, allowed: &[&str]) -> AudioResult<()> {
    let s = text(value, what)?;
    if allowed.contains(&s) {
        Ok(())
    } else {
        Err(AudioError::semantic(format!(
            "{what} '{s}' is not in the supported set"
        )))
    }
}

pub(crate) fn only_keys(
    map: &serde_json::Map<String, Value>,
    allowed: &[&str],
    what: &str,
) -> AudioResult<()> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(AudioError::semantic(format!(
                "{what} has unsupported field '{key}'"
            )));
        }
    }
    Ok(())
}

