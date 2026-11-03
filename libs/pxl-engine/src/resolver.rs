use std::collections::BTreeMap;
use std::path::Path;

use pxl_core::{Palette, PxlError, PxlResult};
use pxl_format::{parse_legend_file, parse_palette, LegendFile};
use pxl_gen::builtin_palette;

/// What a referenced file is. A directory-backed resolver searches a little
/// wider for palettes than for legends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Palette,
    Legend,
}

/// Source of the text files a spec refers to (`legend_ref`, palette files).
/// `Ok(None)` means "no such file"; `Err` means the file exists but cannot be
/// read.
pub trait Resolver {
    fn read_text(&self, name: &str, kind: ResourceKind) -> PxlResult<Option<String>>;
}

/// In-memory file table. Looks a name up exactly, then by its final path
/// component, so `../shared/p16.legend.json` finds `p16.legend.json`.
#[derive(Debug, Clone, Default)]
pub struct MemoryResolver {
    files: BTreeMap<String, String>,
}

impl MemoryResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_files(files: BTreeMap<String, String>) -> Self {
        Self { files }
    }

    pub fn insert(&mut self, name: impl Into<String>, text: impl Into<String>) {
        self.files.insert(name.into(), text.into());
    }
}

impl Resolver for MemoryResolver {
    fn read_text(&self, name: &str, _kind: ResourceKind) -> PxlResult<Option<String>> {
        if let Some(text) = self.files.get(name) {
            return Ok(Some(text.clone()));
        }
        let base = Path::new(name).file_name().and_then(|n| n.to_str());
        Ok(base.and_then(|base| {
            self.files
                .iter()
                .find(|(key, _)| {
                    Path::new(key.as_str()).file_name().and_then(|n| n.to_str()) == Some(base)
                })
                .map(|(_, text)| text.clone())
        }))
    }
}

/// A built-in id (`p16`, `tank`) or a palette file the resolver can find.
pub fn load_palette(path_or_id: &str, res: &dyn Resolver) -> PxlResult<Palette> {
    if !path_or_id.contains('/') && !path_or_id.ends_with(".json") {
        if let Ok(p) = builtin_palette(path_or_id) {
            return Ok(p);
        }
    }
    if let Some(text) = res.read_text(path_or_id, ResourceKind::Palette)? {
        let id = Path::new(path_or_id)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("palette");
        return parse_palette(&text, id);
    }
    if let Ok(p) = builtin_palette(path_or_id) {
        return Ok(p);
    }
    Err(
        PxlError::semantic(format!("palette '{path_or_id}' was not found"))
            .with_hint("pass a .palette.json path or a built-in id such as tank or p16"),
    )
}

pub(crate) fn load_legend_file(
    name: Option<&str>,
    res: &dyn Resolver,
) -> PxlResult<Option<LegendFile>> {
    let Some(name) = name else {
        return Ok(None);
    };
    let Some(text) = res.read_text(name, ResourceKind::Legend)? else {
        return Err(
            PxlError::io(format!("cannot read legend file '{name}'")).with_hint(
                "the path is relative to the ascii spec; pass the *.legend.json you wrote",
            ),
        );
    };
    Ok(Some(parse_legend_file(&text)?))
}
