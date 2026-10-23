use std::cell::RefCell;
use std::collections::BTreeMap;

use pxl_core::{IndexFrame, Palette, PxlResult};
use pxl_format::Op;

use super::args::check_args;
use super::catalog::{allowed_args, macro_catalog, macro_info, MACRO_NAMES};
use super::expand_macro;
use crate::{apply_ops, ApplyCx};

fn pal() -> Palette {
    pxl_gen::builtin_palette("tank").unwrap()
}

fn grid(rows: &[&str]) -> IndexFrame {
    let owned: Vec<String> = rows.iter().map(|s| (*s).to_string()).collect();
    pxl_core::decode_matrix(&owned).unwrap()
}

fn run(frame: &IndexFrame, ops: &[Op], pal: &Palette) -> IndexFrame {
    let cx = ApplyCx {
        palette: pal,
        seed: 1,
        frame_name: "f",
        load_template: None,
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    };
    apply_ops(frame, ops, &cx).unwrap()
}

/// Metadata must describe exactly the arguments the validator accepts, and
/// every documented default must actually be accepted by `check_args`.
/// A rig and a frame every macro can expand against, so the catalogue can be
/// checked against real expansion instead of against itself.
fn catalog_fixture() -> (IndexFrame, Palette) {
    let pal = pal();
    let mut f = IndexFrame::new(16, 16).unwrap();
    for y in 1..15 {
        for x in 1..15 {
            f.set(x, y, 3);
        }
    }
    // Tread pixels on the outer columns for N/S and outer rows for E/W.
    let tread = pal.ramps["tread"]
        .iter()
        .copied()
        .find(|c| *c != pal.special.outline);
    let tread = tread.expect("tank palette has a tread colour");
    for y in 2..14 {
        for x in [1, 2, 13, 14] {
            f.set(x, y, tread);
        }
    }
    for x in 2..14 {
        for y in [1, 2, 13, 14] {
            f.set(x, y, tread);
        }
    }
    (f, pal)
}

/// Per-macro arguments the catalogue cannot default (an eye rect, a doll rig).
fn required_extras(name: &str) -> &'static str {
    match name {
        "blink" => "x=5, y=6, w=4, h=2",
        _ => "",
    }
}

fn call(name: &str, args: &[(String, String)], extra: &str) -> String {
    let mut parts: Vec<String> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
    if !extra.is_empty() {
        parts.push(extra.to_string());
    }
    format!("{name}({})", parts.join(", "))
}

/// Omitting an argument must behave exactly like writing its catalogued
/// default, and the catalogued frame count must be what is really produced.
#[test]
fn catalogue_defaults_and_frame_counts_match_real_expansion() {
    let (f, pal) = catalog_fixture();
    for info in macro_catalog() {
        if info.name == "doll" {
            continue; // needs a parts rig; covered by the farmer tests
        }
        let extra = required_extras(info.name);
        let bare = if extra.is_empty() {
            info.name.to_string()
        } else {
            format!("{}({extra})", info.name)
        };
        let explicit: Vec<(String, String)> = info
            .args
            .iter()
            .filter(|a| !extra.contains(&format!("{}=", a.name)))
            .filter_map(|a| a.default.map(|d| (a.name.to_string(), d.to_string())))
            .collect();
        let spelled = call(info.name, &explicit, extra);
        let a =
            expand_macro(&bare, None, &f, &pal, None).unwrap_or_else(|e| panic!("{bare}: {e:?}"));
        let b = expand_macro(&spelled, None, &f, &pal, None)
            .unwrap_or_else(|e| panic!("{spelled}: {e:?}"));
        assert_eq!(
            format!("{a:?}"),
            format!("{b:?}"),
            "{}: the catalogued defaults differ from the real ones",
            info.name
        );
        assert_eq!(
            a.len() as u32,
            info.frames,
            "{}: catalogue says {} frames, expansion gives {}",
            info.name,
            info.frames,
            a.len()
        );
    }
}

/// Every example the catalogue prints must run as written.
#[test]
fn catalogue_examples_expand() {
    let (f, pal) = catalog_fixture();
    for info in macro_catalog() {
        if info.name == "doll" {
            continue;
        }
        let result = expand_macro(&info.example, None, &f, &pal, None);
        assert!(result.is_ok(), "{}: {:?}", info.example, result.err());
    }
}

/// `bob` sways in four frames (rest, up, rest, down) whatever `amp` is.
#[test]
fn bob_emits_the_catalogued_four_frames() {
    let (f, _) = catalog_fixture();
    assert_eq!(expand("bob", &f).unwrap().len(), 4);
    assert_eq!(expand("bob(amp=3)", &f).unwrap().len(), 4);
}

#[test]
fn macro_metadata_matches_validator_signatures() {
    for name in MACRO_NAMES {
        let info = macro_info(name).expect("every macro has metadata");
        let allowed = allowed_args(name);
        assert_eq!(info.args.len(), allowed.len(), "{name} arg count");
        for arg in &info.args {
            assert!(
                allowed.contains(&arg.name),
                "{name}: {0} not allowed",
                arg.name
            );
            // A documented default must be a real accepted argument.
            let mut map = BTreeMap::new();
            map.insert(arg.name.to_string(), arg.default.unwrap_or("1").to_string());
            assert!(
                check_args(name, &map).is_ok(),
                "{name}: default for {0} rejected",
                arg.name
            );
        }
        // Unknown arguments must still be rejected (no silent drift).
        let mut bad = BTreeMap::new();
        bad.insert("steps".to_string(), "2".to_string());
        if !allowed.contains(&"steps") {
            assert!(
                check_args(name, &bad).is_err(),
                "{name} wrongly accepts steps"
            );
        }
        assert!(!info.example.is_empty(), "{name} needs an example");
    }
}

/// `pulse_glow` has no `steps` argument: the historical `pulse_glow(steps=2)`
/// call must stay a schema error with an actionable hint.
#[test]
fn pulse_glow_rejects_steps_and_lists_real_signature() {
    let mut args = BTreeMap::new();
    args.insert("steps".into(), "2".into());
    let err = check_args("pulse_glow", &args).unwrap_err();
    assert!(err.message.contains("steps"), "{err:?}");
    assert!(
        err.hint.as_deref().unwrap_or("").contains("ramp"),
        "{err:?}"
    );
    let info = macro_info("pulse_glow").unwrap();
    assert_eq!(info.args.len(), 1);
    assert_eq!(info.args[0].name, "ramp");
    assert_eq!(info.args[0].default, Some("olive"));
    assert!(info.example.contains("ramp="), "{}", info.example);
    // It emits two frames without any frame-count argument.
    assert_eq!(info.frames, 2);
}

fn expand(spec: &str, frame: &IndexFrame) -> PxlResult<Vec<Vec<Op>>> {
    expand_macro(spec, None, frame, &pal(), None)
}

#[test]
fn unknown_macro_argument_is_an_error_not_a_silent_default() {
    let f = grid(&["22", "22"]);
    let err = expand("idle_rumble(step=2)", &f).unwrap_err();
    assert!(err.message.contains("step"), "{err:?}");
    let err = expand("hurt_flash(amount=3)", &f).unwrap_err();
    assert!(err.message.contains("amount"), "{err:?}");
    // The documented spellings keep working.
    assert!(expand("idle_rumble(steps=2, amp=1)", &f).is_ok());
    assert!(expand("idle_breathe(period=4)", &f).is_ok());
}

#[test]
fn idle_breathe_honours_period() {
    let f = grid(&["22", "22"]);
    assert_eq!(expand("idle_breathe(period=3)", &f).unwrap().len(), 3);
    assert_eq!(expand("idle_breathe(steps=5)", &f).unwrap().len(), 5);
    assert!(expand("idle_breathe(period=0)", &f).is_err());
}

#[test]
fn death_dissolve_starts_whole_and_ends_empty() {
    let pal = pal();
    let f = grid(&["2222", "2222", "2222", "2222"]);
    for steps in [2, 6, 16] {
        let frames = expand(&format!("death_dissolve(steps={steps})"), &f).unwrap();
        assert_eq!(frames.len(), steps);
        let first = run(&f, &frames[0], &pal);
        let last = run(&f, frames.last().unwrap(), &pal);
        assert_eq!(
            first.pixels, f.pixels,
            "steps={steps}: first frame is the sprite"
        );
        assert_eq!(last.count_opaque(), 0, "steps={steps}: last frame is empty");
    }
    assert!(expand("death_dissolve(steps=1)", &f).is_err());
}

#[test]
fn blink_closes_only_the_named_eye_rect() {
    let pal = pal();
    // Skin 3 around two eye pixels coloured 11.
    let f = grid(&["3333", "3B3B", "3333"]);
    let frames = expand("blink(x=0, y=1, w=4, h=1)", &f).unwrap();
    assert_eq!(frames.len(), 2);
    let open = run(&f, &frames[0], &pal);
    let shut = run(&f, &frames[1], &pal);
    assert_eq!(open.pixels, f.pixels);
    assert_ne!(shut.pixels, f.pixels, "closed eyes must differ");
    // Rows outside the rect never change.
    for x in 0..4 {
        assert_eq!(shut.get(x, 0), f.get(x, 0));
        assert_eq!(shut.get(x, 2), f.get(x, 2));
    }
    // The lid takes the surrounding skin colour.
    assert!((0..4).all(|x| shut.get(x, 1) == 3));
}

#[test]
fn blink_needs_a_rect_inside_the_frame() {
    let f = grid(&["33", "33"]);
    assert!(expand("blink", &f).is_err());
    assert!(expand("blink(x=0, y=0, w=9, h=1)", &f).is_err());
}

/// On a non-square sprite every cw turn crops back to the canvas, so chaining
/// turns loses pixels. Each pose must come from the source in one step: the
/// 180 pose is the direct 180, and the 270 pose is a single ccw.
#[test]
fn spin_projectile_poses_are_single_transforms_of_the_source() {
    let pal = pal();
    let f = grid(&["1234", "5678"]);
    let frames = expand("spin_projectile(steps=4)", &f).unwrap();
    let poses: Vec<_> = frames.iter().map(|o| run(&f, o, &pal)).collect();
    assert_eq!(poses[0].pixels, f.pixels, "pose 0 is the source");
    assert_eq!(
        poses[1].pixels,
        crate::apply::rotate90(&f, "cw").unwrap().pixels
    );
    assert_eq!(
        poses[2].pixels,
        crate::apply::rotate90(&f, "180").unwrap().pixels,
        "180 degrees must not be two cropped quarter turns"
    );
    assert_eq!(
        poses[3].pixels,
        crate::apply::rotate90(&f, "ccw").unwrap().pixels
    );
    // 8765/4321 is the direct 180 of 1234/5678 (hand-traced).
    assert_eq!(
        pxl_core::encode_matrix(&poses[2]).unwrap(),
        vec!["8765".to_string(), "4321".to_string()]
    );
}

#[test]
fn spin_projectile_only_offers_four_distinct_poses() {
    let pal = pal();
    let f = grid(&["2222", "2...", "2...", "...."]);
    let frames = expand("spin_projectile(steps=4)", &f).unwrap();
    let poses: Vec<_> = frames.iter().map(|o| run(&f, o, &pal).pixels).collect();
    for i in 0..poses.len() {
        for j in i + 1..poses.len() {
            assert_ne!(poses[i], poses[j], "poses {i} and {j} repeat");
        }
    }
    assert!(expand("spin_projectile(steps=8)", &f).is_err());
}
