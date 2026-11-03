use super::*;
use alloc::string::{String, ToString};

fn frame(rows: &[&str]) -> IndexFrame {
    let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    pxl_core::decode_matrix(&rows).unwrap()
}

fn palette() -> pxl_core::Palette {
    let mut p = pxl_core::Palette {
        id: "t".into(),
        colors: alloc::vec![
            pxl_core::Rgba::TRANSPARENT,
            pxl_core::Rgba::rgb(32, 32, 40),
            pxl_core::Rgba::rgb(240, 240, 232),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    p.special.outline = 1;
    p
}

fn always(_x: i32, _y: i32) -> bool {
    true
}

#[test]
fn a_lone_pixel_goes_away() {
    let mut f = frame(&["000", "020", "000"]);
    assert_eq!(remove_orphans(&mut f, &always), 1);
    assert_eq!(f.count_opaque(), 0);
}

#[test]
fn an_elbow_becomes_a_diagonal() {
    let mut f = frame(&["020", "022", "000"]);
    // (1,1) has neighbours up (1,0) and right (2,1) but no corner at (2,0).
    assert_eq!(remove_elbows(&mut f, &always), 1);
    assert_eq!(f.get(1, 1), 0);
    assert_eq!(f.get(1, 0), 2);
    assert_eq!(f.get(2, 1), 2);
}

#[test]
fn a_three_sided_notch_is_filled() {
    let mut f = frame(&["222", "202", "222"]);
    assert_eq!(fill_notches(&mut f, &always), 1);
    assert_eq!(f.get(1, 1), 2);
}

#[test]
fn kept_features_are_left_alone() {
    let f0 = frame(&["000", "020", "000"]);
    let g = SpriteGraph::build(&f0, &palette(), None);
    let mut f = f0.clone();
    let report = clean(&mut f, &f0, 1, &g, CleanOptions::ALL);
    assert_eq!(report.changed(), 0, "a kept detail is never cleaned away");
    assert_eq!(f, f0);
}

fn staircase(lengths: &[i32]) -> IndexFrame {
    let mut rows = Vec::new();
    for (position, length) in lengths.iter().enumerate() {
        for _ in 0..*length {
            rows.push(
                (0..9)
                    .map(|x| if x < position as i32 { '0' } else { '2' })
                    .collect::<String>(),
            );
        }
    }
    pxl_core::decode_matrix(&rows).unwrap()
}

#[test]
fn rhythm_repairs_irregular_runs_but_preserves_a_curve() {
    let mut f = staircase(&[4, 2, 1, 3, 4]);
    let pal = palette();
    let mut graph = SpriteGraph::build(&f, &pal, None);
    graph.protect.fill(false);
    let before = crate::metrics::jaggy_irregular(&f);
    assert!(before > 0);
    assert!(repair_rhythm(&mut f, 1, &graph) > 0);
    assert!(crate::metrics::jaggy_irregular(&f) < before);

    let curve = staircase(&[5, 3, 2, 1, 1, 2]);
    let mut profile = silhouette_profiles(&curve)[PROFILE_LEFT].clone();
    let original = profile.clone();
    regularize_profile(&mut profile);
    assert_eq!(profile, original);
}

#[test]
fn symmetry_repairs_only_s2_changes_and_respects_protect() {
    let native = frame(&["2222", "2222", "2222", "2222"]);
    let pal = palette();
    let graph = SpriteGraph::build(&native, &pal, None);
    let axis = graph.symmetry.unwrap();
    let mut scaled = pxl_core::scale_copy(&native, 2).unwrap();
    scaled.set(0, 0, 0);
    let introduced = introduced_blocks(&scaled, &native, 2);
    assert_eq!(
        repair_symmetry(&mut scaled, &native, 2, &graph, &introduced),
        1
    );
    let (mx, my) = mirror(0, 0, scaled.w as i32, scaled.h as i32, axis);
    assert_eq!(scaled.get(0, 0), scaled.get(mx, my));

    scaled.set(0, 0, 0);
    let untouched = vec![false; native.pixels.len()];
    assert_eq!(
        repair_symmetry(&mut scaled, &native, 2, &graph, &untouched),
        0
    );

    let mut protected = graph.clone();
    protected.protect[0] = true;
    assert_eq!(
        repair_symmetry(&mut scaled, &native, 2, &protected, &introduced),
        0
    );
    assert_eq!(scaled.get(0, 0), 0);
}
