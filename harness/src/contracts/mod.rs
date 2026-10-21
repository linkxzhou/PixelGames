//! Domain contracts. JSON is the source of truth; Markdown views are derived.

pub mod assets;
pub mod design;

use serde::{Deserialize, Serialize};

/// Everything versioned in the design package shares this header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractHeader {
    pub schema_version: u32,
}

pub fn stable_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
