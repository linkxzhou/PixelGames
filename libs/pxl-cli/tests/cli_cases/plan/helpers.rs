//! Plan-case helpers.

use std::fs;
use std::path::PathBuf;

use crate::cli_cases::common::temp_dir as common_temp;
use crate::support::repo_root;

pub(super) fn temp_dir(tag: &str) -> PathBuf {
    common_temp("plan", tag)
}

pub(super) fn bullet_dir() -> PathBuf {
    repo_root().join("examples/static_bullet")
}

pub(super) fn install_visual_contract(root: &PathBuf, asset_dir: &str) {
    let fixture = repo_root().join(
        "libs/pxl-cli/tests/fixtures/visual-contract",
    );
    for name in ["visual-intent.snapshot.json", "visual-plan.json"] {
        fs::copy(fixture.join(name), root.join(name)).unwrap();
    }
    fs::copy(
        fixture.join("static_bullet.shapes.json"),
        root.join(asset_dir).join("static_bullet.shapes.json"),
    )
    .unwrap();
}
