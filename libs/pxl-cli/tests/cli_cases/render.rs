use crate::support::{build_to, json_line, pxl, repo_root};
use std::fs;

/// A png cannot become a gif: refusing beats writing png bytes under a .gif name.
#[test]
fn render_refuses_a_gif_name_for_a_png_input() {
    let root = repo_root();
    let recipe = root.join("examples/static_bullet/static_bullet.recipe.json");
    let out = build_to(&recipe, "gifname");
    let png = out.join("static_bullet.sheet.png");
    let gif = out.join("preview.gif");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        png.to_str().unwrap(),
        "-o",
        gif.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "png->gif must be a usage error: {stdout}");
    assert!(
        !gif.exists(),
        "nothing may be written under the wrong format"
    );
    let _ = fs::remove_dir_all(&out);
}

/// Read the NETSCAPE loop block: present means the GIF repeats.
fn gif_repeats(bytes: &[u8]) -> bool {
    bytes.windows(11).any(|w| w == b"NETSCAPE2.0")
}

#[test]
fn one_shot_animation_previews_as_a_non_looping_gif() {
    let dir = std::env::temp_dir().join(format!("pxl-gifloop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "gifloop",
    );
    let doc = out.join("farmer.pxl.json");
    let looping = out.join("walk.gif");
    let once = out.join("hoe.gif");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "walk",
        "--dir",
        "down",
        "-o",
        looping.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "hoe",
        "--dir",
        "down",
        "-o",
        once.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert!(gif_repeats(&fs::read(&looping).unwrap()), "walk loops");
    assert!(!gif_repeats(&fs::read(&once).unwrap()), "hoe plays once");
    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn multi_frame_png_reports_the_real_strip_size() {
    let out = build_to(
        &repo_root().join("examples/farmer/farmer.recipe.json"),
        "strip",
    );
    let doc = out.join("farmer.pxl.json");
    let strip = out.join("strip.png");
    let (code, stdout, _) = pxl(&[
        "--json",
        "render",
        doc.to_str().unwrap(),
        "--anim",
        "walk",
        "--dir",
        "down",
        "--scale",
        "2",
        "-o",
        strip.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let v = json_line(&stdout);
    let bytes = fs::read(&strip).unwrap();
    let (w, h, _) = pxl_export::decode_png_rgba(&bytes).unwrap();
    assert_eq!(v["png_w"].as_u64().unwrap(), w as u64);
    assert_eq!(v["png_h"].as_u64().unwrap(), h as u64);
    assert!(
        w > 16 * 2,
        "a strip of several frames is wider than one frame"
    );
    let _ = fs::remove_dir_all(&out);
}
