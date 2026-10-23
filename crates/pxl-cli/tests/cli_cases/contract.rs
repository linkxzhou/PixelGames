use crate::support::{json_line, pxl, repo_root};
use serde_json::Value;
use std::fs;

#[test]
fn usage_error_is_json_exit_2() {
    let (code, stdout, _) = pxl(&["--json"]);
    assert_eq!(code, 2);
    let v = json_line(&stdout);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "E_USAGE");
    assert!(v["error"]["hint"].as_str().unwrap().contains("help"));
}

#[test]
fn schema_is_raw_json_and_recipe_parses() {
    let (code, stdout, _) = pxl(&["--json", "schema", "recipe"]);
    assert_eq!(code, 0);
    let schema = json_line(&stdout);
    assert!(schema.get("ok").is_none(), "schema is not wrapped in ok");
    assert!(
        schema.get("properties").is_some()
            || schema.get("$schema").is_some()
            || schema.get("title").is_some()
    );
    let recipe_path = repo_root().join("examples/tank/tank.recipe.json");
    let recipe: pxl_format::Recipe =
        serde_json::from_str(&fs::read_to_string(recipe_path).unwrap()).unwrap();
    assert_eq!(recipe.asset, "tank");
    assert_eq!(
        recipe.sprites[0].anims[0].macro_name(),
        Some("tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)")
    );
}

#[test]
fn bad_json_is_exit_3_and_rotate_is_exit_4_and_exists_is_exit_6() {
    let dir = std::env::temp_dir().join(format!("pxl-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.json");
    fs::write(&bad, "{").unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "op",
        bad.to_str().unwrap(),
        "--op",
        "{\"op\":\"noop\"}",
        "-o",
        dir.join("x.pxl.json").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert_eq!(json_line(&stdout)["error"]["code"], "E_SCHEMA");

    let spec = repo_root().join("examples/tank/tank_body.shapes.json");
    let doc = dir.join("tank.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "shapes",
        "--spec",
        spec.to_str().unwrap(),
        "--palette",
        "tank",
        "-o",
        doc.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    let ok = json_line(&stdout);
    assert_eq!(ok["ok"], true);
    assert!(ok["hash"].as_str().unwrap().starts_with("sha256:"));

    let again = dir.join("tank.pxl.json");
    let (code, stdout, _) = pxl(&[
        "--json",
        "gen",
        "shapes",
        "--spec",
        spec.to_str().unwrap(),
        "--palette",
        "tank",
        "-o",
        again.to_str().unwrap(),
    ]);
    assert_eq!(code, 6, "{stdout}");
    assert_eq!(json_line(&stdout)["error"]["code"], "E_EXISTS");

    let (code, stdout, _) = pxl(&[
        "--json",
        "--overwrite",
        "op",
        doc.to_str().unwrap(),
        "--op",
        r#"{"op":"rotate","angle":45}"#,
        "-o",
        doc.to_str().unwrap(),
    ]);
    assert_eq!(code, 4, "{stdout}");
    let err = json_line(&stdout);
    assert_eq!(err["error"]["code"], "E_SEMANTIC");
    assert!(err["error"]["hint"].as_str().unwrap().contains("rotate90"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn list_is_one_json_line() {
    let (code, stdout, _) = pxl(&["--json", "list", "macros"]);
    assert_eq!(code, 0);
    let v = json_line(&stdout);
    assert_eq!(v["ok"], true);
    let items = v["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i == "tread_cycle"));
    assert!(items.iter().any(|i| i == "idle_rumble"));
    assert!(items.iter().any(|i| i == "doll"));
    // Macros also carry argument metadata so callers do not guess signatures.
    let macros = v["macros"].as_array().expect("macros metadata present");
    let glow = macros
        .iter()
        .find(|m| m["name"] == "pulse_glow")
        .expect("pulse_glow metadata");
    let args = glow["args"].as_array().unwrap();
    assert_eq!(args.len(), 1, "{args:?}");
    assert_eq!(args[0]["name"], "ramp");
    assert!(!args.iter().any(|a| a["name"] == "steps"));
    assert!(glow["example"].as_str().unwrap().contains("ramp="));
    assert_eq!(glow["frames"], 2);
    let (code, stdout, _) = pxl(&["--json", "list", "generators"]);
    assert_eq!(code, 0);
    let gens = json_line(&stdout);
    assert!(gens["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i == "parts"));
}

/// Every schema kind a consumer is told about must resolve, including `shapes`
/// (the drawing spec) and `doc` (the name the harness uses for `pxl`).
#[test]
fn schema_serves_shapes_and_the_doc_alias() {
    for kind in [
        "recipe", "ops", "op", "pxl", "palette", "parts", "sheet", "shapes", "doc",
    ] {
        let (code, stdout, _) = pxl(&["--json", "schema", kind]);
        assert_eq!(code, 0, "schema {kind}: {stdout}");
        let schema = json_line(&stdout);
        assert!(
            schema.get("ok").is_none(),
            "schema {kind} is a bare document"
        );
    }
    let shapes = json_line(&pxl(&["--json", "schema", "shapes"]).1);
    let text = shapes.to_string();
    assert!(
        text.contains("\"draw\""),
        "the shapes schema describes the draw list"
    );
    let doc = json_line(&pxl(&["--json", "schema", "doc"]).1);
    let pxl_schema = json_line(&pxl(&["--json", "schema", "pxl"]).1);
    assert_eq!(doc, pxl_schema, "`doc` is an alias of `pxl`");
    // An unknown kind still lists every valid one, aliases included.
    let (code, stdout, _) = pxl(&["--json", "schema", "nope"]);
    assert_eq!(code, 2);
    let err = json_line(&stdout);
    let hint = err["error"]["hint"].as_str().unwrap();
    for kind in ["shapes", "doc", "pxl"] {
        assert!(hint.contains(kind), "hint omits {kind}: {hint}");
    }
}

/// `pxl list macros` is the source an agent plans from, so its numbers must be
/// the ones expansion really produces.
#[test]
fn list_macros_reports_real_defaults_and_marks_required_arguments() {
    let (code, stdout, _) = pxl(&["--json", "list", "macros"]);
    assert_eq!(code, 0, "{stdout}");
    let list = json_line(&stdout);
    let macros = list["macros"].as_array().unwrap();
    let find = |name: &str| macros.iter().find(|m| m["name"] == name).unwrap().clone();
    let default = |m: &Value, arg: &str| -> Value {
        m["args"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == arg)
            .unwrap()
            .clone()
    };
    assert_eq!(find("bob")["frames"], 4);
    assert_eq!(find("death_dissolve")["frames"], 6);
    assert_eq!(default(&find("death_dissolve"), "seed")["default"], "7");
    assert_eq!(
        default(&find("death_dissolve"), "pattern")["default"],
        "bayer4"
    );
    assert_eq!(default(&find("tread_cycle"), "bob")["default"], "1");
    assert_eq!(default(&find("walk_side"), "stride")["default"], "1");
    assert_eq!(default(&find("run_side"), "stride")["default"], "2");
    for arg in ["x", "y", "w", "h"] {
        assert_eq!(
            default(&find("blink"), arg)["required"],
            true,
            "blink {arg}"
        );
    }
    assert_eq!(default(&find("blink"), "lid")["derived"], true);
}

#[test]
fn misspelled_macro_argument_fails_the_build() {
    let dir = std::env::temp_dir().join(format!("pxl-typo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = repo_root().join("examples/reference/meadow-slime");
    fs::create_dir_all(&dir).unwrap();
    for f in ["palette.json", "slime.shapes.json"] {
        fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    // The meadow recipe's blink call is the fixture. A renamed argument must
    // fail the build instead of being dropped.
    let recipe = fs::read_to_string(src.join("recipe.json"))
        .unwrap()
        .replace(
            "blink(x=2, y=11, w=3, h=2, lid=5)",
            "blink(step=2, y=11, w=3, h=2, lid=5)",
        );
    fs::write(dir.join("recipe.json"), recipe).unwrap();
    let (code, stdout, _) = pxl(&[
        "--json",
        "build",
        dir.join("recipe.json").to_str().unwrap(),
        "-o",
        dir.join("out").to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{stdout}");
    assert!(json_line(&stdout)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("step"));
    let _ = fs::remove_dir_all(&dir);
}
