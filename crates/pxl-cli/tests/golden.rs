//! Frozen hashes of frames and encoded images. Same recipe, seed, and version
//! must keep these byte-identical.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use pxl_core::{sha256_hex, IndexFrame, Palette};
use pxl_export::{encode_gif, encode_indexed_png};
use pxl_format::Op;
use pxl_gen::{
    generate_bollinger, generate_noise_tile, generate_shapes, load_builtin_mask, variant_seed,
};
use pxl_ops::{apply_op, apply_ops, expand_macro, ApplyCx};

fn tank_palette() -> Palette {
    pxl_gen::builtin_palette("tank").unwrap()
}

fn p16_palette() -> Palette {
    pxl_gen::builtin_palette("p16").unwrap()
}

fn hash_frame(frame: &IndexFrame) -> String {
    sha256_hex(&frame.hash_bytes())
}

fn cx<'a>(pal: &'a Palette) -> ApplyCx<'a> {
    ApplyCx {
        palette: pal,
        seed: 1,
        frame_name: "golden",
        load_template: Some(&load_builtin_mask),
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    }
}

fn bollinger(pal: &Palette, id: &str, seed: u64, ramp: Option<&str>) -> IndexFrame {
    let mask = load_builtin_mask(id).unwrap();
    generate_bollinger(&mask, seed, pal, ramp).unwrap()
}

fn apply(frame: &IndexFrame, op: Op, pal: &Palette) -> IndexFrame {
    apply_op(frame, &op, &cx(pal), 0).unwrap()
}

fn compute() -> BTreeMap<String, String> {
    let p16 = p16_palette();
    let pal = tank_palette();
    let mut out = BTreeMap::new();
    let mut push = |name: &str, frame: &IndexFrame| {
        out.insert(name.to_string(), hash_frame(frame));
    };

    for (id, seed) in [
        ("slime_8x8", 1u64),
        ("slime_8x8", 2),
        ("slime_8x8", 42),
        ("ship_8x12", 7),
        ("robot_6x10", 3),
        ("bug_8x8", 4),
        ("gem_6x6", 5),
        ("mushroom_8x8", 8),
        ("tree_8x12", 9),
        ("rock_6x6", 11),
        ("dragon_side_8x10", 13),
        ("dragon_top_8x8", 15),
        ("panel_4x8", 42),
    ] {
        push(
            &format!("bollinger.{id}.{seed}"),
            &bollinger(&p16, id, seed, None),
        );
    }
    push(
        "bollinger.panel_4x8.42.olive",
        &bollinger(&pal, "panel_4x8", 42, Some("olive")),
    );

    for (kind, seed) in [("dirt", 7u64), ("dirt", 8), ("grass", 7), ("sand", 3)] {
        let tile = generate_noise_tile(kind, 16, seed, &pal, Some("ground")).unwrap();
        push(&format!("tile.{kind}.{seed}"), &tile);
    }
    let varied = generate_noise_tile("dirt", 16, variant_seed(7, 3), &pal, Some("ground")).unwrap();
    push("tile.dirt.variant3", &varied);

    let shapes =
        pxl_format::parse_shapes(include_str!("../../../examples/tank/tank_body.shapes.json"))
            .unwrap();
    let body = generate_shapes(&shapes, &pal).unwrap();
    push("shapes.tank_body", &body);
    let mirrored = pxl_format::parse_shapes(
        r#"{"w":8,"h":4,"mirror":"x","draw":[{"rect":[1,0,2,4],"c":3},{"rect":[1,0,2,4],"c":6,"pattern":"stripe_y:2"}]}"#,
    )
    .unwrap();
    push("shapes.stripe", &generate_shapes(&mirrored, &pal).unwrap());

    let base = bollinger(&p16, "slime_8x8", 1, None);
    push(
        "op.flip_x",
        &apply(&base, Op::Flip { axis: "x".into() }, &pal),
    );
    push(
        "op.flip_y",
        &apply(&base, Op::Flip { axis: "y".into() }, &pal),
    );
    push(
        "op.rotate_cw",
        &apply(&base, Op::Rotate90 { dir: "cw".into() }, &pal),
    );
    push(
        "op.rotate_ccw",
        &apply(&base, Op::Rotate90 { dir: "ccw".into() }, &pal),
    );
    push(
        "op.rotate_180",
        &apply(&base, Op::Rotate90 { dir: "180".into() }, &pal),
    );
    push(
        "op.shift",
        &apply(
            &base,
            Op::Shift {
                dx: 1,
                dy: -1,
                wrap: false,
                region: None,
            },
            &pal,
        ),
    );
    push(
        "op.palette_cycle",
        &apply(
            &body,
            Op::PaletteCycle {
                cycle: "tread".into(),
                step: 1,
                region: None,
            },
            &pal,
        ),
    );
    push(
        "op.outline",
        &apply(
            &body,
            Op::Outline {
                color: pxl_format::ColorRef::Index(1),
                mode: "outer".into(),
                brush: "square".into(),
                width: 1,
            },
            &pal,
        ),
    );
    push(
        "op.shade",
        &apply(
            &body,
            Op::Shade {
                dir: "top_left".into(),
                amount: 1,
                region: None,
            },
            &pal,
        ),
    );
    push(
        "op.dissolve",
        &apply(
            &body,
            Op::Dissolve {
                seed: 9,
                steps: 4,
                step: 2,
                pattern: "bayer4".into(),
            },
            &pal,
        ),
    );
    push(
        "op.row_shift",
        &apply(
            &body,
            Op::RowShift {
                rows: Some([4, 10]),
                dx: 1,
                dy: 0,
                pattern: Some("sine4".into()),
                phase: 1,
                region: None,
                axis: None,
                wrap: Some(false),
            },
            &pal,
        ),
    );
    push(
        "op.shear",
        &apply(
            &body,
            Op::Shear {
                axis: "x".into(),
                k: 2,
                pivot: 8,
                region: None,
            },
            &pal,
        ),
    );
    push(
        "op.texture",
        &apply(
            &body,
            Op::Texture {
                gen: "bollinger".into(),
                template: "panel_4x8".into(),
                region: pxl_format::Region::Rect([4, 7, 8, 4]),
                ramp: Some("olive".into()),
                seed: Some(42),
            },
            &pal,
        ),
    );

    let context = cx(&pal);
    for dir in ["N", "E"] {
        let lists = expand_macro(
            "tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)",
            Some(dir),
            &body,
            &pal,
            None,
        )
        .unwrap();
        for (i, ops) in lists.iter().enumerate() {
            let frame = apply_ops(&body, ops, &context).unwrap();
            push(&format!("macro.tread.{dir}.{i}"), &frame);
        }
        let idle =
            expand_macro("idle_rumble(steps=2, amp=1)", Some(dir), &body, &pal, None).unwrap();
        for (i, ops) in idle.iter().enumerate() {
            let frame = apply_ops(&body, ops, &context).unwrap();
            push(&format!("macro.idle.{dir}.{i}"), &frame);
        }
    }

    let farmer_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/farmer");
    let farmer_pal = pxl_format::parse_palette(
        &fs::read_to_string(farmer_dir.join("farmer.palette.json")).unwrap(),
        "farmer",
    )
    .unwrap();
    let rig =
        pxl_format::parse_parts(&fs::read_to_string(farmer_dir.join("farmer.parts.json")).unwrap())
            .unwrap();
    let blank = IndexFrame::new(rig.w, rig.h).unwrap();
    let mut farmer_cx = cx(&farmer_pal);
    farmer_cx.parts = Some(&rig);
    let outline = Op::Outline {
        color: pxl_format::ColorRef::Name("outline".into()),
        mode: "outer".into(),
        brush: "square".into(),
        width: 1,
    };
    let down = apply_ops(
        &blank,
        &[
            Op::Compose {
                parts: vec![],
                dir: Some("down".into()),
            },
            outline.clone(),
        ],
        &farmer_cx,
    )
    .unwrap();
    push("farmer.down", &down);
    let walk = expand_macro(
        "doll(anim=walk)",
        Some("down"),
        &blank,
        &farmer_pal,
        Some(&rig),
    )
    .unwrap();
    for (i, ops) in walk.iter().enumerate() {
        let mut ops = ops.clone();
        ops.push(outline.clone());
        push(
            &format!("farmer.walk.down.{i}"),
            &apply_ops(&blank, &ops, &farmer_cx).unwrap(),
        );
    }
    let hoe = expand_macro(
        "doll(anim=hoe)",
        Some("down"),
        &blank,
        &farmer_pal,
        Some(&rig),
    )
    .unwrap();
    let mut planted = hoe[2].clone();
    planted.push(outline.clone());
    push(
        "farmer.hoe.down.2",
        &apply_ops(&blank, &planted, &farmer_cx).unwrap(),
    );
    push(
        "farmer.swap.blonde",
        &apply(
            &down,
            Op::PaletteSwap {
                map: "ramp:hair->blonde,ramp:shirt->red".into(),
                region: None,
            },
            &farmer_pal,
        ),
    );

    let png = encode_indexed_png(&body, &pal, 2).unwrap();
    out.insert("png.tank_body".into(), sha256_hex(&png));
    let gif = encode_gif(
        &[
            body.clone(),
            apply(&body, Op::Flip { axis: "x".into() }, &pal),
        ],
        &pal,
        2,
        10,
    )
    .unwrap();
    out.insert("gif.flip".into(), sha256_hex(&gif));
    out
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hashes.json")
}

#[test]
fn golden_hashes_match() {
    let got = compute();
    assert!(
        got.len() >= 30,
        "need at least 30 golden cases, got {}",
        got.len()
    );
    let path = golden_path();
    if std::env::var("UPDATE_GOLDEN").ok().as_deref() == Some("1") {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut text = serde_json::to_string_pretty(&got).unwrap();
        text.push('\n');
        fs::write(&path, text).unwrap();
        return;
    }
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: {e}. Run UPDATE_GOLDEN=1 cargo test -p pxl-cli --test golden",
            path.display()
        )
    });
    let expected: BTreeMap<String, String> = serde_json::from_str(&text).unwrap();
    assert_eq!(expected.len(), got.len(), "golden case count changed");
    for (key, value) in &got {
        assert_eq!(
            expected.get(key).map(String::as_str),
            Some(value.as_str()),
            "hash drift for {key}"
        );
    }
}
