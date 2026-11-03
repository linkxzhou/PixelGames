use super::*;

    fn cell(name: &str, w: u16) -> SheetCell {
        SheetCell {
            name: name.into(),
            frame: IndexFrame::new(w, 4).unwrap(),
            duration_ms: 100,
            tag: None,
        }
    }

    #[test]
    fn pad_does_not_wrap_and_oversize_sheets_are_rejected() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let ok = pack_sheet(&rows, "rows-by-anim", 1).unwrap();
        assert_eq!((ok.image.w, ok.image.h), (9, 4));
        let err = pack_sheet(&rows, "rows-by-anim", u32::MAX).unwrap_err();
        assert!(err.message.contains("2048"), "{err:?}");
        let wide: Vec<SheetCell> = (0..40).map(|i| cell(&format!("f{i}"), 64)).collect();
        assert!(pack_sheet(&[wide], "rows-by-anim", 1).is_err());
    }

    #[test]
    fn budget_reports_rgba8_estimate_not_png_size() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let packed = pack_sheet(&rows, "rows-by-anim", 1).unwrap();
        let triad = pack_sheet_padded(
            &rows,
            "rows-by-anim",
            SheetPadding {
                border: 2,
                shape: 3,
                inner: 1,
            },
        )
        .unwrap();
        assert!(triad.image.w > packed.image.w);
        assert_eq!(triad.placements[0].x, 2);
        assert_eq!(triad.placements[0].w, rows[0][0].frame.w as u32 + 2);

        let budget = SheetBudget::of(&packed);
        // 9x4 sheet: the RGBA8 estimate is w*h*4, not the indexed PNG size.
        assert_eq!(budget.rgba8_bytes, 9 * 4 * 4);
        assert_eq!(budget.frames, 2);
        assert_eq!(budget.pages, 1);
        assert_eq!(budget.page_limit, 1);
        // A full 2048x2048 single page tops out at exactly 16 MiB. Frames
        // cannot reach that size (MAX_DIM is 256), so build the sheet
        // directly and check the arithmetic.
        let full = PackedSheet {
            image: IndexFrame::canvas(2048, 2048).unwrap(),
            placements: Vec::new(),
        };
        assert_eq!(SheetBudget::of(&full).rgba8_bytes, 2048 * 2048 * 4);
        assert_eq!(SheetBudget::of(&full).rgba8_mib, 16.0);
    }


    #[test]
    fn json_array_frames_is_array_hash_is_object() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let packed = pack_sheet(&rows, "rows-by-anim", 0).unwrap();
        let hash = sheet_json_format(&packed, "s.png", serde_json::json!({}), "json-hash").unwrap();
        let arr = sheet_json_format(&packed, "s.png", serde_json::json!({}), "json-array").unwrap();
        let hv: serde_json::Value = serde_json::from_str(&hash).unwrap();
        let av: serde_json::Value = serde_json::from_str(&arr).unwrap();
        assert!(hv["frames"].is_object());
        assert!(av["frames"].is_array());
        assert_eq!(av["frames"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn columns_and_power_of_two_and_width_wrap() {
        let cells: Vec<SheetCell> = (0..4).map(|i| cell(&format!("f{i}"), 8)).collect();
        let rows = vec![cells];
        let by_cols = pack_sheet_opts(
            &rows,
            "grid",
            SheetPackOpts {
                columns: Some(2),
                ..SheetPackOpts::default()
            },
        )
        .unwrap();
        assert_eq!(by_cols.image.w, 16);
        assert_eq!(by_cols.image.h, 8);
        assert_eq!(by_cols.placements[2].y, 4); // second row starts at y=4? wait cells h=4
        // cell() uses h=4 — placements[2] should be on second row
        assert_eq!(by_cols.placements[2].y, 4);

        let pot = pack_sheet_opts(
            &rows,
            "horizontal",
            SheetPackOpts {
                power_of_two: true,
                ..SheetPackOpts::default()
            },
        )
        .unwrap();
        assert!(pot.image.w.is_power_of_two());
        assert!(pot.image.h.is_power_of_two());

        let wrapped = pack_sheet_opts(
            &rows,
            "horizontal",
            SheetPackOpts {
                sheet_width: Some(20),
                pad: SheetPadding::shape_only(0),
                ..SheetPackOpts::default()
            },
        )
        .unwrap();
        assert_eq!(wrapped.image.w, 20);
        assert!(wrapped.placements.iter().any(|p| p.y > 0));
    }

    #[test]
    fn fit_dims_never_upsizes() {
        assert_eq!(crate::fit_dims(10, 8, 100, 100), (10, 8));
        assert_eq!(crate::fit_dims(100, 50, 50, 50), (50, 25));
    }
