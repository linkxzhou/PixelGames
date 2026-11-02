use std::collections::BTreeMap;

use pxl_core::Palette;
use pxl_format::{parse_ascii_spec, AsciiSpec, LegendValue};

use super::compile_ascii;

fn p16() -> Palette {
    crate::builtin_palette("p16").unwrap()
}

fn spec(text: &str) -> AsciiSpec {
    parse_ascii_spec(text).unwrap()
}

fn none() -> BTreeMap<String, LegendValue> {
    BTreeMap::new()
}

fn pixel(frame: &pxl_core::IndexFrame, x: i32, y: i32) -> u8 {
    frame.get(x, y)
}

#[test]
fn digits_are_palette_indices_and_the_layout_is_centred() {
    let s = spec(r#"{"v":1,"size":32,"palette":"p16","frames":{"base":{"rows":["12","03"]}}}"#);
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    assert_eq!(got.frames.len(), 1);
    let frame = &got.frames[0].1;
    assert_eq!((frame.w, frame.h), (32, 32));
    assert_eq!(pixel(frame, 15, 15), 1);
    assert_eq!(pixel(frame, 16, 15), 2);
    assert_eq!(pixel(frame, 15, 16), 0);
    assert_eq!(pixel(frame, 16, 16), 3);
    assert_eq!(got.palette.id, "p16");
    assert!(got.palette_file.is_none());
}

#[test]
fn a_legend_colour_must_exist_in_the_palette() {
    let s = spec(
        r##"{"v":1,"size":32,"palette":"p16","legend":{"1":"#deeed6"},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    assert_eq!(got.frames[0].1.get(15, 15), 15);

    let bad = spec(
        r##"{"v":1,"size":32,"palette":"p16","legend":{"1":"#123456"},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let err = compile_ascii(&bad, Some(&pal), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.color_not_in_palette"),
        "{}",
        err.message
    );
    assert!(err.hint.unwrap_or_default().contains("nearest"));
}

#[test]
fn extend_appends_the_missing_colour() {
    let s = spec(
        r##"{"v":1,"size":32,"palette":"p16","palette_mode":"extend","legend":{"1":"#ff0000"},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    assert_eq!(got.palette.colors.len(), 17);
    assert_eq!(got.palette.colors[16].to_hex(), "#ff0000");
    let file = got.palette_file.expect("extended palette is written");
    assert_eq!(file.colors.len(), 17);
}

#[test]
fn without_a_palette_the_legend_builds_one() {
    let s = spec(
        r##"{"v":1,"size":32,"legend":{"1":"#ff0000","2":"#00ff00"},"frames":{"base":{"rows":["121"]}}}"##,
    );
    let got = compile_ascii(&s, None, &none()).unwrap();
    assert_eq!(got.palette.colors.len(), 3);
    assert_eq!(
        got.palette.special.outline, 1,
        "outline falls back to index 1"
    );
    assert_eq!(got.palette.special.flash, 2);
    let frame = &got.frames[0].1;
    assert_eq!(pixel(frame, 14, 15), 1);
    assert_eq!(pixel(frame, 15, 15), 2);
    assert_eq!(pixel(frame, 16, 15), 1);
    assert!(got.palette_file.is_some());
}

#[test]
fn cells_expand_to_blocks_and_stamps_fill_them() {
    let s = spec(
        r##"{"v":1,"size":32,"cell":[2,2],"palette":"p16","anchor":"top-left","legend":{"1":{"stamp":["23","45"]}},"frames":{"base":{"rows":["11"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    let frame = &got.frames[0].1;
    assert_eq!(pixel(frame, 0, 0), 2);
    assert_eq!(pixel(frame, 1, 0), 3);
    assert_eq!(pixel(frame, 0, 1), 4);
    assert_eq!(pixel(frame, 1, 1), 5);
}

#[test]
fn checker_pattern_uses_base_and_alt() {
    let s = spec(
        r##"{"v":1,"size":32,"cell":[2,2],"palette":"p16","anchor":"top-left","legend":{"1":{"index":2,"pattern":"checker","alt":3}},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    let frame = &got.frames[0].1;
    assert_eq!(pixel(frame, 0, 0), 2);
    assert_eq!(pixel(frame, 1, 0), 3);
    assert_eq!(pixel(frame, 0, 1), 3);
    assert_eq!(pixel(frame, 1, 1), 2);
}

#[test]
fn bevel_uses_the_next_ramp_step() {
    let s = spec(
        r##"{"v":1,"size":32,"cell":[2,2],"palette":"p16","anchor":"top-left","legend":{"1":{"ramp":"metal","step":1,"pattern":"bevel"}},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    let frame = &got.frames[0].1;
    assert_eq!(pixel(frame, 0, 0), 4, "metal[1] is 4");
    assert_eq!(pixel(frame, 1, 1), 8, "metal[2] is 8");
}

#[test]
fn delta_patches_keep_and_erase_cells() {
    let s = spec(
        r#"{"v":1,"size":32,"palette":"p16","frames":{
            "base":{"rows":["11","11"]},
            "blink":{"from":"base","patch":[{"x":1,"y":0,"rows":["0"]},{"x":0,"y":1,"rows":["1."]}]}
        }}"#,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    let names: Vec<&str> = got.frames.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["base", "blink"]);
    let blink = &got.frames[1].1;
    assert_eq!(pixel(blink, 15, 15), 1);
    assert_eq!(pixel(blink, 16, 15), 0, "the patch erased this cell");
    assert_eq!(pixel(blink, 15, 16), 1, "keep left this cell alone");
    assert_eq!(pixel(blink, 16, 16), 1);
}

#[test]
fn layers_composite_in_order_with_offsets() {
    let s = spec(
        r#"{"v":1,"size":32,"palette":"p16","cell":[8,8],"anchor":"top-left",
            "layers":[
                {"name":"body","rows":["11","11"]},
                {"name":"eye","rows":["2"],"x":1,"y":0}
            ],
            "frames":{}}"#,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    assert_eq!(got.frames.len(), 1);
    assert_eq!(got.frames[0].0, "base");
    let frame = &got.frames[0].1;
    assert_eq!(pixel(frame, 0, 0), 1);
    assert_eq!(pixel(frame, 8, 0), 2);
    assert_eq!(pixel(frame, 8, 8), 1);
}

#[test]
fn shared_legend_entries_are_used_and_unused_ones_warn() {
    let s = spec(r#"{"v":1,"size":32,"palette":"p16","frames":{"base":{"rows":["1"]}}}"#);
    let pal = p16();
    let mut shared = BTreeMap::new();
    shared.insert("1".to_string(), LegendValue::Index(2));
    let got = compile_ascii(&s, Some(&pal), &shared).unwrap();
    assert_eq!(got.frames[0].1.get(15, 15), 2);
    assert!(!got.warnings.iter().any(|w| w.id == "ascii.unused_legend"));

    let with_local = spec(
        r##"{"v":1,"size":32,"palette":"p16","legend":{"3":4},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let got = compile_ascii(&with_local, Some(&pal), &shared).unwrap();
    assert!(got.warnings.iter().any(|w| w.id == "ascii.unused_legend"));
}

#[test]
fn auto_legend_entries_warn() {
    let s = spec(
        r##"{"v":1,"size":32,"legend":{"1":{"color":"#ff0000","auto":true}},"frames":{"base":{"rows":["1"]}}}"##,
    );
    let got = compile_ascii(&s, None, &none()).unwrap();
    assert!(got.warnings.iter().any(|w| w.id == "ascii.auto_legend"));
}

#[test]
fn canvas_and_placement_limits_are_reported() {
    let small: AsciiSpec = pxl_format::parse_json(
        r##"{"v":1,"size":16,"frames":{"base":{"rows":["1"]}}}"##,
        "ascii spec",
    )
    .unwrap();
    let err = small.validate().unwrap_err();
    assert!(err.message.contains("ascii.bad_size"), "{}", err.message);

    let big = spec(
        r##"{"v":1,"size":32,"palette":"p16","cell":[16,16],"frames":{"base":{"rows":["111"]}}}"##,
    );
    let pal = p16();
    let err = compile_ascii(&big, Some(&pal), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.size_overflow"),
        "{}",
        err.message
    );

    let offset = spec(
        r##"{"v":1,"size":32,"palette":"p16","offset":[20,0],"frames":{"base":{"rows":["11"]}}}"##,
    );
    let err = compile_ascii(&offset, Some(&pal), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.offset_out_of_bounds"),
        "{}",
        err.message
    );
}

#[test]
fn frame_cycles_and_unknown_sources_are_rejected() {
    let cycle =
        spec(r##"{"v":1,"size":32,"palette":"p16","frames":{"a":{"from":"b"},"b":{"from":"a"}}}"##);
    let pal = p16();
    let err = compile_ascii(&cycle, Some(&pal), &none()).unwrap_err();
    assert!(err.message.contains("ascii.frame_cycle"), "{}", err.message);

    let missing = spec(r#"{"v":1,"size":32,"palette":"p16","frames":{"a":{"from":"nope"}}}"#);
    let err = compile_ascii(&missing, Some(&pal), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.frame_source"),
        "{}",
        err.message
    );
}

#[test]
fn undefined_printable_characters_name_the_cell() {
    let s = spec(
        r##"{"v":1,"charset":"printable","size":32,"palette":"p16","frames":{"base":{"rows":["x"]}}}"##,
    );
    let pal = p16();
    let err = compile_ascii(&s, Some(&pal), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.undefined_char"),
        "{}",
        err.message
    );
    assert!(err
        .path
        .unwrap_or_default()
        .contains("/frames/base/rows/0/0"));
}

#[test]
fn mirror_and_post_are_returned_for_the_caller() {
    let s = spec(
        r##"{"v":1,"size":32,"palette":"p16","mirror":"x","post":[{"op":"noop"}],"frames":{"base":{"rows":["1"]}}}"##,
    );
    let pal = p16();
    let got = compile_ascii(&s, Some(&pal), &none()).unwrap();
    assert_eq!(got.mirror.as_deref(), Some("x"));
    assert_eq!(got.post.len(), 1);
    assert!(got.delta_ops.is_empty());
}

#[test]
fn size_overflow_names_the_largest_cell_that_fits() {
    let spec = spec(
        r#"{"palette":"p16","size":32,"cell":[8,8],"frames":{"base":{"rows":["11111","11111"]}}}"#,
    );
    let err = compile_ascii(&spec, Some(&p16()), &none()).unwrap_err();
    assert!(
        err.message.contains("ascii.size_overflow"),
        "{}",
        err.message
    );
    let hint = err.hint.clone().unwrap_or_default();
    assert!(hint.contains("6x6"), "{hint}");
}
