//! CLI exit codes, JSON shape, and the tank demo acceptance checks.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn pxl(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pxl"))
        .args(args)
        .output()
        .expect("pxl binary");
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    (code, stdout, stderr)
}

fn json_line(stdout: &str) -> Value {
    let line = stdout.trim();
    assert!(!line.contains('\n'), "stdout must be one JSON line: {line}");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {line}"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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
}

#[test]
fn tank_build_is_deterministic_and_validates() {
    let root = repo_root();
    let recipe = root.join("examples/tank/tank.recipe.json");
    let a = std::env::temp_dir().join(format!("pxl-tank-a-{}", std::process::id()));
    let b = std::env::temp_dir().join(format!("pxl-tank-b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "build",
        recipe.to_str().unwrap(),
        "-o",
        a.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let built = json_line(&stdout);
    assert_eq!(built["ok"], true);
    assert!(built["hash"].as_str().unwrap().starts_with("sha256:"));
    let (code, _, stderr) = pxl(&[
        "build",
        recipe.to_str().unwrap(),
        "-o",
        b.to_str().unwrap(),
        "--quiet",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let mut files = Vec::new();
    for entry in fs::read_dir(&a).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    assert!(files.iter().any(|f| f == "tank.sheet.png"));
    assert!(files.iter().any(|f| f == "tank.sheet.json"));
    assert!(files.iter().any(|f| f == "preview.png"));
    assert!(files.iter().any(|f| f == "tank_move_E.gif"));
    for name in &files {
        let ha = sha256(&a.join(name));
        let hb = sha256(&b.join(name));
        assert_eq!(ha, hb, "{name} differs between builds");
    }

    let (code, stdout, stderr) = pxl(&[
        "--json",
        "validate",
        a.join("tank.sheet.json").to_str().unwrap(),
        "--rules",
        "tank",
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let report = json_line(&stdout);
    assert_eq!(report["ok"], true);
    assert!(report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["pass"] == true));

    let sheet: Value =
        serde_json::from_str(&fs::read_to_string(a.join("tank.sheet.json")).unwrap()).unwrap();
    let frames = sheet["frames"].as_object().unwrap();
    assert!(frames.len() >= 4 * 4 + 4 * 2 + 4);
    for frame in frames.values() {
        assert!(frame.get("frame").is_some());
        assert!(frame.get("duration").is_some());
        assert!(frame.get("spriteSourceSize").is_some());
        assert!(frame.get("sourceSize").is_some());
    }
    let tags = sheet["meta"]["frameTags"].as_array().unwrap();
    for dir in ["N", "E", "S", "W"] {
        let move_tag = tags
            .iter()
            .find(|t| t["name"] == format!("move_{dir}"))
            .unwrap();
        let idle_tag = tags
            .iter()
            .find(|t| t["name"] == format!("idle_{dir}"))
            .unwrap();
        let span = |t: &Value| t["to"].as_u64().unwrap() - t["from"].as_u64().unwrap() + 1;
        assert!(span(move_tag) >= 4, "{move_tag}");
        assert!(span(idle_tag) >= 2, "{idle_tag}");
        assert_eq!(move_tag["direction"], "forward");
    }
    let tiles = sheet["meta"]["pxl"]["tiles"]["ground"]["frames"]
        .as_array()
        .unwrap();
    assert!(tiles.len() >= 4);

    let doc: Value =
        serde_json::from_str(&fs::read_to_string(a.join("tank.pxl.json")).unwrap()).unwrap();
    for (dir, expect_x_gt, expect_y_lt) in [
        ("N", None, Some(true)),
        ("E", Some(true), None),
        ("S", None, Some(false)),
        ("W", Some(false), None),
    ] {
        let rows = doc["frames"][&format!("idle_{dir}_0")].as_array().unwrap();
        let (mx, my, bx, by, mn, bn) = centroids(rows);
        assert!(mn > 0.0, "{dir} has no muzzle");
        assert!(bn > mn);
        if let Some(gt) = expect_x_gt {
            if gt {
                assert!(mx > bx, "{dir} gun should point east");
            } else {
                assert!(mx < bx, "{dir} gun should point west");
            }
        }
        if let Some(lt) = expect_y_lt {
            if lt {
                assert!(my < by, "{dir} gun should point north");
            } else {
                assert!(my > by, "{dir} gun should point south");
            }
        }
    }

    let (code, stdout, _) = pxl(&[
        "--json",
        "validate",
        a.join("tank.pxl.json").to_str().unwrap(),
        "--rules",
        "tank",
    ]);
    assert_eq!(
        code, 5,
        "sprite-only validate should fail tile count: {stdout}"
    );
    assert_eq!(json_line(&stdout)["error"]["code"], "E_QA");

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

fn centroids(rows: &[Value]) -> (f64, f64, f64, f64, f64, f64) {
    let mut mx = 0.0;
    let mut my = 0.0;
    let mut mn = 0.0;
    let mut bx = 0.0;
    let mut by = 0.0;
    let mut bn = 0.0;
    for (y, row) in rows.iter().enumerate() {
        let row = row.as_str().unwrap();
        for (x, ch) in row.chars().enumerate() {
            if ch == '.' || ch == '0' {
                continue;
            }
            bx += x as f64;
            by += y as f64;
            bn += 1.0;
            if ch == 'a' {
                mx += x as f64;
                my += y as f64;
                mn += 1.0;
            }
        }
    }
    (mx / mn, my / mn, bx / bn, by / bn, mn, bn)
}

fn sha256(path: &std::path::Path) -> String {
    let bytes = fs::read(path).unwrap();
    pxl_core::sha256_hex(&bytes)
}
