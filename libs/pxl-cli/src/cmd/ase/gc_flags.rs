//! GC-2 flag helpers (split-*/ignore-layer/slice/padding).

use pxl_ase::doc::{Slice, Sprite};

/// Sheet pack padding from `--padding` and official border/shape/inner.
/// If any official flag is set, use the **max** of provided values (including
/// `--padding` when >0); else `--padding`. Full triad semantics need richer pack.
pub(crate) fn effective_sheet_padding(
    padding: u32,
    border: Option<u32>,
    shape: Option<u32>,
    inner: Option<u32>,
) -> u32 {
    let mut vals = Vec::new();
    if border.is_some() || shape.is_some() || inner.is_some() {
        if padding > 0 {
            vals.push(padding);
        }
        vals.extend(border);
        vals.extend(shape);
        vals.extend(inner);
        return vals.into_iter().max().unwrap_or(0);
    }
    padding
}

pub(crate) fn apply_ignore_layers(sprite: &mut Sprite, names: &[String]) {
    for layer in &mut sprite.layers {
        if names.iter().any(|n| n == &layer.name) {
            layer.flags.visible = false;
        }
    }
}

pub(crate) fn find_slice<'a>(sprite: &'a Sprite, name: &str) -> Option<&'a Slice> {
    sprite.slices.iter().find(|s| s.name == name)
}

/// If `path` lacks `{layer}`/`{tag}`, insert `_{layer}` / `_{tag}` before the extension.
pub(crate) fn ensure_format_token(path: &str, brace_token: &str) -> String {
    if path.contains(brace_token) {
        return path.to_string();
    }
    let insert = format!("_{brace_token}");
    if let Some(dot) = path.rfind('.') {
        format!("{}{}{}", &path[..dot], insert, &path[dot..])
    } else {
        format!("{path}{insert}")
    }
}
