//! JSON models for the `pxl` CLI. Key order comes from struct fields and
//! `BTreeMap`, so the same value always encodes the same bytes.

mod model;
mod ops;
mod parts;
mod recipe;
mod schema;

pub use model::*;
pub use ops::*;
pub use parts::*;
pub use recipe::*;
pub use schema::*;

use pxl_core::PxlError;
use serde::de::DeserializeOwned;

pub fn parse_json<T: DeserializeOwned>(text: &str, what: &str) -> Result<T, PxlError> {
    serde_json::from_str(text).map_err(|e| {
        PxlError::schema(alloc_msg(what, &e)).with_hint(
            "fix the JSON, then retry. Run `pxl schema <recipe|ops|pxl|palette>` to see the shape.",
        )
    })
}

fn alloc_msg(what: &str, e: &serde_json::Error) -> String {
    format!("{what} does not match the schema: {e}")
}

pub fn to_pretty<T: serde::Serialize>(value: &T) -> Result<String, PxlError> {
    let mut s = serde_json::to_string_pretty(value)
        .map_err(|e| PxlError::io(format!("json encode failed: {e}")))?;
    s.push('\n');
    Ok(s)
}
