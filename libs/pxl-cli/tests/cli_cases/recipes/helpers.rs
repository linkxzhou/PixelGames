//! Recipe-case helpers.

use std::path::Path;

use serde_json::Value;

use crate::cli_cases::common::sha256_file;

pub(super) fn centroids(rows: &[Value]) -> (f64, f64, f64, f64, f64, f64) {
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

pub(super) fn sha256(path: &Path) -> String {
    sha256_file(path)
}
