use super::*;

use std::collections::BTreeMap;

fn kit() -> SceneKit {
    SceneKit {
        contract: SCENE_KIT_CONTRACT.into(),
        id: "porch_group".into(),
        version: 1,
        bounds: [48, 32],
        items: vec![
            KitItem { prototype: "door_wood".into(), at: [16, 8], params: BTreeMap::new() },
            KitItem { prototype: "flowerpot".into(), at: [4, 24], params: BTreeMap::new() },
        ],
        license: "original".into(),
        review: ReviewState::Pending,
    }
}

#[test]
fn a_consistent_kit_validates_and_lists_its_dependencies() {
    let k = kit();
    assert_eq!(k.validate(), Vec::<String>::new());
    assert_eq!(k.requires(), vec!["door_wood".to_string(), "flowerpot".to_string()]);
    assert_eq!(k.digest(), kit().digest());
}

#[test]
fn bad_kits_are_rejected_with_reasons() {
    let mut k = kit();
    k.items.truncate(1);
    k.items[0].at = [99, 0];
    k.bounds = [48, 0];
    k.license.clear();
    let errors = k.validate().join("\n");
    for needle in ["bounds must be positive", "at least two items", "outside the kit bounds", "license"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn items_must_name_real_prototypes_and_legal_params() {
    let mut k = kit();
    k.items[1].params.insert("tall".into(), ParamValue::Flag(true));
    let errors = k.validate_items(&BTreeMap::new()).join("\n");
    assert!(errors.contains("missing prototype 'door_wood'"), "{errors}");
}
