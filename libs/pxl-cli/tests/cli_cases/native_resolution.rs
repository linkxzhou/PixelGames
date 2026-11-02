//! v3.0 native-resolution baseline: `hero_falcon` is a 24x24 sprite, the previews
//! only copy its pixels bigger, and the CLI says so.

use crate::support::{build_to, json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

fn load(path: &std::path::Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn hero_falcon_baseline_is_24x24_native() {
    let recipe = repo_root().join("examples/hero_falcon/hero_falcon.recipe.json");
    let out = build_to(&recipe, "hero-falcon-baseline");

    let doc = load(&out.join("hero_falcon.pxl.json"));
    assert_eq!((doc["w"].as_u64(), doc["h"].as_u64()), (Some(24), Some(24)));
    let frames = doc["frames"].as_object().unwrap();
    for name in ["base", "left", "right", "idle_left_0", "hurt_left_1"] {
        assert!(frames.contains_key(name), "missing frame {name}");
    }
    let base: Vec<&str> = doc["frames"]["base"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(base.len(), 24);
    assert!(base.iter().all(|r| r.chars().count() == 24));
    let opaque = base
        .iter()
        .flat_map(|r| r.chars())
        .filter(|c| *c != '.')
        .count();
    assert_eq!(opaque, 236, "the baseline sprite has 236 opaque pixels");

    // The sheet is 24x24 cells plus 1px padding, kept at native size (scale 1).
    let sheet = load(&out.join("hero_falcon.sheet.json"));
    assert_eq!(sheet["meta"]["scale"], "1");
    let size = &sheet["meta"]["size"];
    assert_eq!(
        (size["w"].as_u64(), size["h"].as_u64()),
        (Some(99), Some(149))
    );
    for frame in sheet["frames"].as_object().unwrap().values() {
        assert_eq!(frame["frame"]["w"], 24);
        assert_eq!(frame["frame"]["h"], 24);
    }

    // Previews are bigger copies of that sheet / frame, not new detail.
    let (w, h, _) =
        pxl_export::decode_png_rgba(&fs::read(out.join("hero_falcon.preview.png")).unwrap())
            .unwrap();
    assert_eq!((w, h), (99 * 4, 149 * 4));
    let (w, h, _) =
        pxl_export::decode_png_rgba(&fs::read(out.join("hero_falcon.sheet.png")).unwrap()).unwrap();
    assert_eq!((w, h), (99, 149));

    let _ = fs::remove_dir_all(&out);
}

/// `examples/hero_falcon/pxlgen.png` is the native `base` frame on the preview
/// backdrop, copied 16x16 per pixel. Pin that so "the image looks blurry/blocky"
/// is traceable to native resolution, not to the exporter.
#[test]
fn committed_pxlgen_png_is_the_native_base_frame_scaled_16x() {
    let root = repo_root().join("examples/hero_falcon");
    let png = root.join("pxlgen.png");
    if !png.exists() {
        return;
    }
    let (w, h, rgba) = pxl_export::decode_png_rgba(&fs::read(&png).unwrap()).unwrap();
    assert_eq!((w, h), (384, 384));
    let px = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    for y in 0..h {
        for x in 0..w {
            assert_eq!(
                px(x, y),
                px(x / 16 * 16, y / 16 * 16),
                "pxlgen.png has a non-uniform 16x16 block at {x},{y}"
            );
        }
    }
}

#[test]
fn render_json_reports_native_size_scale_and_purpose() {
    let recipe = repo_root().join("examples/hero_falcon/hero_falcon.recipe.json");
    let out = build_to(&recipe, "hero-falcon-render-meta");
    let doc = out.join("hero_falcon.pxl.json");
    let dest = out.join("meta.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--frame",
        "base",
        "--scale",
        "8",
        "-o",
        dest.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let v = json_line(&stdout);
    assert_eq!(
        (v["native_w"].as_u64(), v["native_h"].as_u64()),
        (Some(24), Some(24))
    );
    assert_eq!(v["scale"], 8);
    assert_eq!(
        (v["png_w"].as_u64(), v["png_h"].as_u64()),
        (Some(192), Some(192))
    );
    assert_eq!(v["sampling"], "nearest");
    assert_eq!(v["grid"], false);
    assert_eq!(v["purpose"], "preview");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn asset_render_is_transparent_and_conflicts_with_grid() {
    let recipe = repo_root().join("examples/hero_falcon/hero_falcon.recipe.json");
    let out = build_to(&recipe, "hero-falcon-asset");
    let doc = out.join("hero_falcon.pxl.json");
    let dest = out.join("asset.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--frame",
        "base",
        "--scale",
        "4",
        "--asset",
        "-o",
        dest.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert_eq!(json_line(&stdout)["purpose"], "asset");
    let (w, h, rgba) = pxl_export::decode_png_rgba(&fs::read(&dest).unwrap()).unwrap();
    assert_eq!((w, h), (96, 96));
    let transparent = rgba.chunks(4).filter(|p| p[3] == 0).count();
    assert_eq!(
        transparent,
        340 * 16,
        "340 empty native pixels stay transparent"
    );
    assert!(rgba.chunks(4).all(|p| p[3] == 0 || p[3] == 255));

    let (code, _, _) = pxl(&[
        "render",
        doc.to_str().unwrap(),
        "--scale",
        "4",
        "--asset",
        "--grid",
        "-o",
        out.join("both.png").to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "--asset with --grid is a usage error");
    let _ = fs::remove_dir_all(&out);
}

/// The detailed sample is drawn natively at 48x48 and then refined once to a
/// 96x96 grid, so it is neither a copy of the 24x24 art nor a 2x of a single
/// 48x48 pass: the refinement adds corner pixels the source does not have.
/// Both fill a 96x96 footprint at their own preview scale.
#[test]
fn hero_falcon_48_is_native_detail_not_a_2x_copy() {
    let root = repo_root().join("examples");
    let low = build_to(
        &root.join("hero_falcon/hero_falcon.recipe.json"),
        "hf-24-cmp",
    );
    let high = build_to(
        &root.join("hero_falcon_48/hero_falcon_48.recipe.json"),
        "hf-48-cmp",
    );
    let low_doc = load(&low.join("hero_falcon.pxl.json"));
    let high_doc = load(&high.join("hero_falcon_48.pxl.json"));
    assert_eq!(
        (high_doc["w"].as_u64(), high_doc["h"].as_u64()),
        (Some(96), Some(96))
    );

    let grid = |doc: &Value, frame: &str| -> Vec<Vec<char>> {
        doc["frames"][frame]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap().chars().collect())
            .collect()
    };
    let (a, b) = (grid(&low_doc, "base"), grid(&high_doc, "base"));
    let opaque = |g: &Vec<Vec<char>>| g.iter().flatten().filter(|c| **c != '.').count();
    assert_eq!(opaque(&a), 236);
    assert!(
        opaque(&b) > opaque(&a) * 3,
        "48x48 has {} opaque px vs {}",
        opaque(&b),
        opaque(&a)
    );
    // A plain 4x copy of the 24x24 sprite would equal the 96x96 frame exactly.
    let copy_mismatch = (0..96)
        .flat_map(|y| (0..96).map(move |x| (x, y)))
        .filter(|&(x, y)| a[y / 4][x / 4] != b[y][x])
        .count();
    assert!(
        copy_mismatch > 400,
        "only {copy_mismatch} px differ from a 4x copy"
    );
    // Distinct palette indices in use: shaded detail, not just a bigger outline.
    let colours = |g: &Vec<Vec<char>>| {
        let mut s: Vec<char> = g.iter().flatten().filter(|c| **c != '.').copied().collect();
        s.sort();
        s.dedup();
        s.len()
    };
    // Same palette, no new colours: the extra detail comes from pixel budget.
    assert!(colours(&b) >= 14 && colours(&b) <= colours(&a) + 1);

    // Same 96x96 display footprint: 24px at 4x, 48px at 2x. No grid in the 48 preview.
    let sheet = load(&high.join("hero_falcon_48.sheet.json"));
    for frame in sheet["frames"].as_object().unwrap().values() {
        assert_eq!(
            (frame["frame"]["w"].as_u64(), frame["frame"]["h"].as_u64()),
            (Some(96), Some(96))
        );
    }
    let build: Value = load(&high.join("hero_falcon_48.build.json"));
    assert_eq!(build["qa"]["ok"], true, "no error-level QA failures");
    // GIF logical screen size is bytes 6..10 of the header (little-endian u16 x2).
    let gif = fs::read(high.join("hero_falcon_48.walk.gif")).unwrap();
    assert!(gif.starts_with(b"GIF89a"));
    let gw = u16::from_le_bytes([gif[6], gif[7]]);
    let gh = u16::from_le_bytes([gif[8], gif[9]]);
    assert_eq!((gw, gh), (96, 96));

    let _ = fs::remove_dir_all(&low);
    let _ = fs::remove_dir_all(&high);
}

/// v3.0 M2/M4: the new primitives and material/detail ops work through the same
/// CLI path recipes use, on a 96x96 canvas that the old 64 limit rejected.
#[test]
fn new_primitives_and_ops_build_on_a_96px_canvas() {
    let dir = std::env::temp_dir().join(format!("pxl-m2m4-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let shapes = r#"{
  "w": 96, "h": 96,
  "draw": [
    {"ellipse": [24, 30, 48, 44], "c": 1},
    {"ellipse": [26, 32, 44, 40], "c": 11},
    {"ellipse": [30, 36, 30, 28], "c": 12},
    {"polygon": [[68, 48], [88, 54], [70, 60]], "c": 15},
    {"polygon": [[26, 60], [8, 84], [22, 78], [34, 70]], "c": 12},
    {"line": [40, 76, 38, 90], "width": 3, "c": 16},
    {"line": [54, 76, 56, 90], "width": 3, "c": 16}
  ]
}"#;
    let recipe = r#"{
  "v": 1, "asset": "m2m4", "seed": 3, "palette": "palette.json",
  "sprites": [{
    "id": "m2m4", "kind": "sprite",
    "base": {"gen": "shapes", "spec": "m2m4.shapes.json"},
    "post": [
      {"op": "material", "from": "steel", "region": [26, 32, 44, 40], "steps": 0},
      {"op": "detail", "ramp": "steel", "color": 13, "region": [26, 32, 44, 40], "density": 800, "inset": 2},
      {"op": "shade", "dir": "top_left", "amount": 1, "ramp": "steel"}
    ],
    "directions": [{"id": "left", "from": "base", "ops": [{"op": "noop"}]}],
    "anims": [{"name": "idle", "fps": 3, "dirs": ["left"],
      "frames": [[{"op": "noop"}], [{"op": "shift", "dx": 0, "dy": 1, "region": [0, 0, 96, 70]}]]}]
  }],
  "sheet": {"sources": ["m2m4"], "file": "m2m4.sheet.png"},
  "preview": [{"sheet": true, "scale": 2, "file": "m2m4.preview.png"}]
}"#;
    fs::write(dir.join("m2m4.shapes.json"), shapes).unwrap();
    fs::write(dir.join("m2m4.recipe.json"), recipe).unwrap();
    fs::copy(
        repo_root().join("examples/hero_falcon/palette.json"),
        dir.join("palette.json"),
    )
    .unwrap();
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        dir.join("m2m4.recipe.json").to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let built = json_line(&stdout);
    assert_eq!(built["ok"], true);
    let doc = load(&dir.join("out/m2m4.pxl.json"));
    assert_eq!((doc["w"].as_u64(), doc["h"].as_u64()), (Some(96), Some(96)));
    // The ellipse is round: its widest row and its height both come from the box.
    let base: Vec<&str> = doc["frames"]["base"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    let opaque_rows = base.iter().filter(|r| r.contains(|c| c != '.')).count();
    assert!(
        opaque_rows > 40,
        "the body spans the 30..74 rows, got {opaque_rows}"
    );
    let widest = base
        .iter()
        .map(|r| r.matches(|c| c != '.').count())
        .max()
        .unwrap();
    assert!(
        widest >= 40,
        "the 48 wide ellipse fills its box, got {widest}"
    );
    let _ = fs::remove_dir_all(&dir);
}
