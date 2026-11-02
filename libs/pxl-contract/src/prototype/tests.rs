use super::*;

fn zero_digest() -> String {
    sha256_digest(b"source")
}

fn house() -> Prototype {
    Prototype {
        contract: PROTOTYPE_CONTRACT.into(),
        id: "house_gable".into(),
        version: 1,
        kind: PrototypeKind::Building,
        view: ViewKind::TopDown,
        size_tiers: vec![SizeTier { id: "m".into(), size: [48, 48] }],
        params: BTreeMap::from([
            ("width_units".into(), ParamRange::Int { min: 2, max: 4, default: 3 }),
            ("door_side".into(), ParamRange::Choice { options: vec!["left".into(), "right".into()], default: "left".into() }),
            ("has_chimney".into(), ParamRange::Flag { default: true }),
        ]),
        regions: vec!["roof".into(), "wall".into(), "door".into()],
        connectors: vec![Connector { id: "porch".into(), at: [24, 47], dir: "S".into(), width: 8, accepts: vec!["path".into()] }],
        footprint: Footprint { visual: [0, 0, 48, 48], sort_y: 47, collision: vec![[0, 24, 48, 24]], interact: Some([24, 47]) },
        layers: vec![LayerSplit { id: "roof_front".into(), regions: vec!["roof".into()], layer: DrawLayer::Front }],
        variants: vec![VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(true), region: "roof".into(), rows: vec!["..11".into()] }],
        source: PrototypeSource { kind: "ascii".into(), path: "house_gable.ascii".into(), digest: zero_digest() },
        requires: vec!["roof_tile".into()],
        license: "original".into(),
        review: ReviewState::Pending,
    }
}

fn roof_tile() -> SurfaceRecipe {
    SurfaceRecipe {
        contract: SURFACE_RECIPE_CONTRACT.into(),
        id: "roof_tile".into(),
        version: 1,
        material_id: "tile".into(),
        applies_to: vec!["roof".into()],
        recipe: "tile_rows".into(),
        params: BTreeMap::new(),
        scale_px: 3,
        license: "original".into(),
        review: ReviewState::Pending,
    }
}

fn instance(proto: &Prototype) -> AssetInstance {
    AssetInstance {
        contract: ASSET_INSTANCE_CONTRACT.into(),
        instance_id: "farm_house".into(),
        prototype: proto.reference(),
        size_tier: "m".into(),
        params: BTreeMap::from([("width_units".into(), ParamValue::Int(3))]),
        surfaces: BTreeMap::from([("roof".into(), "roof_tile".into())]),
        seed: 7,
        patches: Vec::new(),
    }
}

fn surfaces() -> BTreeMap<String, SurfaceRecipe> {
    BTreeMap::from([("roof_tile".into(), roof_tile())])
}

fn entry(id: &str, kind: EntryKind, requires: &[&str], review: ReviewState) -> LockedEntry {
    LockedEntry {
        id: id.into(),
        kind,
        version: 1,
        digest: sha256_digest(id.as_bytes()),
        requires: requires.iter().map(|s| s.to_string()).collect(),
        review,
    }
}

/// A serialized record must expose exactly the keys its JSON Schema requires,
/// so the Rust types and the published contract cannot drift apart silently.
fn assert_matches_schema(value: &serde_json::Value, schema_text: &str, what: &str) {
    let schema: serde_json::Value = serde_json::from_str(schema_text).expect("schema parses");
    let required: BTreeSet<&str> = schema["required"]
        .as_array()
        .expect("required list")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    let properties: BTreeSet<&str> = schema["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    let actual: BTreeSet<&str> = value.as_object().expect("object").keys().map(String::as_str).collect();
    assert_eq!(actual, required, "{what}: serialized keys differ from the schema's required keys");
    assert_eq!(properties, required, "{what}: schema has optional or extra properties");
    assert_eq!(schema["additionalProperties"], false, "{what}: schema must reject unknown fields");
}

#[test]
fn rust_types_and_json_schemas_stay_in_step() {
    let proto = house();
    assert_matches_schema(
        &serde_json::to_value(&proto).unwrap(),
        include_str!("../../../steps/contracts/prototype.schema.json"),
        "prototype",
    );
    assert_matches_schema(
        &serde_json::to_value(roof_tile()).unwrap(),
        include_str!("../../../steps/contracts/surface-recipe.schema.json"),
        "surface-recipe",
    );
    assert_matches_schema(
        &serde_json::to_value(instance(&proto)).unwrap(),
        include_str!("../../../steps/contracts/asset-instance.schema.json"),
        "asset-instance",
    );
    let lock = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Approved)]);
    assert_matches_schema(
        &serde_json::to_value(&lock).unwrap(),
        include_str!("../../../steps/contracts/catalog-lock.schema.json"),
        "catalog-lock",
    );
}

#[test]
fn enums_serialize_to_the_values_the_schemas_allow() {
    let proto = serde_json::to_value(house()).unwrap();
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../steps/contracts/prototype.schema.json")).unwrap();
    let allowed = |path: &[&str]| -> Vec<String> {
        let mut node = &schema["properties"];
        for key in path {
            node = &node[*key];
        }
        node["enum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
    };
    assert!(allowed(&["kind"]).contains(&proto["kind"].as_str().unwrap().to_string()));
    assert!(allowed(&["view"]).contains(&proto["view"].as_str().unwrap().to_string()));
    assert!(allowed(&["review"]).contains(&proto["review"].as_str().unwrap().to_string()));
    for kind in [PrototypeKind::Building, PrototypeKind::Plant, PrototypeKind::Crop, PrototypeKind::Terrain, PrototypeKind::Prop, PrototypeKind::Fence, PrototypeKind::SceneKit] {
        let text = serde_json::to_value(kind).unwrap();
        assert!(allowed(&["kind"]).contains(&text.as_str().unwrap().to_string()), "{text}");
    }
}

#[test]
fn valid_records_pass() {
    let proto = house();
    assert_eq!(proto.validate(), Vec::<String>::new());
    assert_eq!(roof_tile().validate(), Vec::<String>::new());
    assert_eq!(instance(&proto).validate_against(&proto, &surfaces()), Vec::<String>::new());
}

#[test]
fn digest_is_stable_and_changes_with_content() {
    let a = house();
    let mut b = house();
    assert_eq!(a.digest(), b.digest());
    b.version = 2;
    assert_ne!(a.digest(), b.digest());
}

#[test]
fn invalid_prototype_is_rejected_with_reasons() {
    let mut proto = house();
    proto.size_tiers[0].size = [300, 48];
    proto.params.insert("width_units".into(), ParamRange::Int { min: 5, max: 2, default: 3 });
    proto.layers[0].regions = vec!["missing".into()];
    proto.connectors[0].dir = "Q".into();
    proto.license = String::new();
    let errors = proto.validate().join("\n");
    for needle in ["per edge", "min > max", "unknown region", "N/E/S/W", "license"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn variant_rules_must_reference_real_params_regions_and_values() {
    let mut proto = house();
    proto.variants = vec![
        VariantRule { param: "colour".into(), equals: ParamValue::Flag(true), region: "roof".into(), rows: vec!["1".into()] },
        VariantRule { param: "door_side".into(), equals: ParamValue::Text("up".into()), region: "roof".into(), rows: vec!["1".into()] },
        VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(true), region: "moat".into(), rows: vec!["1".into()] },
        VariantRule { param: "has_chimney".into(), equals: ParamValue::Flag(false), region: "roof".into(), rows: Vec::new() },
    ];
    let errors = proto.validate().join("\n");
    for needle in ["unknown param 'colour'", "not one of", "unknown region 'moat'", "non-empty rows"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn instance_param_and_reference_errors_are_reported() {
    let proto = house();
    let mut inst = instance(&proto);
    inst.params.insert("width_units".into(), ParamValue::Int(9));
    inst.params.insert("colour".into(), ParamValue::Text("red".into()));
    inst.params.insert("door_side".into(), ParamValue::Text("up".into()));
    inst.size_tier = "xl".into();
    inst.surfaces.insert("moat".into(), "roof_tile".into());
    inst.surfaces.insert("wall".into(), "no_such_surface".into());
    inst.patches.push(RegionPatch { region: "chimney".into(), rows: vec![".".into()] });
    let errors = inst.validate_against(&proto, &surfaces()).join("\n");
    for needle in ["outside 2..=4", "unknown param 'colour'", "not one of", "unknown size tier", "unknown region 'moat'", "not in the catalog", "patch targets unknown region"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn stale_prototype_digest_is_detected() {
    let proto = house();
    let inst = instance(&proto);
    let mut changed = house();
    changed.footprint.sort_y = 40;
    let errors = inst.validate_against(&changed, &surfaces());
    assert!(errors.iter().any(|e| e.contains("digest does not match")), "{errors:?}");
}

#[test]
fn catalog_lock_seal_ignores_input_order() {
    let a = vec![
        entry("house_gable", EntryKind::Prototype, &["roof_tile"], ReviewState::Approved),
        entry("roof_tile", EntryKind::Surface, &[], ReviewState::Approved),
    ];
    let mut b = a.clone();
    b.reverse();
    let (la, lb) = (CatalogLock::seal(a), CatalogLock::seal(b));
    assert_eq!(la.digest, lb.digest);
    assert_eq!(la.verify(true), Vec::<String>::new());
}

#[test]
fn catalog_lock_rejects_missing_dependency_cycle_tamper_and_unapproved() {
    let missing = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &["ghost"], ReviewState::Approved)]);
    assert!(missing.verify(true).iter().any(|e| e.contains("missing entry 'ghost'")));

    let cycle = CatalogLock::seal(vec![
        entry("a", EntryKind::Prototype, &["b"], ReviewState::Approved),
        entry("b", EntryKind::Prototype, &["a"], ReviewState::Approved),
    ]);
    assert!(cycle.verify(true).iter().any(|e| e.contains("cycle")));

    let mut tampered = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Approved)]);
    tampered.entries[0].version = 9;
    assert!(tampered.verify(true).iter().any(|e| e.contains("digest does not match")));

    let pending = CatalogLock::seal(vec![entry("a", EntryKind::Prototype, &[], ReviewState::Pending)]);
    assert!(pending.verify(true).iter().any(|e| e.contains("not approved")));
    assert_eq!(pending.verify(false), Vec::<String>::new());
}
