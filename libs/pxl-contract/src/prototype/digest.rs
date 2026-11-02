//! Prototype library contracts: prototypes, surface recipes, asset instances
//! and the catalog lock that freezes the build input closure.
//!
//! Shape, surface, and instance are separate records so structure and material
//! vary independently. Nothing here produces pixels; step 300 expands a frozen
//! instance deterministically.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};


pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut hex = String::with_capacity(71);
    hex.push_str("sha256:");
    for byte in out {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Ids are lowercase ascii words joined by underscores, so they stay stable
/// as file names and as cross-reference keys.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !value.starts_with(|c: char| c.is_ascii_digit())
}

