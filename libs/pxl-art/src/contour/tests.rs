use super::*;
use crate::upscale::UNIT;
use crate::SpriteGraph;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use pxl_core::{IndexFrame, Palette, Rgba};

use super::geom::fill_new_holes;
use super::geom::rdp_open;
use super::geom::smooth_curves;
use super::trace::Point;

fn frame(rows: &[&str]) -> IndexFrame {
    let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    pxl_core::decode_matrix(&rows).unwrap()
}

fn graph(frame: &IndexFrame) -> SpriteGraph {
    let mut palette = Palette {
        id: "t".into(),
        colors: alloc::vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(20, 20, 24),
            Rgba::rgb(240, 240, 232)
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: Default::default(),
    };
    palette.special.outline = 1;
    SpriteGraph::build(frame, &palette, None)
}

#[test]
fn rectangles_keep_their_exact_geometry() {
    let f = frame(&["000000", "022220", "022220", "022220", "000000"]);
    let mut g = graph(&f);
    g.protect.fill(false);
    for k in [2, 3, 4] {
        let out = reconstruct(&f, k, &g).unwrap();
        assert_eq!(out, pxl_core::scale_copy(&f, k).unwrap(), "k={k}");
    }
}

#[test]
fn holes_are_independent_loops() {
    let f = frame(&["022220", "020020", "020020", "022220", "000000"]);
    let mut g = graph(&f);
    g.protect.fill(false);
    let out = reconstruct(&f, 3, &g).unwrap();
    assert_eq!(out.get(4, 7), 2, "left ring");
    assert_eq!(out.get(7, 7), 0, "hole centre");
    assert_eq!(out.get(7, 10), 2, "bottom ring");
}

#[test]
fn only_new_enclosed_holes_are_restored() {
    let solid = frame(&["222", "222", "222"]);
    let mut new_hole = solid.clone();
    new_hole.set(1, 1, 0);
    fill_new_holes(&mut new_hole, &solid);
    assert_eq!(new_hole, solid);

    let existing = frame(&["222", "202", "222"]);
    let mut preserved = existing.clone();
    fill_new_holes(&mut preserved, &existing);
    assert_eq!(preserved.get(1, 1), 0);
}

#[test]
fn rdp_simplifies_a_pixel_staircase() {
    let points = [(0, 0), (1, 0), (1, 1), (2, 1), (2, 2), (3, 2)]
        .map(|(x, y)| Point::new(x * UNIT, y * UNIT));
    let simplified = rdp_open(&points);
    assert!(simplified.len() < points.len(), "{simplified:?}");
    assert_eq!(simplified.first(), points.first());
    assert_eq!(simplified.last(), points.last());
}

#[test]
fn gentle_curve_runs_are_fitted_with_quadratics() {
    let points = [
        (0, 2),
        (2, 0),
        (4, 0),
        (6, 2),
        (6, 4),
        (4, 6),
        (2, 6),
        (0, 4),
    ]
    .map(|(x, y)| Point::new(x * UNIT, y * UNIT));
    let curved = smooth_curves(&points);
    assert!(curved.len() > points.len());
    assert!(curved.windows(2).all(|edge| edge[0] != edge[1]));
}

#[test]
fn reconstruction_is_deterministic_and_uses_source_colours() {
    let f = frame(&["0011100", "0122210", "1222221", "0122210", "0011100"]);
    let g = graph(&f);
    let a = reconstruct(&f, 3, &g).unwrap();
    let b = reconstruct(&f, 3, &g).unwrap();
    assert_eq!(a, b);
    assert!(a.pixels.iter().all(|p| *p <= 2));
}

fn sloped_strip(tread: usize) -> IndexFrame {
    let mut rows = Vec::new();
    for step in 0..4usize {
        for _ in 0..tread {
            rows.push(
                (0..8)
                    .map(|x| if x < 3 - step { '1' } else { '2' })
                    .collect::<String>(),
            );
        }
    }
    pxl_core::decode_matrix(&rows).unwrap()
}

fn colour_profile(frame: &IndexFrame, color: u8) -> Vec<(i32, i32)> {
    let mut runs: Vec<(i32, i32)> = Vec::new();
    for y in 0..frame.h as i32 {
        let count = (0..frame.w as i32)
            .filter(|&x| frame.get(x, y) == color)
            .count() as i32;
        match runs.last_mut() {
            Some((position, length)) if *position == count => *length += 1,
            _ => runs.push((count, 1)),
        }
    }
    runs
}

#[test]
fn straight_slopes_have_uniform_output_steps() {
    for tread in [2usize, 3] {
        let f = sloped_strip(tread);
        let mut g = graph(&f);
        g.protect.fill(false);
        for k in [2, 3] {
            let out = reconstruct(&f, k, &g).unwrap();
            let runs = colour_profile(&out, 1);
            assert!(runs.len() >= 4, "tread={tread} k={k}: {runs:?}");
            let deltas: Vec<i32> = runs.windows(2).map(|pair| pair[1].0 - pair[0].0).collect();
            assert!(deltas.iter().all(|delta| *delta != 0));
            let delta_spread = deltas.iter().max().unwrap() - deltas.iter().min().unwrap();
            assert!(delta_spread <= 1, "tread={tread} k={k}: {runs:?}");
            let interior = &runs[1..runs.len() - 1];
            let min = interior.iter().map(|run| run.1).min().unwrap();
            let max = interior.iter().map(|run| run.1).max().unwrap();
            assert!(max - min <= 1, "tread={tread} k={k}: {runs:?}");
        }
    }
}

fn midpoint_disc(radius: i32) -> IndexFrame {
    let size = (2 * radius + 1) as usize;
    let mut mask = vec![false; size * size];
    let (mut x, mut y) = (radius, 0);
    let mut decision = 1 - radius;
    while y <= x {
        for (px, py) in [
            (x, y),
            (y, x),
            (y, -x),
            (x, -y),
            (-x, -y),
            (-y, -x),
            (-y, x),
            (-x, y),
        ] {
            let gx = radius + px;
            let gy = radius + py;
            if gx >= 0 && gy >= 0 && gx < size as i32 && gy < size as i32 {
                mask[gy as usize * size + gx as usize] = true;
            }
        }
        y += 1;
        if decision <= 0 {
            decision += 2 * y + 1;
        } else {
            x -= 1;
            decision += 2 * (y - x) + 1;
        }
    }
    let rows = (0..size)
        .map(|y| {
            let first = (0..size).find(|&x| mask[y * size + x]);
            first.map_or_else(
                || "0".repeat(size),
                |first| {
                    let last = (first..size).rev().find(|&x| mask[y * size + x]).unwrap();
                    (0..size)
                        .map(|x| if x >= first && x <= last { '2' } else { '0' })
                        .collect::<String>()
                },
            )
        })
        .collect::<Vec<_>>();
    pxl_core::decode_matrix(&rows).unwrap()
}

#[test]
fn circle_boundaries_stay_within_one_native_pixel() {
    let radius = 6i32;
    let k = 3i32;
    let f = midpoint_disc(radius);
    let mut g = graph(&f);
    g.protect.fill(false);
    let out = reconstruct(&f, k as u16, &g).unwrap();
    let centre = (2 * radius + 1) * k;
    let radius_q = centre;
    let mut largest_error = 0i32;
    for y in 0..out.h as i32 {
        let dy = 2 * y + 1 - centre;
        if dy.abs() + 2 * k >= radius_q {
            continue;
        }
        let expected = |x: i32| {
            let dx = 2 * x + 1 - centre;
            dx * dx + dy * dy <= radius_q * radius_q
        };
        let expected_left = (0..out.w as i32).find(|&x| expected(x));
        let actual_left = (0..out.w as i32).find(|&x| out.get(x, y) != 0);
        let (Some(expected_left), Some(actual_left)) = (expected_left, actual_left) else {
            panic!("missing row coverage at y={y}");
        };
        let expected_right = (expected_left..out.w as i32)
            .rev()
            .find(|&x| expected(x))
            .unwrap();
        let actual_right = (actual_left..out.w as i32)
            .rev()
            .find(|&x| out.get(x, y) != 0)
            .unwrap();
        largest_error = largest_error
            .max((actual_left - expected_left).abs())
            .max((actual_right - expected_right).abs());
    }
    assert!(
        largest_error <= k,
        "boundary error {largest_error} output pixels"
    );
}

#[test]
fn random_frames_are_deterministic_and_never_invent_colours() {
    let mut seed = 0x6d2b79f5u32;
    for case in 0..500u32 {
        let rows = (0..16)
            .map(|_| {
                (0..16)
                    .map(|_| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        match seed >> 29 {
                            0 | 1 => '0',
                            2 => '1',
                            3 => '2',
                            _ => '3',
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let f = pxl_core::decode_matrix(&rows).unwrap();
        let mut g = graph(&f);
        if case % 3 == 0 {
            g.protect.fill(false);
        }
        let k = 2 + (case % 3) as u16;
        let a = reconstruct(&f, k, &g).unwrap();
        let b = reconstruct(&f, k, &g).unwrap();
        assert_eq!(a, b, "case {case}");
        assert_eq!((a.w, a.h), (f.w * k, f.h * k));
        assert!(a.pixels.iter().all(|p| *p <= 3), "case {case}");
    }
}
