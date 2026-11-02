    #[test]
    fn contract_hard_limits_match_step_300() {
        use pxl_contract::policy;
        assert_eq!(policy::MAX_DIM, pxl_core::MAX_DIM);
        assert_eq!(policy::MAX_TEXT_COLORS, pxl_core::MAX_TEXT_COLORS);
        assert_eq!(policy::MAX_CANVAS, pxl_core::MAX_CANVAS);
        assert_eq!(usize::from(policy::MAX_ANIM_FRAMES), pxl_core::MAX_ANIM_FRAMES);
    }
