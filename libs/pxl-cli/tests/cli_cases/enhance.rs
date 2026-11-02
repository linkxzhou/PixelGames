use crate::support::{json_line, pxl, repo_root};
use pxl_export::decode_png_rgba;
use std::fs;
use std::path::PathBuf;

const SPRITE: &str = "examples/retro_sprites_32/bomberman.ascii.json";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-enhance-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a png: {path:?}");
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

/// Count visible pixels with no same-colour 4-neighbour, including protected ones.
fn orphan_pixels(path: &std::path::Path) -> usize {
    let (w, h, rgba) = decode_png_rgba(&fs::read(path).unwrap()).unwrap();
    let mut count = 0;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = ((y * w as i32 + x) * 4) as usize;
            let color = &rgba[i..i + 4];
            if color[3] == 0 {
                continue;
            }
            let alone = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    return true;
                }
                let j = ((ny * w as i32 + nx) * 4) as usize;
                &rgba[j..j + 4] != color
            });
            if alone {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn enhance_compare_writes_a_sheet_with_one_entry_per_variant() {
    let dir = scratch("compare");
    let input = repo_root().join(SPRITE);
    let out = dir.join("sheet.png");
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "enhance",
        "compare",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
        "--scale",
        "3",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let v = json_line(&stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["cmd"], "enhance_compare");
    assert_eq!(
        v["background"], "checker",
        "compare defaults to the checker"
    );
    // epx and xbr only exist at 2x, so a 3x sheet skips them.
    let names: Vec<&str> = v["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["upscale"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"epx") && !names.contains(&"xbr"),
        "{names:?}"
    );
    assert!(
        names.contains(&"copy") && names.contains(&"hybrid"),
        "{names:?}"
    );
    for variant in v["variants"].as_array().unwrap() {
        assert!(variant["metrics"]["copy2x_bps"].is_number(), "{variant}");
    }
    let (w, h) = png_size(&out);
    assert_eq!(
        (w as u64, h as u64),
        (v["png_w"].as_u64().unwrap(), v["png_h"].as_u64().unwrap())
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn enhance_is_deterministic_and_scales_the_native_frame() {
    let dir = scratch("run");
    let input = repo_root().join(SPRITE);
    let mut hashes = Vec::new();
    for name in ["a.png", "b.png"] {
        let out = dir.join(name);
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "enhance",
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
        hashes.push(fs::read(&out).unwrap());
    }
    assert_eq!(hashes[0], hashes[1], "same input must give the same bytes");
    let (w, h) = png_size(&dir.join("a.png"));
    assert_eq!((w, h), (96, 96), "a 32px sprite at the default 3x");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn enhance_without_an_input_is_a_usage_error() {
    let (code, _, stderr) = pxl(&["enhance"]);
    assert_eq!(code, 2, "stderr={stderr}");
    assert!(stderr.contains("E_USAGE"), "{stderr}");
}

#[test]
fn enhance_does_not_overwrite_without_the_flag() {
    let dir = scratch("overwrite");
    let input = repo_root().join(SPRITE);
    let out = dir.join("x.png");
    fs::write(&out, b"keep me").unwrap();
    let (code, _, _) = pxl(&[
        "enhance",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_ne!(code, 0, "an existing output must be protected");
    assert_eq!(fs::read(&out).unwrap(), b"keep me");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scale3x_protects_small_features_and_leaves_no_orphans() {
    let dir = scratch("scale3x-protect");
    let input = repo_root().join(SPRITE);
    let mut results = Vec::new();
    for protect in ["auto", "none"] {
        let out = dir.join(format!("{protect}.png"));
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "enhance",
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--scale",
            "3",
            "--upscale",
            "scale3x",
            "--outline",
            "keep",
            "--cleanup",
            "none",
            "--asset",
            "--protect",
            protect,
        ]);
        assert_eq!(code, 0, "{protect}\nstdout={stdout}\nstderr={stderr}");
        results.push((json_line(&stdout), orphan_pixels(&out)));
    }

    let (auto, auto_orphans) = &results[0];
    assert_eq!(auto["report"]["protect"]["enabled"], true);
    assert!(auto["report"]["protect"]["small"].as_u64().unwrap() > 0);
    assert!(auto["report"]["protect"]["thin"].as_u64().unwrap() > 0);
    assert_eq!(auto["report"]["metrics"]["small_feature_kept_bps"], 10_000);
    assert_eq!(auto["report"]["metrics"]["lshape_count"], 0);
    assert_eq!(auto["report"]["metrics"]["orphan_count"], 0);
    assert_eq!(
        *auto_orphans, 0,
        "the PNG itself must have no isolated pixels"
    );

    let (off, _) = &results[1];
    assert_eq!(off["report"]["protect"]["enabled"], false);
    assert!(
        off["report"]["metrics"]["small_feature_kept_bps"]
            .as_u64()
            .unwrap()
            < 10_000
    );

    let (code, _, stderr) = pxl(&[
        "enhance",
        input.to_str().unwrap(),
        "-o",
        dir.join("invalid.png").to_str().unwrap(),
        "--protect",
        "sometimes",
    ]);
    assert_eq!(code, 2);
    assert!(stderr.contains("protect is auto or none"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn contour_symmetry_cleanup_is_applied_and_reported() {
    let dir = scratch("contour-symmetry");
    let input = repo_root().join(SPRITE);
    let mut reports = Vec::new();
    for cleanup in ["none", "symmetry"] {
        let out = dir.join(format!("{cleanup}.png"));
        let (code, stdout, stderr) = pxl(&[
            "--json",
            "enhance",
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--scale",
            "3",
            "--upscale",
            "contour",
            "--outline",
            "keep",
            "--shade",
            "none",
            "--cleanup",
            cleanup,
        ]);
        assert_eq!(code, 0, "{cleanup}\nstdout={stdout}\nstderr={stderr}");
        reports.push(json_line(&stdout));
    }
    let before = reports[0]["report"]["metrics"]["symmetry_err"]
        .as_u64()
        .unwrap();
    assert!(before > 0);
    assert_eq!(reports[1]["report"]["metrics"]["symmetry_err"], 0);
    assert!(reports[1]["report"]["clean"]["symmetry"].as_u64().unwrap() > 0);
    assert_eq!(reports[1]["report"]["clean"]["rhythm"], 0);
    assert_eq!(
        reports[1]["report"]["metrics"]["small_feature_kept_bps"],
        10_000
    );
    let _ = fs::remove_dir_all(&dir);
}
