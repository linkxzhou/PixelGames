use super::*;
use serde_json::json;

fn instrument() -> Value {
    json!({"type":"chip","volume":100,"eqFilter":[],"effects":["chord type"],"chord":"simultaneous",
           "fadeInSeconds":0,"fadeOutTicks":-3,"wave":"square","unison":"none","envelopes":[]})
}

fn song(bpm: u64, bars: u64) -> Value {
    json!({
        "format":"BeepBox","version":9,"scale":"normal :(","key":"C","introBars":0,"loopBars":bars,
        "beatsPerBar":8,"ticksPerBeat":4,"beatsPerMinute":bpm,
        "layeredInstruments":false,"patternInstruments":false,
        "channels":[{"type":"pitch","instruments":[instrument()],
            "patterns":[{"notes":[{"pitches":[12],"points":[{"tick":0,"pitchBend":0,"volume":100},{"tick":8,"pitchBend":0,"volume":100}]}]}],
            "sequence": vec![1; bars as usize], "octaveScrollBar":3}]
    })
}

#[test]
fn accepts_a_valid_song_and_reports_loop_frames() {
    let facts = validate_song(&song(125, 2), 0, 2).unwrap();
    assert_eq!(facts.loop_frames, 338_688);
    assert_eq!(facts.voices, 1);
}

#[test]
fn rejects_fractional_loop_lengths() {
    let err = validate_song(&song(110, 2), 0, 2).unwrap_err();
    assert!(err.message.contains("fractional"), "{}", err.message);
}

#[test]
fn rejects_note_crossing_the_bar() {
    let mut s = song(125, 2);
    s["channels"][0]["patterns"][0]["notes"][0]["points"][1]["tick"] = json!(40);
    assert!(validate_song(&s, 0, 2)
        .unwrap_err()
        .message
        .contains("past the bar"));
}

#[test]
fn rejects_overlapping_notes() {
    let mut s = song(125, 2);
    let second = json!({"pitches":[14],"points":[{"tick":4,"pitchBend":0,"volume":100},{"tick":12,"pitchBend":0,"volume":100}]});
    s["channels"][0]["patterns"][0]["notes"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert!(validate_song(&s, 0, 2)
        .unwrap_err()
        .message
        .contains("overlapping"));
}

#[test]
fn rejects_unknown_fields_and_fm() {
    let mut s = song(125, 2);
    s["extra"] = json!(1);
    assert!(validate_song(&s, 0, 2)
        .unwrap_err()
        .message
        .contains("unsupported field"));
    let mut s = song(125, 2);
    s["channels"][0]["instruments"][0]["type"] = json!("FM");
    assert!(validate_song(&s, 0, 2)
        .unwrap_err()
        .message
        .contains("not supported"));
}

#[test]
fn rejects_loop_mismatch_and_pitch_out_of_range() {
    assert!(validate_song(&song(125, 2), 0, 1).is_err());
    let mut s = song(125, 2);
    s["channels"][0]["patterns"][0]["notes"][0]["pitches"] = json!([99]);
    assert!(validate_song(&s, 0, 2)
        .unwrap_err()
        .message
        .contains("outside"));
}

#[test]
fn rejects_empty_channels() {
    let mut s = song(125, 2);
    s["channels"] = json!([]);
    assert!(validate_song(&s, 0, 2).is_err());
}
