use pxl_core::{Palette, Rgba};
use pxl_svg::{rasterize, ColorMode, SvgOptions};

fn pal() -> Palette {
    let mut p = Palette {
        id: "t".into(),
        colors: vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(24, 20, 30),
            Rgba::rgb(200, 90, 60),
            Rgba::rgb(90, 160, 80),
            Rgba::rgb(240, 220, 190),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    p.special.outline = 1;
    p.special.flash = 1;
    p
}

fn svg(body: &str) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">{body}</svg>"##)
}

fn size(px: u16) -> SvgOptions {
    SvgOptions {
        w: px,
        h: px,
        samples: 4,
        ..Default::default()
    }
}

#[test]
fn an_integer_rect_lands_exactly_on_the_pixel_grid() {
    let text = svg(r##"<rect x="4" y="4" width="8" height="8" fill="#c85a3c"/>"##);
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    assert_eq!((f.w, f.h), (16, 16));
    for y in 0..16 {
        for x in 0..16 {
            let inside = (4..12).contains(&x) && (4..12).contains(&y);
            assert_eq!(f.get(x, y) != 0, inside, "{x},{y}");
        }
    }
    assert_eq!(f.get(8, 8), 2);
}

#[test]
fn the_viewbox_scales_to_the_target_and_keeps_circles_round() {
    // A 32-unit viewBox at 16 px halves every coordinate.
    let text = r##"<svg viewBox="0 0 32 32"><circle cx="16" cy="16" r="8" fill="#5aa050"/></svg>"##;
    let out = rasterize(text, &size(16), &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    let (mut minx, mut maxx, mut miny, mut maxy) = (16, -1, 16, -1);
    for y in 0..16 {
        for x in 0..16 {
            if f.get(x, y) != 0 {
                minx = minx.min(x);
                maxx = maxx.max(x);
                miny = miny.min(y);
                maxy = maxy.max(y);
            }
        }
    }
    // r = 4 in the output: eight pixels across, symmetric.
    assert_eq!(
        (maxx - minx, maxy - miny),
        (7, 7),
        "{minx} {maxx} {miny} {maxy}"
    );
    assert_eq!(
        (minx, maxx),
        (16 - maxx - 1, maxx),
        "horizontally symmetric"
    );
}

#[test]
fn a_one_pixel_line_is_continuous_and_one_pixel_wide() {
    let text = svg(r##"<line x1="2" y1="2" x2="13" y2="13" stroke="#000000" stroke-width="1"/>"##);
    let mut o = size(16);
    o.color_mode = ColorMode::Nearest;
    let out = rasterize(&text, &o, &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    // 8-connected from start to end: the diagonal never breaks.
    let mut prev = (2i32, 2i32);
    for i in 3..13i32 {
        let hit = (i - 1..=i + 1)
            .flat_map(|y| (i - 1..=i + 1).map(move |x| (x, y)))
            .find(|(x, y)| f.get(*x, *y) != 0)
            .unwrap_or_else(|| panic!("diagonal broken at {i}"));
        assert!((hit.0 - prev.0).abs() <= 2, "{hit:?} after {prev:?}");
        prev = hit;
    }
    // No pixel wider than one: a naive 4x4 average would smear the stroke.
    let row: usize = (0..16).filter(|x| f.get(*x, 8) != 0).count();
    assert!(row <= 2, "{row} pixels across a 1 px line");
}

#[test]
fn unsupported_elements_warn_and_strict_refuses_them() {
    let text = svg(
        r##"<text x="1" y="8">hi</text><rect x="0" y="0" width="4" height="4" fill="#18141e"/>"##,
    );
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    assert_eq!(out.report.unsupported.len(), 1);
    assert_eq!(out.frames.len(), 1);
    let mut strict = size(16);
    strict.strict = true;
    let err = rasterize(&text, &strict, &pal()).unwrap_err();
    assert!(err.message.contains("svg.strict"), "{}", err.message);
}

#[test]
fn a_colour_outside_the_palette_is_refused_in_strict_mode() {
    let text = svg(r##"<rect x="0" y="0" width="4" height="4" fill="#ff00ff"/>"##);
    let err = rasterize(&text, &size(16), &pal()).unwrap_err();
    assert!(
        err.message.contains("svg.color_not_in_palette"),
        "{}",
        err.message
    );
    let mut nearest = size(16);
    nearest.color_mode = ColorMode::Nearest;
    let out = rasterize(&text, &nearest, &pal()).unwrap();
    assert!(out.report.colors.snapped > 0);
}

#[test]
fn frames_come_from_data_frame_groups() {
    let text = svg(
        r##"<g data-frame="idle_0"><rect x="2" y="2" width="4" height="4" fill="#c85a3c"/></g>
            <g data-frame="idle_1"><rect x="2" y="4" width="4" height="4" fill="#5aa050"/></g>"##,
    );
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    let names: Vec<&str> = out.frames.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["idle_0", "idle_1"]);
    let a = &out.frames[0].1.frame;
    let b = &out.frames[1].1.frame;
    assert_eq!(a.get(3, 3), 2);
    assert_eq!(b.get(3, 6), 3);
    assert_eq!(a.get(3, 6), 0, "each frame draws only its own group");
}

#[test]
fn use_shares_a_part_between_frames() {
    let text = svg(
        r##"<defs><g id="eye"><rect x="0" y="0" width="1" height="1" fill="#f0dcbe"/></g></defs>
            <g data-frame="a"><use href="#eye" x="3" y="3"/></g>
            <g data-frame="b"><use href="#eye" x="3" y="5"/></g>"##,
    );
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    assert_eq!(out.frames[0].1.frame.get(3, 3), 4);
    assert_eq!(out.frames[1].1.frame.get(3, 5), 4);
}

#[test]
fn data_min_keeps_a_detail_that_the_grid_would_drop() {
    // A quarter-pixel eye disappears at 16 px around 4x4 samples; data-min
    // forces the best-covered pixel to keep it.
    let text = svg(
        r##"<rect x="2" y="2" width="12" height="12" fill="#c85a3c"/>
            <rect data-role="eye" data-min="1" x="7.9" y="7.9" width="0.2" height="0.2" fill="#f0dcbe"/>"##,
    );
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    eprintln!(
        "{:?} {:?} {:?}",
        out.report,
        f.pixels
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == 4)
            .collect::<Vec<_>>(),
        f.get(8, 8)
    );
    assert!(f.pixels.iter().any(|p| *p == 4), "the eye survived");
    assert!(out.report.forced >= 1, "{:?}", out.report);
}

#[test]
fn data_sizes_draws_a_detail_only_at_the_sizes_it_names() {
    let body = r##"<rect x="2" y="2" width="12" height="12" fill="#c85a3c"/>
                 <rect data-sizes="32" x="4" y="4" width="2" height="2" fill="#5aa050"/>"##;
    let text = svg(body);
    let small = rasterize(&text, &size(16), &pal()).unwrap();
    assert_eq!(small.report.size_filtered, 1);
    assert_eq!(small.frames[0].1.frame.get(4, 4), 2);
    let mut big = size(32);
    big.w = 32;
    big.h = 32;
    let large = rasterize(&text, &big, &pal()).unwrap();
    assert_eq!(large.frames[0].1.frame.get(8, 8), 3);
}

#[test]
fn a_gradient_becomes_a_hard_ramp_inside_the_palette() {
    let text = svg(
        r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#181420"/>
                <stop offset="1" stop-color="#f0dcbe"/></linearGradient></defs>
            <rect x="0" y="0" width="16" height="16" fill="url(#g)"/>"##,
    );
    let mut o = size(16);
    o.color_mode = ColorMode::Nearest;
    o.gradient_bands = 4;
    let out = rasterize(&text, &o, &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    let top = f.get(8, 1);
    let bottom = f.get(8, 14);
    assert_ne!(top, bottom, "the gradient runs top to bottom");
    let bands: std::collections::BTreeSet<u8> =
        f.pixels.iter().copied().filter(|p| *p != 0).collect();
    assert!(bands.len() >= 2 && bands.len() <= 4, "{bands:?}");
    assert!(bands
        .iter()
        .all(|b| (*b as usize) < out.palette.colors.len()));
}

#[test]
fn clip_path_limits_a_shape() {
    let text = svg(
        r##"<defs><clipPath id="c"><rect x="0" y="0" width="8" height="8"/></clipPath></defs>
            <rect x="0" y="0" width="16" height="16" fill="#c85a3c" clip-path="url(#c)"/>"##,
    );
    let out = rasterize(&text, &size(16), &pal()).unwrap();
    let f = &out.frames[0].1.frame;
    assert!(f.get(2, 2) != 0);
    assert_eq!(f.get(10, 10), 0);
}

#[test]
fn the_same_document_gives_the_same_pixels_at_every_size() {
    let text = svg(
        r##"<rect x="1.5" y="2" width="6.25" height="7.5" fill="#c85a3c"/>
            <path d="M2 12 Q 8 6 15 12" stroke="#181420" stroke-width="1.2" fill="none"/>"##,
    );
    for px in [32u16, 64, 128] {
        let mut o = SvgOptions {
            w: px,
            h: px,
            color_mode: ColorMode::Nearest,
            ..Default::default()
        };
        o.samples = 0;
        let a = rasterize(&text, &o, &pal()).unwrap();
        let b = rasterize(&text, &o, &pal()).unwrap();
        assert_eq!(a.frames[0].1.frame, b.frames[0].1.frame);
        assert!(
            a.report.samples >= 2,
            "auto samples at {px}: {}",
            a.report.samples
        );
    }
}

#[test]
fn finishing_runs_on_the_native_frame() {
    let text = svg(
        r##"<rect x="2" y="2" width="12" height="12" fill="#c85a3c"/>
            <rect x="2" y="2" width="12" height="6" fill="#e05a3c"/>"##,
    );
    let mut o = size(16);
    o.color_mode = ColorMode::Nearest;
    let profile = pxl_core::SizeProfile::of(pxl_core::SizeTier::S64);
    o.finish = Some(pxl_art::FinishOptions::for_profile(&profile));
    let out = rasterize(&text, &o, &pal()).unwrap();
    assert!(!out.report.finish.is_empty(), "{:?}", out.report);
    out.palette.validate().unwrap();
    for p in &out.frames[0].1.frame.pixels {
        assert!((*p as usize) < out.palette.colors.len());
    }
}
