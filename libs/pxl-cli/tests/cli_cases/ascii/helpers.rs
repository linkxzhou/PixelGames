//! ASCII-case helpers.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cli_cases::common::{path_str, temp_dir as common_temp};
use crate::support::{pxl, repo_root};

pub(super) fn temp_dir(tag: &str) -> PathBuf {
    let dir = common_temp("ascii", tag);
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(super) fn s(path: &Path) -> &str {
    path_str(path)
}

pub(super) fn example(name: &str) -> PathBuf {
    repo_root().join("examples/ascii").join(name)
}

pub(super) fn import_slime(dir: &Path) -> PathBuf {
    let spec = dir.join("slime.ascii.json");
    fs::create_dir_all(dir.join("shared")).unwrap();
    let legend = dir.join("shared/p16.legend.json");
    fs::copy(example("shared/p16.legend.json"), &legend).unwrap();
    let text = dir.join("slime.ascii.txt");
    fs::copy(example("slime/slime.ascii.txt"), &text).unwrap();
    let (code, stdout, stderr) = pxl(&[
        "--json",
        "ascii",
        "import",
        s(&text),
        "--size",
        "32",
        "--cell",
        "fit",
        "--anchor",
        "bottom-center",
        "--palette",
        "p16",
        "--legend",
        "shared/p16.legend.json",
        "-o",
        s(&spec),
    ]);
    assert_eq!(code, 0, "import failed\nstdout={stdout}\nstderr={stderr}");
    spec
}
