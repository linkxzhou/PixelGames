//! Strict validator for the frozen BeepBox JSON subset (A0, section 14.4).
//!
//! BeepBox's own loader clamps and silently drops anything it dislikes: notes
//! past the end of a bar, overlapping notes, unknown instrument fields. That is
//! fine for an editor and wrong for a build, so everything here is an error.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::error::{AudioError, AudioResult};
use crate::limits::{MAX_BGM_BARS, MAX_BGM_FRAMES, MAX_POLYPHONY, MAX_VOICES, SAMPLE_RATE};


use super::consts::*;

pub fn validate_song(
    song: &Value,
    loop_start_bar: u64,
    loop_end_bar: u64,
) -> AudioResult<SongFacts> {
    let root = obj(song, "song")?;
    only_keys(root, SONG_KEYS, "song")?;
    if text(root.get("format").unwrap_or(&Value::Null), "format")? != BEEPBOX_FORMAT {
        return Err(AudioError::semantic("song.format must be \"BeepBox\""));
    }
    let version = uint(root.get("version").unwrap_or(&Value::Null), "version")?;
    if version != BEEPBOX_VERSION {
        return Err(AudioError::semantic(format!(
            "song.version {version} is unsupported; the frozen subset is {BEEPBOX_VERSION}"
        )));
    }
    one_of(root.get("scale").unwrap_or(&Value::Null), "scale", SCALES)?;
    one_of(root.get("key").unwrap_or(&Value::Null), "key", KEYS)?;
    let intro = uint(root.get("introBars").unwrap_or(&Value::Null), "introBars")?;
    let loop_bars = uint(root.get("loopBars").unwrap_or(&Value::Null), "loopBars")?;
    if intro != 0 {
        return Err(AudioError::semantic(
            "introBars must be 0; the whole song is the loop",
        ));
    }
    if loop_start_bar != 0 || loop_end_bar != loop_bars {
        return Err(AudioError::semantic(format!(
            "spec loop {loop_start_bar}..{loop_end_bar} must equal song loop 0..{loop_bars}"
        )));
    }
    if loop_bars == 0 || loop_bars > MAX_BGM_BARS {
        return Err(AudioError::limit(format!(
            "loopBars {loop_bars} is outside 1..={MAX_BGM_BARS}"
        )));
    }
    let beats_per_bar = uint(
        root.get("beatsPerBar").unwrap_or(&Value::Null),
        "beatsPerBar",
    )?;
    if !(3..=16).contains(&beats_per_bar) {
        return Err(AudioError::semantic("beatsPerBar must be in 3..=16"));
    }
    let tpb = uint(
        root.get("ticksPerBeat").unwrap_or(&Value::Null),
        "ticksPerBeat",
    )?;
    if !ALLOWED_TICKS_PER_BEAT.contains(&tpb) {
        return Err(AudioError::semantic(format!(
            "ticksPerBeat {tpb} is not one of {ALLOWED_TICKS_PER_BEAT:?} (freehand 24 is excluded)"
        )));
    }
    let bpm = uint(
        root.get("beatsPerMinute").unwrap_or(&Value::Null),
        "beatsPerMinute",
    )?;
    if !(30..=300).contains(&bpm) {
        return Err(AudioError::semantic("beatsPerMinute must be in 30..=300"));
    }
    let numerator = SAMPLE_RATE as u64 * 60 * beats_per_bar * loop_bars;
    if (numerator) % (bpm) != 0 {
        return Err(AudioError::semantic(format!(
            "{bpm} bpm with {beats_per_bar} beats per bar and {loop_bars} bars gives a fractional loop length; \
             choose a tempo that divides {numerator}"
        ))
        .with_hint("tempos such as 100, 120, 125, 140 and 150 give whole-sample loops at 8 beats per bar"));
    }
    let loop_frames = numerator / bpm;
    if loop_frames > MAX_BGM_FRAMES {
        return Err(AudioError::limit(format!(
            "loop is {loop_frames} frames; the limit is {MAX_BGM_FRAMES}"
        )));
    }
    for (key, name) in [
        ("layeredInstruments", "layeredInstruments"),
        ("patternInstruments", "patternInstruments"),
    ] {
        if let Some(v) = root.get(key) {
            if v.as_bool() != Some(false) {
                return Err(AudioError::semantic(format!("{name} must be false")));
            }
        }
    }

    let channels = arr(root.get("channels").unwrap_or(&Value::Null), "channels")?;
    if channels.is_empty() || channels.len() > MAX_VOICES {
        return Err(AudioError::limit(format!(
            "channels must be 1..={MAX_VOICES}"
        )));
    }
    let bar_ticks = beats_per_bar * tpb;
    let mut max_polyphony = 0usize;
    for (ci, channel) in channels.iter().enumerate() {
        let what = format!("channels[{ci}]");
        max_polyphony = max_polyphony.max(validate_channel(channel, &what, loop_bars, bar_ticks)?);
    }
    Ok(SongFacts {
        bpm,
        beats_per_bar,
        bars: loop_bars,
        ticks_per_beat: tpb,
        voices: channels.len(),
        max_polyphony,
        loop_frames,
    })
}


pub(crate) fn validate_channel(channel: &Value, what: &str, bars: u64, bar_ticks: u64) -> AudioResult<usize> {
    let map = obj(channel, what)?;
    only_keys(map, CHANNEL_KEYS, what)?;
    let kind = text(
        map.get("type").unwrap_or(&Value::Null),
        &format!("{what}.type"),
    )?;
    let drum = match kind {
        "pitch" => false,
        "drum" => true,
        _ => {
            return Err(AudioError::semantic(format!(
                "{what}.type '{kind}' must be pitch or drum"
            )))
        }
    };
    let instruments = arr(
        map.get("instruments").unwrap_or(&Value::Null),
        &format!("{what}.instruments"),
    )?;
    if instruments.len() != 1 {
        return Err(AudioError::semantic(format!(
            "{what} must have exactly one instrument (patternInstruments is not supported)"
        )));
    }
    validate_instrument(&instruments[0], &format!("{what}.instruments[0]"), drum)?;

    let sequence = arr(
        map.get("sequence").unwrap_or(&Value::Null),
        &format!("{what}.sequence"),
    )?;
    if sequence.len() as u64 != bars {
        return Err(AudioError::semantic(format!(
            "{what}.sequence has {} entries; expected {bars}",
            sequence.len()
        )));
    }
    let patterns = arr(
        map.get("patterns").unwrap_or(&Value::Null),
        &format!("{what}.patterns"),
    )?;
    let mut used = BTreeSet::new();
    for (si, entry) in sequence.iter().enumerate() {
        let index = uint(entry, &format!("{what}.sequence[{si}]"))?;
        if index > patterns.len() as u64 {
            return Err(AudioError::semantic(format!(
                "{what}.sequence[{si}] refers to pattern {index}; only {} exist",
                patterns.len()
            )));
        }
        if index > 0 {
            used.insert(index as usize - 1);
        }
    }
    let mut max_poly = 0usize;
    for (pi, pattern) in patterns.iter().enumerate() {
        let w = format!("{what}.patterns[{pi}]");
        let pmap = obj(pattern, &w)?;
        only_keys(pmap, &["notes"], &w)?;
        if !used.contains(&pi) {
            // Unused patterns are still validated so a stale one cannot hide bad data.
        }
        let notes = arr(
            pmap.get("notes").unwrap_or(&Value::Null),
            &format!("{w}.notes"),
        )?;
        max_poly = max_poly.max(validate_pattern(notes, &w, drum, bar_ticks)?);
    }
    Ok(max_poly)
}

pub(crate) fn validate_pattern(notes: &[Value], what: &str, drum: bool, bar_ticks: u64) -> AudioResult<usize> {
    let pitch_max = if drum { DRUM_MAX } else { PITCH_MAX };
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for (ni, note) in notes.iter().enumerate() {
        let w = format!("{what}.notes[{ni}]");
        let map = obj(note, &w)?;
        only_keys(map, &["pitches", "points"], &w)?;
        let pitches = arr(
            map.get("pitches").unwrap_or(&Value::Null),
            &format!("{w}.pitches"),
        )?;
        if pitches.is_empty() || pitches.len() > MAX_POLYPHONY {
            return Err(AudioError::limit(format!(
                "{w}.pitches must hold 1..={MAX_POLYPHONY} entries"
            )));
        }
        let mut seen = BTreeSet::new();
        for p in pitches {
            let value = int(p, &format!("{w}.pitches"))?;
            if !(0..=pitch_max).contains(&value) {
                return Err(AudioError::semantic(format!(
                    "{w} pitch {value} is outside 0..={pitch_max}"
                )));
            }
            if !seen.insert(value) {
                return Err(AudioError::semantic(format!("{w} repeats pitch {value}")));
            }
        }
        let points = arr(
            map.get("points").unwrap_or(&Value::Null),
            &format!("{w}.points"),
        )?;
        if points.len() < 2 {
            return Err(AudioError::semantic(format!(
                "{w}.points needs at least two points"
            )));
        }
        let mut last = None::<u64>;
        for (pi, point) in points.iter().enumerate() {
            let pw = format!("{w}.points[{pi}]");
            let pm = obj(point, &pw)?;
            only_keys(pm, &["tick", "pitchBend", "volume"], &pw)?;
            let tick = uint(
                pm.get("tick").unwrap_or(&Value::Null),
                &format!("{pw}.tick"),
            )?;
            if tick > bar_ticks {
                return Err(AudioError::semantic(format!(
                    "{pw}.tick {tick} runs past the bar ({bar_ticks} ticks); notes may not cross bars"
                )));
            }
            if let Some(prev) = last {
                if tick <= prev {
                    return Err(AudioError::semantic(format!("{pw}.tick must increase")));
                }
            }
            last = Some(tick);
            if let Some(bend) = pm.get("pitchBend") {
                if int(bend, &format!("{pw}.pitchBend"))? != 0 {
                    return Err(AudioError::semantic(format!(
                        "{pw}.pitchBend must be 0 in v1"
                    )));
                }
            }
            if let Some(vol) = pm.get("volume") {
                let v = uint(vol, &format!("{pw}.volume"))?;
                if v > 100 {
                    return Err(AudioError::semantic(format!("{pw}.volume must be 0..=100")));
                }
            }
        }
        let start = int_first(points)?;
        let end = last.unwrap_or(start);
        spans.push((start, end));
    }
    spans.sort_unstable();
    let mut poly = 0usize;
    // Notes in one pattern may not overlap: BeepBox keeps only one note per time slot.
    for window in spans.windows(2) {
        if window[1].0 < window[0].1 {
            return Err(AudioError::semantic(format!(
                "{what} has overlapping notes"
            )));
        }
    }
    if !spans.is_empty() {
        poly = 1;
    }
    Ok(poly)
}

pub(crate) fn int_first(points: &[Value]) -> AudioResult<u64> {
    uint(&points[0]["tick"], "first point tick")
}

pub(crate) fn validate_instrument(inst: &Value, what: &str, drum: bool) -> AudioResult<()> {
    let map = obj(inst, what)?;
    only_keys(map, INSTRUMENT_KEYS, what)?;
    let kind = text(
        map.get("type").unwrap_or(&Value::Null),
        &format!("{what}.type"),
    )?;
    if !INSTRUMENT_TYPES.contains(&kind) {
        return Err(AudioError::semantic(format!(
            "{what}.type '{kind}' is not supported (FM is excluded)"
        )));
    }
    let noise_family = matches!(kind, "noise" | "spectrum" | "drumset");
    if noise_family != drum {
        return Err(AudioError::semantic(format!(
            "{what}.type '{kind}' does not match its channel (drum channels take noise, spectrum, drumset)"
        )));
    }
    if let Some(v) = map.get("volume") {
        let volume = uint(v, &format!("{what}.volume"))?;
        if volume > 100 {
            return Err(AudioError::semantic(format!(
                "{what}.volume must be 0..=100"
            )));
        }
    }
    if let Some(effects) = map.get("effects") {
        for effect in arr(effects, &format!("{what}.effects"))? {
            one_of(effect, &format!("{what}.effects"), ALLOWED_EFFECTS)?;
        }
    }
    if let Some(v) = map.get("eqFilter") {
        if !arr(v, &format!("{what}.eqFilter"))?.is_empty() {
            return Err(AudioError::semantic(format!(
                "{what}.eqFilter must be empty in v1"
            )));
        }
    }
    if let Some(v) = map.get("transition") {
        one_of(v, &format!("{what}.transition"), TRANSITIONS)?;
    }
    if let Some(v) = map.get("chord") {
        one_of(v, &format!("{what}.chord"), CHORDS)?;
    }
    if let Some(v) = map.get("unison") {
        one_of(v, &format!("{what}.unison"), UNISONS)?;
    }
    if let Some(v) = map.get("fadeInSeconds") {
        if v.as_f64().is_none_or(|f| !(0.0..=1.0).contains(&f)) {
            return Err(AudioError::semantic(format!(
                "{what}.fadeInSeconds must be 0..=1"
            )));
        }
    }
    if let Some(v) = map.get("fadeOutTicks") {
        let ticks = int(v, &format!("{what}.fadeOutTicks"))?;
        if !FADE_OUT_TICKS.contains(&ticks) {
            return Err(AudioError::semantic(format!(
                "{what}.fadeOutTicks {ticks} is not a BeepBox step"
            )));
        }
    }
    if let Some(v) = map.get("wave") {
        let allowed = if noise_family {
            NOISE_WAVES
        } else {
            CHIP_WAVES
        };
        one_of(v, &format!("{what}.wave"), allowed)?;
    }
    if let Some(envelopes) = map.get("envelopes") {
        for (ei, envelope) in arr(envelopes, &format!("{what}.envelopes"))?
            .iter()
            .enumerate()
        {
            let w = format!("{what}.envelopes[{ei}]");
            let em = obj(envelope, &w)?;
            only_keys(em, &["target", "envelope", "index"], &w)?;
            one_of(
                em.get("target").unwrap_or(&Value::Null),
                &format!("{w}.target"),
                &["noteVolume"],
            )?;
            one_of(
                em.get("envelope").unwrap_or(&Value::Null),
                &format!("{w}.envelope"),
                ENVELOPES,
            )?;
        }
    }
    Ok(())
}

