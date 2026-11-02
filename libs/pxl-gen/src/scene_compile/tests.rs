    use super::*;
    use pxl_contract::tileset::{
        CollisionCell, Decor, ModuleCell, TerrainLayer, TileCell, TileDef, TILESET_CONTRACT,
        MASK_E, MASK_N, MASK_NE, MASK_NW, MASK_S, MASK_SE, MASK_SW, MASK_W,
    };

    /// Every reachable mask except the all-same one, for approximation tests.
    fn reachable_approx() -> Vec<u8> {
        pxl_contract::tileset::reachable_masks()
            .into_iter()
            .filter(|m| *m != 255)
            .collect()
    }
    use pxl_contract::prototype::KitItem;

    struct NoKits;
    impl KitResolver for NoKits {
        fn resolve(&self, kit_id: &str) -> PxlResult<Vec<KitPlacement>> {
            Err(PxlError::semantic(format!("kit '{kit_id}' is not resolvable in this test")))
        }
    }

    fn grass_tileset() -> Tileset {
        Tileset {
            contract: TILESET_CONTRACT.into(),
            id: "terrain_grass".into(),
            version: 1,
            palette_family: "environment".into(),
            tile_size: 16,
            tiles: vec![TileDef {
                id: "grass_fill".into(),
                resource_id: "res-env-grass-fill".into(),
                exact_masks: (0..=255u8).filter(|b| pxl_contract::tileset::reduce_mask(*b) == 255).collect(),
                approximate_masks: vec![],
                variants: vec![],
            }],
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn plaza_module() -> Module {
        Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "landmark_plaza".into(),
            version: 1,
            size: [2, 2],
            terrain: vec![],
            tiles: vec![
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [1, 0] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 1] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [1, 1] },
            ],
            modules: vec![],
            decor: vec![Decor {
                id: "fountain".into(),
                at: [8, 8],
                resource_id: Some("res-env-fountain".into()),
                scene_kit: None,
                layer: DecorLayer::Front,
            }],
            collision: vec![CollisionCell { at: [1, 1], kind: "solid".into() }],
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn library() -> ModuleLibrary {
        ModuleLibrary {
            tilesets: BTreeMap::from([("terrain_grass".into(), grass_tileset())]),
            modules: BTreeMap::from([("landmark_plaza".into(), plaza_module())]),
            kits: BTreeMap::new(),
        }
    }

    #[test]
    fn a_simple_module_expands_deterministically_with_collision_kept_separate() {
        let lib = library();
        let (scene, report) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.instances.len(), 5, "4 tiles + 1 decor");
        assert_eq!(report.instances, 5);
        assert_eq!(report.pixels, 4 * 16 * 16);
        assert_eq!(scene.collision.get("1,1").map(String::as_str), Some("solid"));

        // Decoration is pixel-anchored at its declared offset, front layer.
        let fountain = scene.instances.iter().find(|i| i.resource_id.contains("fountain")).unwrap();
        assert_eq!(fountain.at, [8, 8]);
        assert_eq!(fountain.layer, DecorLayer::Front);

        // Determinism: same inputs, byte-identical scene.
        let (again, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.digest(), again.digest());

        // Collision is untouched when decor changes.
        let mut edited = library();
        edited.modules.get_mut("landmark_plaza").unwrap().decor[0].at = [4, 4];
        let (moved, _) = compile_scene(
            &edited,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(moved.collision, scene.collision, "moving decor must not move collision");
    }

    #[test]
    fn nested_modules_expand_with_offsets_and_depth_is_bounded() {
        let mut lib = library();
        let mut plaza = plaza_module();
        plaza.modules.push(ModuleCell { module: "landmark_plaza".into(), at: [2, 0], variant: None });
        // Self-nesting is a structural error…
        let errors = plaza.validate();
        assert!(errors.iter().any(|e| e.contains("cannot nest itself")), "{errors:?}");
        // …so use a distinct wrapper module for the depth test.
        let plaza = plaza_module();
        lib.modules.insert("landmark_plaza".into(), plaza);
        let wrapper = Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "wrapper".into(),
            version: 1,
            size: [6, 2],
            terrain: vec![],
            tiles: vec![],
            modules: vec![
                ModuleCell { module: "landmark_plaza".into(), at: [0, 0], variant: None },
                ModuleCell { module: "landmark_plaza".into(), at: [2, 0], variant: None },
            ],
            decor: vec![],
            collision: vec![],
            license: "original".into(),
            review: ReviewState::Approved,
        };
        lib.modules.insert("wrapper".into(), wrapper);

        let (scene, report) = compile_scene(
            &lib,
            &NoKits,
            "wrapper",
            "scene_wrapped",
            [96, 32],
            3,
            SceneBudget::default(),
        )
        .unwrap();
        // 2 plazas × (4 tiles + 1 decor).
        assert_eq!(scene.instances.len(), 10);
        assert_eq!(report.max_depth_reached, 2);
        // Both plazas expanded; the second is offset by 2 cells = 32px.
        let fountains: Vec<&SceneInstance> = scene
            .instances
            .iter()
            .filter(|i| i.resource_id.contains("fountain"))
            .collect();
        assert_eq!(fountains.len(), 2);
        let xs: BTreeSet<i32> = fountains.iter().map(|f| f.at[0]).collect();
        assert_eq!(xs, BTreeSet::from([8, 40]));

        // A two-module cycle (loop_a → loop_b → loop_a) is caught before expansion.
        let base = Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "cell".into(),
            version: 1,
            size: [1, 1],
            terrain: vec![],
            tiles: vec![TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] }],
            modules: vec![],
            decor: vec![],
            collision: vec![],
            license: "original".into(),
            review: ReviewState::Approved,
        };
        let mut loop_a = base.clone();
        loop_a.id = "loop_a".into();
        loop_a.modules = vec![ModuleCell { module: "loop_b".into(), at: [0, 0], variant: None }];
        loop_a.tiles.clear();
        let mut loop_b = base.clone();
        loop_b.id = "loop_b".into();
        loop_b.modules = vec![ModuleCell { module: "loop_a".into(), at: [0, 0], variant: None }];
        loop_b.tiles.clear();
        let mut cyclic = library();
        cyclic.modules.insert("loop_a".into(), loop_a);
        cyclic.modules.insert("loop_b".into(), loop_b);
        let err = compile_scene(
            &cyclic,
            &NoKits,
            "loop_a",
            "scene_cycle",
            [16, 16],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("cycle"), "{text}");

        // depth budget: a chain deeper than the limit fails closed.
        let mut depth_lib = library();
        depth_lib.modules.insert("deep".into(), base.clone());
        for n in 0..10 {
            let mut m = base.clone();
            m.id = format!("deep_{n}");
            m.modules = vec![ModuleCell {
                module: if n == 0 { "deep".into() } else { format!("deep_{}", n - 1) },
                at: [0, 0],
                variant: None,
            }];
            depth_lib.modules.insert(m.id.clone(), m);
        }
        let err = compile_scene(
            &depth_lib,
            &NoKits,
            "deep_9",
            "scene_deep",
            [16, 16],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("depth limit"), "{text}");
    }

    #[test]
    fn missing_tileset_coverage_is_an_error_not_a_default_tile() {
        let mut lib = library();
        let set = lib.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].exact_masks = vec![255]; // only the all-same mask
        set.tiles[0].approximate_masks = vec![MASK_N | MASK_E | MASK_S | MASK_W];

        // A single terrain cell still works: its own mask is all-same.
        let mut plaza = plaza_module();
        plaza.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0]],
        }];
        plaza.tiles.clear();
        plaza.decor.clear();
        plaza.collision.clear();
        lib.modules.insert("landmark_plaza".into(), plaza);

        // But a 2x2 terrain grid needs edge masks beyond the approximation
        // list; coverage gaps surface as missing masks and fail the build.
        let mut two = plaza_module();
        two.id = "two_cells".into();
        two.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0], [1, 0], [0, 1], [1, 1]],
        }];
        two.tiles.clear();
        two.modules.clear();
        lib.modules.insert("two_cells".into(), two);

        let err = compile_scene(
            &lib,
            &NoKits,
            "two_cells",
            "scene_gap",
            [32, 32],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("do not cover every mask"), "{text}");

        // Exact vs approximate are reported separately: rerun with full
        // approximation coverage and the report shows the approximated masks.
        let mut full = library();
        let set = full.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].exact_masks = vec![255];
        set.tiles[0].approximate_masks = reachable_approx();
        let mut two = plaza_module();
        two.id = "two_cells".into();
        two.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0], [1, 0], [0, 1], [1, 1]],
        }];
        two.tiles.clear();
        two.modules.clear();
        full.modules.insert("two_cells".into(), two);
        let (_, report) = compile_scene(
            &full,
            &NoKits,
            "two_cells",
            "scene_gap_approx",
            [32, 32],
            1,
            SceneBudget::default(),
        )
        .unwrap();
        assert!(
            !report.approximated_masks.is_empty(),
            "approximated masks must be reported separately: {:?}",
            report.approximated_masks
        );
        assert!(report.missing_masks.is_empty());
    }

    #[test]
    fn variant_choice_is_stable_and_seed_dependent() {
        let mut lib = library();
        let set = lib.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].variants = vec!["a".into(), "b".into()];
        let (scene, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            11,
            SceneBudget::default(),
        )
        .unwrap();
        let tags: Vec<&str> = scene
            .instances
            .iter()
            .filter(|i| i.resource_id.contains('#'))
            .map(|i| i.resource_id.split('#').nth(1).unwrap())
            .collect();
        assert_eq!(tags.len(), 4, "every tile instance carries a variant: {tags:?}");

        // Same seed: identical choices.
        let (again, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            11,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.digest(), again.digest());

        // Different seed: at least one choice differs (statistically certain
        // for 4 cells × 2 variants with avalanche mixing).
        let (other, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            12,
            SceneBudget::default(),
        )
        .unwrap();
        assert_ne!(scene.digest(), other.digest());
    }

    #[test]
    fn budget_limits_fail_closed() {
        let lib = library();
        let err = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_tiny",
            [32, 32],
            1,
            SceneBudget { max_instances: 3, max_pixels: 1024, max_depth: 8 },
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("instance budget"), "{text}");

        let err = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_tiny",
            [32, 32],
            1,
            SceneBudget { max_instances: 100, max_pixels: 100, max_depth: 8 },
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("pixel"), "{text}");
    }

    #[test]
    fn mask_helpers_match_the_frozen_47_tile_reduction() {
        // A 3x3 all-same grid: the centre cell is fully surrounded.
        let mask = cell_mask(3, 3, 1, 1, |_, _| true);
        assert_eq!(mask, 255);
        // Top-left corner of a 2x2 all-same grid: E, S and SE set.
        let mask = cell_mask(2, 2, 0, 0, |_, _| true);
        assert_eq!(mask, MASK_E | MASK_S | MASK_SE);
        // A diagonal neighbour without both orthogonal neighbours is reduced away.
        let mask = cell_mask(3, 3, 1, 1, |x, y| (x == 0 && y == 0) || (x == 1 && y == 1));
        assert_eq!(mask & MASK_NW, 0, "NW is not counted unless N and W are both same");

        let set = grass_tileset();
        assert!(matches!(
            tile_for_mask(&set, 255),
            Some(MaskResolution::Exact(_))
        ));
        let mut partial = set.clone();
        partial.tiles[0].exact_masks = vec![];
        partial.tiles[0].approximate_masks = vec![255];
        assert!(matches!(
            tile_for_mask(&partial, 255),
            Some(MaskResolution::Approximate(_))
        ));
        assert!(tile_for_mask(&partial, 12).is_none());
    }

    #[test]
    fn scene_kit_decor_expands_through_the_resolver() {
        let mut lib = library();
        lib.kits.insert(
            "porch_group".into(),
            SceneKit {
                contract: pxl_contract::prototype::SCENE_KIT_CONTRACT.into(),
                id: "porch_group".into(),
                version: 1,
                bounds: [32, 32],
                items: vec![
                    KitItem { prototype: "door_wood".into(), at: [0, 0], params: BTreeMap::new() },
                    KitItem { prototype: "flowerpot".into(), at: [16, 24], params: BTreeMap::new() },
                ],
                license: "original".into(),
                review: ReviewState::Approved,
            },
        );
        let mut plaza = plaza_module();
        plaza.decor.push(Decor {
            id: "porch".into(),
            at: [2, 2],
            resource_id: None,
            scene_kit: Some("porch_group".into()),
            layer: DecorLayer::Back,
        });
        lib.modules.insert("landmark_plaza".into(), plaza);

        struct Porch;
        impl KitResolver for Porch {
            fn resolve(&self, kit_id: &str) -> PxlResult<Vec<KitPlacement>> {
                assert_eq!(kit_id, "porch_group");
                Ok(vec![
                    KitPlacement { resource_id: "res-env-door".into(), layer: DecorLayer::Front, sort_y: 32, at: [0, 0] },
                    KitPlacement { resource_id: "res-env-flowerpot".into(), layer: DecorLayer::Front, sort_y: 24, at: [16, 24] },
                ])
            }
        }

        let (scene, _) = compile_scene(
            &lib,
            &Porch,
            "landmark_plaza",
            "scene_kit",
            [32, 32],
            5,
            SceneBudget::default(),
        )
        .unwrap();
        // 4 tiles + fountain + 2 kit placements.
        assert_eq!(scene.instances.len(), 7);
        let door = scene.instances.iter().find(|i| i.resource_id == "res-env-door").unwrap();
        assert_eq!(door.at, [2, 2]);
        let pot = scene.instances.iter().find(|i| i.resource_id == "res-env-flowerpot").unwrap();
        assert_eq!(pot.at, [18, 26]);
        assert!(scene.depends.contains_key("porch_group"));
    }
