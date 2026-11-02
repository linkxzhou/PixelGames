    use super::*;
    use pxl_format::parse_shapes;

    #[test]
    fn mirrored_rect_is_symmetric() {
        let spec = parse_shapes(r#"{"w":8,"h":4,"mirror":"x","draw":[{"rect":[1,1,2,2],"c":3}]}"#)
            .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(1, 1), 3);
        assert_eq!(frame.get(5, 1), 3);
        assert_eq!(frame.get(0, 0), 0);
    }

    #[test]
    fn circle_mirror_false_stays_put() {
        let spec = parse_shapes(
            r#"{"w":8,"h":8,"mirror":"x","draw":[{"circle":[2,3,1],"c":4,"mirror":false}]}"#,
        )
        .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(2, 3), 4);
        assert!(frame.get(5, 3) == 0);
    }

    #[test]
    fn local_mirror_true_defaults_to_x_when_global_is_none() {
        let spec = parse_shapes(
            r#"{"w":8,"h":4,"mirror":"none","draw":[{"rect":[1,1,2,2],"c":3,"mirror":true}]}"#,
        )
        .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let frame = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(frame.get(1, 1), 3);
        assert_eq!(frame.get(5, 1), 3);
    }

    fn run(draw: &str) -> PxlResult<IndexFrame> {
        let spec = parse_shapes(&format!(r#"{{"w":8,"h":8,"draw":[{draw}]}}"#)).unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        generate_shapes(&spec, &pal)
    }

    #[test]
    fn extreme_rect_coordinates_neither_panic_nor_wrap() {
        // x + w overflows i32; the rect is entirely off the canvas.
        let frame = run(r#"{"rect":[2147483647,0,1,1],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
        // A huge rect that does cover the canvas is clipped, not iterated.
        let frame =
            run(r#"{"rect":[-2147483648,-2147483648,2147483647,2147483647],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0, "the far corner stays off-canvas");
        let frame =
            run(r#"{"rect":[-1000000000,-1000000000,1000000008,1000000008],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 64);
    }

    #[test]
    fn huge_circle_is_clipped_and_cheap() {
        let started = std::time::Instant::now();
        let frame = run(r#"{"circle":[4,4,1000000000],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 64);
        assert!(
            started.elapsed().as_millis() < 500,
            "circle was not clipped"
        );
        let frame = run(r#"{"circle":[2147483647,2147483647,2],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
    }

    #[test]
    fn extreme_lines_are_rejected_or_clipped_without_overflow() {
        assert!(run(r#"{"line":[-2147483648,0,2147483647,0],"c":2}"#).is_err());
        let frame = run(r#"{"line":[-3,4,20,4],"c":2}"#).unwrap();
        assert_eq!(frame.count_opaque(), 8);
    }

    #[test]
    fn an_invalid_colour_fails_even_when_fully_clipped_or_overdrawn() {
        let bad = 200;
        assert!(run(&format!(r#"{{"rect":[100,100,1,1],"c":{bad}}}"#)).is_err());
        assert!(run(&format!(
            r#"{{"rect":[0,0,8,8],"c":{bad}}},{{"rect":[0,0,8,8],"c":2}}"#
        ))
        .is_err());
        assert!(run(&format!(r#"{{"circle":[0,0,1],"c":{bad}}}"#)).is_err());
        assert!(run(&format!(r#"{{"line":[0,0,3,3],"c":{bad}}}"#)).is_err());
    }

    #[test]
    fn pixels_with_extreme_origin_do_not_overflow() {
        let frame = run(r#"{"pixels":["22"],"x":2147483647,"y":0}"#).unwrap();
        assert_eq!(frame.count_opaque(), 0);
    }

    fn grid(frame: &IndexFrame) -> Vec<String> {
        (0..frame.h as i32)
            .map(|y| {
                (0..frame.w as i32)
                    .map(|x| if frame.get(x, y) == 0 { '.' } else { '#' })
                    .collect()
            })
            .collect()
    }

    fn run_n(w: u16, h: u16, draw: &str) -> PxlResult<IndexFrame> {
        let spec = parse_shapes(&format!(r#"{{"w":{w},"h":{h},"draw":[{draw}]}}"#)).unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        generate_shapes(&spec, &pal)
    }

    #[test]
    fn a_plain_line_still_parses_as_a_line_and_width_makes_it_thick() {
        let thin = run_n(8, 8, r#"{"line":[0,2,7,2],"c":2}"#).unwrap();
        assert_eq!(thin.count_opaque(), 8);
        let one = run_n(8, 8, r#"{"line":[0,2,7,2],"width":1,"c":2}"#).unwrap();
        assert_eq!(thin.pixels, one.pixels, "width 1 equals a plain line");
        let thick = run_n(8, 8, r#"{"line":[0,3,7,3],"width":3,"c":2}"#).unwrap();
        assert_eq!(thick.count_opaque(), 24);
        assert_eq!(thick.get(4, 2), 2);
        assert_eq!(thick.get(4, 4), 2);
        assert_eq!(thick.get(4, 5), 0);
        assert!(run_n(8, 8, r#"{"line":[0,0,7,7],"width":0,"c":2}"#).is_err());
        assert!(run_n(8, 8, r#"{"line":[0,0,7,7],"width":99,"c":2}"#).is_err());
    }

    #[test]
    fn ellipse_fills_a_symmetric_round_shape_inside_its_box() {
        let f = run_n(10, 6, r#"{"ellipse":[0,0,10,6],"c":2}"#).unwrap();
        let g = grid(&f);
        for row in &g {
            let r: String = row.chars().rev().collect();
            assert_eq!(*row, r, "mirror symmetric in x: {g:?}");
        }
        assert_eq!(g[0], g[5], "mirror symmetric in y");
        assert_eq!(g[2].matches('#').count(), 10, "widest rows span the box");
        assert!(g[0].matches('#').count() < g[2].matches('#').count());
        assert_eq!(f.get(0, 0), 0, "corner of the box is outside the ellipse");
        let ring = run_n(10, 6, r#"{"ellipse":[0,0,10,6],"c":2,"fill":false}"#).unwrap();
        assert!(ring.count_opaque() < f.count_opaque());
        assert_eq!(ring.get(5, 3), 0, "ring is hollow");
        assert!(run_n(8, 8, r#"{"ellipse":[0,0,0,4],"c":2}"#).is_err());
    }

    #[test]
    fn a_square_ellipse_matches_a_filled_disc_footprint() {
        let f = run_n(8, 8, r#"{"ellipse":[0,0,8,8],"c":2}"#).unwrap();
        assert_eq!(f.count_opaque(), 52);
    }

    #[test]
    fn polygon_fills_a_triangle_with_even_odd_edges() {
        let f = run_n(8, 8, r#"{"polygon":[[0,0],[8,0],[0,8]],"c":2}"#).unwrap();
        let g = grid(&f);
        assert_eq!(g[0], "########");
        assert_eq!(g[7], "#.......");
        let n = f.count_opaque();
        assert!((28..=36).contains(&n), "triangle area ~32, got {n}");
        // The two halves of the diagonal split are complementary, with no
        // pixel dropped or doubled where the edge passes through a centre.
        let other = run_n(8, 8, r#"{"polygon":[[8,0],[8,8],[0,8]],"c":2}"#).unwrap();
        assert_eq!(n + other.count_opaque(), 64);
        assert!(run_n(8, 8, r#"{"polygon":[[0,0],[1,1]],"c":2}"#).is_err());
    }

    #[test]
    fn polygon_is_even_odd_and_handles_extreme_points() {
        let square = run_n(8, 8, r#"{"polygon":[[1,1],[7,1],[7,7],[1,7]],"c":2}"#).unwrap();
        assert_eq!(square.count_opaque(), 36);
        let frame = run_n(
            8,
            8,
            r#"{"polygon":[[-2147483648,-2147483648],[2147483647,0],[0,2147483647]],"c":2}"#,
        )
        .unwrap();
        assert!(frame.count_opaque() <= 64);
        let too_many = format!(r#"{{"polygon":[{}],"c":2}}"#, vec!["[0,0]"; 300].join(","));
        assert!(run_n(8, 8, &too_many).is_err());
    }

    #[test]
    fn new_primitives_validate_colour_and_mirror() {
        let bad = 200;
        assert!(run_n(8, 8, &format!(r#"{{"ellipse":[0,0,4,4],"c":{bad}}}"#)).is_err());
        assert!(run_n(
            8,
            8,
            &format!(r#"{{"polygon":[[0,0],[4,0],[0,4]],"c":{bad}}}"#)
        )
        .is_err());
        let spec =
            parse_shapes(r#"{"w":8,"h":4,"mirror":"x","draw":[{"ellipse":[0,0,3,3],"c":3}]}"#)
                .unwrap();
        let pal = crate::builtin_palette("tank").unwrap();
        let f = generate_shapes(&spec, &pal).unwrap();
        assert_eq!(f.get(1, 1), 3);
        assert_eq!(f.get(6, 1), 3, "mirrored copy");
    }
