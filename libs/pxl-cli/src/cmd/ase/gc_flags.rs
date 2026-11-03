//! GC-2 flag helpers (split-*/ignore-layer/slice/padding).

use pxl_ase::doc::{Slice, Sprite};

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
