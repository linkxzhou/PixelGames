use super::*;

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::helpers::{find_pxl_in, normalize};
use super::repair::check_failed;
use crate::process::ProcOutput;

fn test_adapter() -> PxlAdapter {
    let cfg = crate::config::Config::default();
    PxlAdapter::discover(&cfg).unwrap_or_else(|_| PxlAdapter::unavailable())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pxl-adapter-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn find_pxl_prefers_sibling_over_path() {
    let sib = scratch("sib");
    let other = scratch("other");
    let name = format!("step-300{}", std::env::consts::EXE_SUFFIX);
    fs::write(sib.join(&name), b"x").unwrap();
    fs::write(other.join(&name), b"x").unwrap();
    let path = std::env::join_paths([&other]).unwrap();
    assert_eq!(find_pxl_in(Some(&sib), &path), Some(sib.join(&name)));
    assert_eq!(find_pxl_in(None, &path), Some(other.join(&name)));
    let _ = fs::remove_dir_all(&sib);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn find_pxl_splits_path_with_platform_separator() {
    let a = scratch("pa");
    let b = scratch("pb");
    let name = format!("step-300{}", std::env::consts::EXE_SUFFIX);
    fs::write(b.join(&name), b"x").unwrap();
    // join_paths uses ':' on unix and ';' on windows, so a Windows drive
    // letter such as C:\bin is never cut in half.
    let path = std::env::join_paths([&a, &b]).unwrap();
    assert_eq!(find_pxl_in(None, &path), Some(b.join(&name)));
    assert_eq!(find_pxl_in(None, std::ffi::OsStr::new("")), None);
    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

#[test]
fn discover_and_version() {
    let a = test_adapter();
    if a.bin.as_os_str().len() > 0 && a.bin.is_file() {
        assert!(a.version().starts_with("0."));
    }
}

#[test]
fn list_and_schema_shapes() {
    let a = test_adapter();
    if !a.bin.is_file() {
        return; // pxl not available in this environment
    }
    let res = a.list("generators").unwrap();
    assert!(res.ok, "{}", res.message);
    let v: Value = serde_json::from_str(&res.stdout).unwrap();
    let items: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(items.contains(&"shapes"));
    let schema = a.schema("recipe").unwrap();
    assert!(schema.get("properties").is_some());
}

#[test]
fn repair_classes() {
    assert_eq!(repair_class("E_SCHEMA"), RepairClass::CandidateRevision);
    assert_eq!(repair_class("E_QA"), RepairClass::AssetFix);
    assert_eq!(repair_class("E_EXISTS"), RepairClass::IsolationConflict);
    assert_eq!(repair_class("INFRA_TIMEOUT"), RepairClass::Infrastructure);
}

fn proc(stdout: &str, exit: i32) -> ProcOutput {
    ProcOutput {
        argv: vec!["pxl".into()],
        exit_code: Some(exit),
        stdout: stdout.into(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
        elapsed_ms: 12,
    }
}

#[test]
fn normalize_extracts_hint_and_path_from_error_envelope() {
    let out = proc(
        r#"{"ok":false,"error":{"code":"E_SCHEMA","message":"macro pulse_glow has no argument 'steps'","path":"/macros","hint":"pulse_glow takes: ramp"}}"#,
        3,
    );
    let res = normalize(&out, &vec!["pxl".into()]).unwrap();
    assert!(!res.ok);
    assert_eq!(res.error_code, "E_SCHEMA");
    assert_eq!(res.exit_code, Some(3));
    assert_eq!(res.hint.as_deref(), Some("pulse_glow takes: ramp"));
    assert_eq!(res.path.as_deref(), Some("/macros"));
    assert!(res.message.contains("pulse_glow"));
}

#[test]
fn checks_keep_value_and_limit_objects() {
    // Matches the CLI `checks_json` output shape: {id, pass, value, limit}.
    let out = proc(
        r#"{"ok":false,"error":{"code":"E_QA","message":"1 check(s) failed"},"checks":[{"id":"anim.exp.diff_0_1","pass":false,"value":"42.96%","limit":"at least 1%, and at most 35% unless a 1px shift explains it"},{"id":"palette.alpha","pass":true,"value":"opaque-or-clear","limit":"alpha 0 or 255, index 0 clear"}]}"#,
        5,
    );
    let res = normalize(&out, &vec!["pxl".into()]).unwrap();
    let checks = res.checks();
    assert_eq!(checks.len(), 2);
    let failed = render_failed_checks(&checks, 8);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].contains("anim.exp.diff_0_1"), "{failed:?}");
    assert!(failed[0].contains("42.96%"), "{failed:?}");
    assert!(failed[0].contains("35%"), "{failed:?}");
    assert!(
        !failed[0].contains("failed"),
        "must not degrade to bare 'failed': {failed:?}"
    );
}

#[test]
fn failed_checks_ignore_advisories_and_accept_legacy_reports() {
    let checks = serde_json::json!([
        {"id":"warning", "pass":false, "severity":"warning"},
        {"id":"info", "pass":false, "severity":"info"},
        {"id":"error", "pass":false, "severity":"error", "value":1},
        {"id":"legacy", "pass":false, "value":2},
        {"id":"passed", "pass":true, "severity":"error"}
    ])
    .as_array()
    .unwrap()
    .clone();
    let rendered = render_failed_checks(&checks, 8);
    assert_eq!(rendered.len(), 2);
    assert!(rendered[0].starts_with("error:"));
    assert!(rendered[1].starts_with("legacy:"));
    let result = normalize(
        &proc(
            r#"{"ok":false,"error":{"code":"E_QA","message":"errors"}}"#,
            5,
        ),
        &[],
    )
    .unwrap();
    let built = PxlBuildOutput {
        result,
        outputs: vec![],
        sheet_hash: None,
        checks,
        file_digests: vec![],
    };
    assert_eq!(built.failed_checks().len(), 2);
}

#[test]
fn validate_contract_keeps_value_and_limit() {
    let a = test_adapter();
    if !a.bin.is_file() {
        return; // pxl not available in this environment
    }
    // Build the tank example; its sheet must validate cleanly. Then run a
    // real `pxl validate` and confirm the checks array carries value/limit.
    // This crate lives in libs/, so the workspace examples are two
    // levels above the manifest directory.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/tank");
    let out = std::env::temp_dir().join(format!("pxl_h_val_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    a.build(&root.join("tank.recipe.json"), &out, None, Some(&root))
        .unwrap();
    let res = a.validate(&out.join("tank.sheet.json"), "default").unwrap();
    assert!(
        res.ok,
        "{} {} hint={:?}",
        res.error_code, res.message, res.hint
    );
    for check in res.checks() {
        assert!(check.get("id").is_some(), "check missing id: {check}");
        assert!(check.get("pass").is_some(), "check missing pass: {check}");
        assert!(
            check.get("severity").is_some(),
            "check missing severity: {check}"
        );
        assert!(
            check.get("samples").is_some(),
            "check missing samples: {check}"
        );
        assert!(!check_failed(&check), "unexpected blocking check: {check}");
        assert!(check.get("message").is_none() && check.get("detail").is_none());
    }
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn build_normalizes_outputs_and_hashes() {
    let a = test_adapter();
    if !a.bin.is_file() {
        return;
    }
    // This crate lives in libs/, so the workspace examples are two
    // levels above the manifest directory.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/tank");
    let out = std::env::temp_dir().join(format!("pxl_h_adapter_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let built = a
        .build(&root.join("tank.recipe.json"), &out, None, Some(&root))
        .unwrap();
    assert!(built.qa_ok(), "{}", built.result.message);
    assert!(built.outputs.iter().any(|o| o == "tank.sheet.png"));
    assert!(built
        .file_digests
        .iter()
        .any(|(n, _)| n == "tank.sheet.png"));
    // Deterministic rebuild.
    let out2 = out.with_extension("again");
    let _ = fs::remove_dir_all(&out2);
    let built2 = a
        .build(&root.join("tank.recipe.json"), &out2, None, Some(&root))
        .unwrap();
    let d1: Vec<_> = built.file_digests.clone();
    let d2: Vec<_> = built2.file_digests.clone();
    for (n, d) in &d1 {
        let match_d = d2.iter().find(|(n2, _)| n2 == n);
        assert_eq!(
            match_d.map(|(_, d2)| d2),
            Some(d),
            "digest mismatch for {n}"
        );
    }
}
