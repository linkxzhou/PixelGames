//! Content digests. Canonical JSON sorts object keys so digests do not depend
//! on field order or whitespace.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::Result;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

pub fn digest_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .map_err(|e| crate::error::HarnessError::io(format!("{}: {e}", path.display())))?;
    Ok(digest_bytes(&bytes))
}

pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

pub fn digest_value<T: Serialize>(v: &T) -> Result<String> {
    let value = serde_json::to_value(v)?;
    Ok(digest_bytes(canonical_json(&value).as_bytes()))
}

/// Digest of an ordered list of `(name, digest)` pairs.
pub fn digest_pairs<'a, I>(pairs: I) -> String
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut text = String::new();
    for (k, v) in pairs {
        text.push_str(k);
        text.push('\t');
        text.push_str(v);
        text.push('\n');
    }
    digest_bytes(text.as_bytes())
}

pub fn short(d: &str) -> String {
    d.trim_start_matches("sha256:").chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ignores_key_order() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":{"y":2,"x":[1,2]}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":[1,2],"y":2},"b":1}"#).unwrap();
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(digest_value(&a).unwrap(), digest_value(&b).unwrap());
    }
}
