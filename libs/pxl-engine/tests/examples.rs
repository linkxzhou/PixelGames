//! The four examples the web page ships, rendered through the same entry point
//! the page uses. Pins the output rules and the shape of each result.

use pxl_engine::{
    detect_kind, inspect, render, FormatChoice, InputKind, MemoryResolver, OutputFormat,
    OutputPurpose, RenderRequest,
};

const GEM: &str = include_str!("../../../examples/ascii/gem/gem.ascii.txt");
const SLIME_TXT: &str = include_str!("../../../examples/ascii/slime/slime.ascii.txt");
const TANK_SHAPES: &str = include_str!("../../../examples/tank/tank_body.shapes.json");

fn none() -> MemoryResolver {
    MemoryResolver::new()
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF89a")
}

fn gif_frame_count(bytes: &[u8]) -> usize {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut dec = opts.read_info(bytes).unwrap();
    let mut n = 0;
    while dec.read_next_frame().unwrap().is_some() {
        n += 1;
    }
    n
}

#[test]
fn example_1_ascii_gem_is_a_static_png() {
    let out = render(
        &RenderRequest {
            input: GEM,
            kind: InputKind::Auto,
            palette: Some("p16"),
            scale: 8,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Png);
    assert!(is_png(&out.bytes));
    assert_eq!(out.info.kind, InputKind::AsciiText);
    assert_eq!((out.width, out.height), (32 * 8, 32 * 8));
    assert!(out.hash.starts_with("sha256:"));
}

#[test]
fn symbols_pick_printable_and_digits_stay_digits() {
    // `.#$` would be rejected under `digits`; the page must not make the user
    // know that flag.
    let symbols = render(
        &RenderRequest {
            input: GEM,
            palette: Some("p16"),
            ..Default::default()
        },
        &none(),
    );
    assert!(symbols.is_ok(), "{symbols:?}");
    let digits = render(
        &RenderRequest {
            input: SLIME_TXT,
            palette: Some("p16"),
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!(digits.info.frames.len(), 2);
}

#[test]
fn example_2_ascii_slime_with_two_blocks_is_still_a_png_without_anims() {
    // The text grid declares no animations, so `Auto` gives a PNG strip of the
    // frames rather than inventing motion.
    let out = render(
        &RenderRequest {
            input: SLIME_TXT,
            palette: Some("p16"),
            charset: Some(pxl_format::Charset::Digits),
            scale: 4,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Png);
    assert_eq!(out.info.frames.len(), 2);
}

#[test]
fn example_2_ascii_json_with_anims_is_a_looping_gif() {
    let mut res = none();
    res.insert(
        "../shared/p16.legend.json",
        include_str!("../../../examples/ascii/shared/p16.legend.json"),
    );
    let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
    let out = render(
        &RenderRequest {
            input: spec,
            kind: InputKind::Auto,
            scale: 6,
            ..Default::default()
        },
        &res,
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Gif);
    assert!(is_gif(&out.bytes));
    // `idle` wins over `hop` when the caller names none.
    assert_eq!(out.anim.as_deref(), Some("idle"));
    assert_eq!(gif_frame_count(&out.bytes), 2);
    assert_eq!(out.info.anims.len(), 2);
}

#[test]
fn example_3_shapes_json_is_a_png() {
    assert_eq!(detect_kind(TANK_SHAPES), InputKind::ShapesJson);
    let out = render(
        &RenderRequest {
            input: TANK_SHAPES,
            palette: Some("tank"),
            scale: 8,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Png);
    assert_eq!((out.width, out.height), (32 * 8, 32 * 8));
}

#[test]
fn example_4_macro_animation_is_a_gif() {
    let spec = r#"{
  "v": 1, "charset": "digits", "size": 64, "cell": [4, 4],
  "anchor": "center", "palette": "p16",
  "frames": { "base": { "rows": [
    "000111111000",
    "001222222100",
    "012222222210",
    "122332233221",
    "122222222221",
    "012222222210",
    "001222222100",
    "000111111000"
  ] } },
  "anims": [ { "name": "breathe", "fps": 6, "loop": true, "macro": "idle_breathe(period=4, amp=1)" } ]
}"#;
    let out = render(
        &RenderRequest {
            input: spec,
            scale: 6,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Gif);
    assert_eq!(out.anim.as_deref(), Some("breathe"));
    assert_eq!(gif_frame_count(&out.bytes), 4);
    assert_eq!(out.frame_count, 4);
}

#[test]
fn output_is_deterministic() {
    let req = RenderRequest {
        input: GEM,
        palette: Some("p16"),
        ..Default::default()
    };
    let a = render(&req, &none()).unwrap();
    let b = render(&req, &none()).unwrap();
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.bytes, b.bytes);
}

#[test]
fn asking_for_a_gif_without_animation_is_an_error() {
    let err = render(
        &RenderRequest {
            input: GEM,
            palette: Some("p16"),
            format: FormatChoice::Gif,
            ..Default::default()
        },
        &none(),
    )
    .unwrap_err();
    assert!(err.message.contains("no animation"), "{err:?}");
}

#[test]
fn forcing_png_on_an_animated_spec_draws_a_still() {
    let mut res = none();
    res.insert(
        "p16.legend.json",
        include_str!("../../../examples/ascii/shared/p16.legend.json"),
    );
    let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
    let out = render(
        &RenderRequest {
            input: spec,
            format: FormatChoice::Png,
            ..Default::default()
        },
        &res,
    )
    .unwrap();
    assert_eq!(out.format, OutputFormat::Png);
    assert!(is_png(&out.bytes));
}

#[test]
fn unknown_anim_is_reported() {
    let mut res = none();
    res.insert(
        "p16.legend.json",
        include_str!("../../../examples/ascii/shared/p16.legend.json"),
    );
    let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
    let err = render(
        &RenderRequest {
            input: spec,
            anim: Some("nope"),
            ..Default::default()
        },
        &res,
    )
    .unwrap_err();
    assert!(err.message.contains("unknown anim"), "{err:?}");
}

#[test]
fn inspect_lists_animations_without_encoding() {
    let mut res = none();
    res.insert(
        "p16.legend.json",
        include_str!("../../../examples/ascii/shared/p16.legend.json"),
    );
    let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
    let info = inspect(
        &RenderRequest {
            input: spec,
            ..Default::default()
        },
        &res,
    )
    .unwrap();
    let names: Vec<_> = info.anims.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["hop", "idle"]);
    assert_eq!(info.anims.iter().find(|a| a.name == "idle").unwrap().fps, 4);
}

#[test]
fn a_missing_shared_legend_says_so() {
    let spec = include_str!("../../../examples/ascii/slime/slime.ascii.json");
    let err = render(
        &RenderRequest {
            input: spec,
            ..Default::default()
        },
        &none(),
    )
    .unwrap_err();
    assert!(err.message.contains("legend"), "{err:?}");
}

#[test]
fn scale_and_empty_input_are_rejected() {
    let err = render(
        &RenderRequest {
            input: GEM,
            palette: Some("p16"),
            scale: 17,
            ..Default::default()
        },
        &none(),
    )
    .unwrap_err();
    assert!(err.message.contains("scale"), "{err:?}");
    let err = render(
        &RenderRequest {
            input: "   \n",
            ..Default::default()
        },
        &none(),
    )
    .unwrap_err();
    assert!(err.message.contains("empty"), "{err:?}");
}

#[test]
fn detect_kind_tells_the_json_shapes_apart() {
    assert_eq!(detect_kind("..##\n.##."), InputKind::AsciiText);
    assert_eq!(detect_kind(r#"{"draw":[]}"#), InputKind::ShapesJson);
    assert_eq!(
        detect_kind(r#"{"frames":{"base":{"rows":["1"]}}}"#),
        InputKind::AsciiJson
    );
    assert_eq!(
        detect_kind(r#"{"w":2,"h":2,"palette":"p16","frames":{"base":["00","00"]}}"#),
        InputKind::DocJson
    );
}

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    pxl_export::decode_png_rgba(bytes).unwrap()
}

#[test]
fn render_reports_native_size_scale_and_purpose() {
    let out = render(
        &RenderRequest {
            input: TANK_SHAPES,
            palette: Some("tank"),
            scale: 6,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    assert_eq!((out.native_width, out.native_height), (32, 32));
    assert_eq!(out.scale, 6);
    assert_eq!((out.width, out.height), (32 * 6, 32 * 6));
    assert!(!out.grid);
    assert_eq!(out.purpose.name(), "preview");
}

/// A larger scale only copies pixels: the 1x image is recovered by sampling the
/// scaled one, so scaling cannot add detail or blur.
#[test]
fn scaling_is_a_pure_pixel_copy_of_the_native_render() {
    let at = |scale| {
        let out = render(
            &RenderRequest {
                input: TANK_SHAPES,
                palette: Some("tank"),
                scale,
                purpose: OutputPurpose::Asset,
                ..Default::default()
            },
            &none(),
        )
        .unwrap();
        decode(&out.bytes)
    };
    let (w1, h1, one) = at(1);
    for scale in [2u32, 4, 8, 16] {
        let (w, h, big) = at(scale);
        assert_eq!((w, h), (w1 * scale, h1 * scale));
        for y in 0..h {
            for x in 0..w {
                let s = (((y / scale) * w1 + x / scale) * 4) as usize;
                let b = ((y * w + x) * 4) as usize;
                assert_eq!(big[b..b + 4], one[s..s + 4], "scale {scale} at {x},{y}");
            }
        }
    }
}

#[test]
fn asset_png_is_transparent_and_refuses_a_grid() {
    let asset = render(
        &RenderRequest {
            input: TANK_SHAPES,
            palette: Some("tank"),
            scale: 2,
            purpose: OutputPurpose::Asset,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    let (_, _, rgba) = decode(&asset.bytes);
    assert!(
        rgba.chunks(4).any(|p| p[3] == 0),
        "asset keeps transparency"
    );
    let preview = render(
        &RenderRequest {
            input: TANK_SHAPES,
            palette: Some("tank"),
            scale: 2,
            ..Default::default()
        },
        &none(),
    )
    .unwrap();
    let (_, _, rgba) = decode(&preview.bytes);
    assert!(rgba.chunks(4).all(|p| p[3] == 255), "preview is opaque");
    let err = render(
        &RenderRequest {
            input: TANK_SHAPES,
            palette: Some("tank"),
            grid: true,
            purpose: OutputPurpose::Asset,
            ..Default::default()
        },
        &none(),
    )
    .unwrap_err();
    assert!(err.message.contains("grid"), "{err:?}");
}
