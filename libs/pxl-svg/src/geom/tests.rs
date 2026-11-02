    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn transforms_compose_in_svg_order() {
        let m = parse_transform("translate(10,0) scale(2)").unwrap();
        assert_eq!(m.apply((1.0, 1.0)), (12.0, 2.0));
        let r = parse_transform("rotate(90)").unwrap();
        let p = r.apply((1.0, 0.0));
        assert!(close(p.0, 0.0) && close(p.1, 1.0));
        let r = parse_transform("rotate(180 5 5)").unwrap();
        let p = r.apply((0.0, 0.0));
        assert!(close(p.0, 10.0) && close(p.1, 10.0));
        let inv = m.invert().unwrap();
        let q = inv.apply(m.apply((3.0, -4.0)));
        assert!(close(q.0, 3.0) && close(q.1, -4.0));
        assert!(parse_transform("perspective(2)").is_err());
    }

    #[test]
    fn path_commands_end_where_expected() {
        let mut b = PathBuilder::new(Affine::IDENTITY);
        path(
            "M1 1 h4 v4 H1 z m10 0 l2 2 c1 0 2 1 2 2 s1 1 2 0 q1 1 2 0 t2 0 a2 2 0 0 1 4 0",
            &mut b,
        )
        .unwrap();
        let polys = b.finish();
        assert_eq!(polys.len(), 2);
        assert!(polys[0].closed);
        assert_eq!(
            polys[0].pts,
            vec![(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0)]
        );
        let end = *polys[1].pts.last().unwrap();
        assert!(close(end.0, 25.0) && close(end.1, 5.0), "{end:?}");
        let mut b = PathBuilder::new(Affine::IDENTITY);
        assert!(path("L 1 2", &mut b).is_err());
    }

    #[test]
    fn a_half_arc_passes_through_its_apex() {
        let mut b = PathBuilder::new(Affine::IDENTITY);
        path("M0 0 A 5 5 0 0 1 10 0", &mut b).unwrap();
        let polys = b.finish();
        let min_y = polys[0].pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!((min_y + 5.0).abs() < 0.05, "{min_y}");
    }

    #[test]
    fn stroke_pieces_cover_segments_and_joins() {
        let poly = Poly {
            pts: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            closed: false,
        };
        let p = stroke(&[poly.clone()], 2.0, Join::Miter, Cap::Butt, 4.0);
        assert_eq!(p.len(), 3);
        let r = stroke(&[poly], 2.0, Join::Round, Cap::Round, 4.0);
        assert_eq!(r.len(), 5);
    }
