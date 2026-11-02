use super::*;
use alloc::string::{String, ToString};

fn frame(rows: &[&str]) -> IndexFrame {
    let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    pxl_core::decode_matrix(&rows).unwrap()
}

fn palette() -> Palette {
    let mut p = Palette {
        id: "t".into(),
        colors: alloc::vec![
            Rgba::TRANSPARENT,
            Rgba::rgb(20, 20, 24),
            Rgba::rgb(232, 64, 64),
            Rgba::rgb(250, 200, 80),
        ],
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special::default(),
    };
    p.special.outline = 1;
    p
}

#[test]
fn a_doubled_frame_scores_ten_thousand() {
    let f = frame(&["12", "21"]);
    let doubled = pxl_core::double_copy(&f).unwrap();
    assert_eq!(copy2x_bps(&doubled), Some(10_000));
    assert_eq!(new_info_bps(&doubled, &f, 2), 0);
}

#[test]
fn odd_sides_have_no_copy2x() {
    assert_eq!(copy2x_bps(&frame(&["111", "111", "111"])), None);
}

#[test]
fn an_elbow_is_counted_and_a_straight_line_is_not() {
    assert_eq!(lshape_count(&frame(&["020", "022", "000"])), 1);
    assert_eq!(lshape_count(&frame(&["000", "222", "000"])), 0);
    assert_eq!(
        lshape_count(&frame(&["22", "22"])),
        0,
        "a block has no elbow"
    );
}

#[test]
fn a_lone_pixel_is_an_orphan_unless_protected() {
    let f = frame(&["000", "020", "000"]);
    let pal = palette();
    let graph = SpriteGraph::build(&f, &pal, None);
    assert_eq!(
        orphan_count(&f, 1, &graph),
        0,
        "a kept feature is not noise"
    );
    let mut loose = graph.clone();
    loose.protect.iter_mut().for_each(|p| *p = false);
    assert_eq!(orphan_count(&f, 1, &loose), 1);
}

#[test]
fn small_features_survive_a_copy_but_not_a_cut_corner() {
    let f = frame(&["000", "020", "000"]);
    let pal = palette();
    let graph = SpriteGraph::build(&f, &pal, None);
    let copy = pxl_core::scale_copy(&f, 3).unwrap();
    assert_eq!(small_feature_kept_bps(&copy, &f, 3, &graph), 10_000);
    let mut cut = copy.clone();
    cut.set(3, 3, 0);
    assert_eq!(small_feature_kept_bps(&cut, &f, 3, &graph), 0);
}

#[test]
fn an_irregular_staircase_is_flagged_and_a_curve_is_not() {
    // Interior run lengths 2, 1, 3: neither monotone nor within one pixel.
    let uneven = [(0, 4), (1, 2), (2, 1), (3, 3), (4, 4)];
    assert_eq!(irregular_runs(&uneven), 1);
    let uneven = [(0, 4), (1, 2), (2, 1), (3, 3), (4, 2), (5, 4)];
    assert_eq!(irregular_runs(&uneven), 2);
    let curve = [(0, 5), (1, 3), (2, 2), (3, 1), (4, 1), (5, 2)];
    assert_eq!(irregular_runs(&curve), 0);
    let slope = [(0, 3), (1, 1), (2, 2), (3, 1), (4, 2), (5, 3)];
    assert_eq!(irregular_runs(&slope), 0);
}

#[test]
fn outline_contrast_is_the_lightness_gap() {
    let pal = palette();
    let dark = outline_bg_dl(&pal, Rgba::rgb(28, 26, 24));
    let light = outline_bg_dl(&pal, Rgba::rgb(240, 240, 240));
    assert!(
        dark < 9_830,
        "a near-black outline on a near-black ground is hidden"
    );
    assert!(light > 9_830);
}

#[test]
fn a_broken_edge_has_more_components_than_bodies() {
    let whole = frame(&["222", "202", "222"]);
    assert_eq!(outline_continuity_bps(&whole), 10_000);
}

#[test]
fn ramp_coverage_counts_only_ramps_of_three() {
    let mut pal = palette();
    pal.ramps.insert("red".into(), alloc::vec![1, 2, 3]);
    let f = frame(&["22", "00"]);
    assert_eq!(ramp_coverage_bps(&f, &pal), 10_000);
    pal.ramps.insert("red".into(), alloc::vec![2, 3]);
    assert_eq!(ramp_coverage_bps(&f, &pal), 0);
}

#[test]
fn value_range_is_zero_for_one_colour_and_positive_for_two() {
    let pal = palette();
    assert_eq!(value_range(&frame(&["22", "22"]), &pal), 0);
    assert!(value_range(&frame(&["13", "13", "13", "13"]), &pal) > 0);
}

#[test]
fn symmetry_error_counts_mismatched_pairs() {
    let pal = palette();
    let f = frame(&["03030", "03330", "00300", "03330"]);
    let graph = SpriteGraph::build(&f, &pal, None);
    assert_eq!(symmetry_err(&f, &graph, &pal), Some(0));
    let mut bad = f.clone();
    bad.set(0, 1, 2);
    assert_eq!(symmetry_err(&bad, &graph, &pal), Some(1));
}

#[test]
fn symmetry_error_ignores_lighting_within_a_ramp() {
    let mut pal = palette();
    pal.ramps.insert("gold".into(), alloc::vec![2, 3]);
    let f = frame(&["03030", "03330", "00300", "03330"]);
    let graph = SpriteGraph::build(&f, &pal, None);
    assert_eq!(graph.symmetry, Some(Axis::Vertical));
    assert_eq!(symmetry_err(&f, &graph, &pal), Some(0));
    let mut lit = f.clone();
    lit.set(1, 1, 2);
    assert_eq!((lit.get(1, 1), lit.get(3, 1)), (2, 3));
    assert_eq!(symmetry_err(&lit, &graph, &pal), Some(0));
    lit.set(1, 1, 1);
    assert_eq!(symmetry_err(&lit, &graph, &pal), Some(1));
}

#[test]
fn two_parallel_lines_of_the_same_span_are_a_band() {
    let f = frame(&["000000", "222222", "333333", "000000"]);
    assert_eq!(banding_count(&f, 1), 1);
    let solid = frame(&["000000", "222222", "222222", "000000"]);
    assert_eq!(banding_count(&solid, 1), 0);
}
