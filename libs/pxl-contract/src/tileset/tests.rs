use super::*;

fn tileset() -> Tileset {
    let fill = TileDef {
        id: "grass_fill".into(),
        resource_id: "res-environment-grass-fill".into(),
        exact_masks: vec![255],
        approximate_masks: vec![],
        variants: vec![],
    };
    Tileset {
        contract: TILESET_CONTRACT.into(),
        id: "terrain_grass".into(),
        version: 1,
        palette_family: "environment".into(),
        tile_size: 32,
        tiles: vec![fill],
        license: "original".into(),
        review: ReviewState::Pending,
    }
}

fn module() -> Module {
    Module {
        contract: MODULE_CONTRACT.into(),
        id: "landmark_plaza".into(),
        version: 1,
        size: [4, 4],
        terrain: vec![TerrainLayer { tileset: "terrain_grass".into(), cells: vec![[0, 0]] }],
        tiles: vec![TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] }],
        modules: vec![],
        decor: vec![Decor {
            id: "fountain".into(),
            at: [64, 32],
            resource_id: Some("res-environment-fountain".into()),
            scene_kit: None,
            layer: DecorLayer::Front,
        }],
        collision: vec![CollisionCell { at: [2, 2], kind: "solid".into() }],
        license: "original".into(),
        review: ReviewState::Pending,
    }
}

#[test]
fn the_47_reduced_masks_match_the_frozen_count_and_bit_order() {
    let masks = reachable_masks();
    assert_eq!(masks.len(), 47, "the diagonal reduction yields exactly 47 masks");
    assert!(masks.windows(2).all(|w| w[0] < w[1]));
    // A diagonal without both orthogonal neighbours is reduced away.
    assert_eq!(reduce_mask(MASK_NE), 0);
    assert_eq!(reduce_mask(MASK_N | MASK_NE), MASK_N);
    assert_eq!(reduce_mask(MASK_N | MASK_E | MASK_NE), MASK_N | MASK_E | MASK_NE);
    // The full 8-neighbourhood survives untouched.
    assert_eq!(reduce_mask(0xff), 0xff);
    assert_eq!(describe_mask(0), "-");
    assert_eq!(describe_mask(MASK_N | MASK_E | MASK_NE), "NE+NE");
}

#[test]
fn a_tileset_reports_exact_approximate_and_missing_masks_separately() {
    let mut set = tileset();
    set.tiles[0].exact_masks = vec![255];
    set.tiles.push(TileDef {
        id: "grass_edge_n".into(),
        resource_id: "res-environment-grass-edge-n".into(),
        exact_masks: vec![MASK_E | MASK_S | MASK_SE | MASK_SW | MASK_W],
        approximate_masks: vec![MASK_S | MASK_SW | MASK_W],
        variants: vec!["a".into()],
    });
    assert!(set.validate().is_empty(), "{:?}", set.validate());
    let (exact, approximated, missing) = set.mask_coverage();
    assert!(exact.contains_key(&255));
    assert!(approximated.contains_key(&(MASK_S | MASK_SW | MASK_W)));
    assert!(missing.len() > 40, "most of the 47 masks are gaps: {:?}", missing.len());
}

#[test]
fn tilesets_reject_bad_ids_unreachable_masks_and_double_claiming() {
    let mut set = tileset();
    set.tiles[0].exact_masks = vec![255, 200]; // NE bit alone is unreachable
    set.tiles[0].approximate_masks = vec![255]; // also claimed as approximate
    let errors = set.validate().join("\n");
    for needle in ["unreachable mask", "exact and approximate", "lists mask"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
    assert!(!errors.contains("duplicate tile"));
    let mut dupe = tileset();
    dupe.tiles.push(dupe.tiles[0].clone());
    assert!(dupe.validate().iter().any(|e| e.contains("duplicate tile")));
}

#[test]
fn modules_validate_structure_but_check_references_at_compile_time() {
    let m = module();
    assert!(m.validate().is_empty(), "{:?}", m.validate());
    assert_eq!(m.requires(), vec!["terrain_grass".to_string()]);

    let mut bad = module();
    bad.size = [0, 4];
    bad.tiles[0].at = [9, 9];
    bad.terrain.push(TerrainLayer { tileset: "terrain_grass".into(), cells: vec![[9, 9]] });
    bad.modules.push(ModuleCell { module: "landmark_plaza".into(), at: [-1, 0], variant: None });
    bad.decor.push(Decor {
        id: "fountain".into(),
        at: [0, 0],
        resource_id: None,
        scene_kit: None,
        layer: DecorLayer::Back,
    });
    bad.collision[0].at = [8, 8];
    let errors = bad.validate().join("\n");
    for needle in [
        "size must be positive",
        "outside the 0x4 grid",
        "terrain cell 9,9",
        "cannot nest itself",
        "negative offset",
        "duplicate decor",
        "needs a resource_id or a scene_kit",
    ] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn compiled_scenes_reject_duplicate_ids_out_of_bounds_and_bad_keys() {
    let scene = CompiledScene {
        contract: COMPILED_SCENE_CONTRACT.into(),
        id: "scene_plaza".into(),
        version: 1,
        bounds: [128, 128],
        instances: vec![SceneInstance {
            id: "inst-0001".into(),
            resource_id: "res-environment-grass-fill".into(),
            layer: DecorLayer::Back,
            sort_y: 32,
            at: [0, 0],
            mirror: None,
        }],
        collision: BTreeMap::from([("2,2".into(), "solid".into())]),
        seed: 7,
        depends: BTreeMap::from([("terrain_grass".into(), sha256_digest(b"tileset"))]),
        license: "original".into(),
        review: ReviewState::Pending,
    };
    assert!(scene.validate().is_empty(), "{:?}", scene.validate());
    assert_eq!(scene.digest(), scene.digest());

    let mut bad = scene.clone();
    bad.instances[0].resource_id = "terrain_grass".into();
    bad.instances[0].at = [200, 0];
    bad.instances.push(bad.instances[0].clone());
    bad.collision.insert("oops".into(), " ".into());
    let errors = bad.validate().join("\n");
    for needle in [
        "does not reference a compiled resource id",
        "outside the 128x128 bounds",
        "duplicate instance",
        "not 'x,y'",
        "empty kind",
    ] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}
