// Palette family orchestration (P04): resolve the families a visual-intent/v2
// declares, verify their palettes against the recorded digests, and give the
// build one canonical view — a family id plus its ordered colour array — so
// sheets group strictly by family and never quantise across families.
//
// The registry keeps the raw palette bytes per family so callers can record
// `source_digest` (byte digest, `digest`) separately from the semantic digest
// the intent already carries. Two families with identical colours stay two
// families: identity is the id, never the colour array.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::visual::{PaletteFamily, VisualIntent};

/// Capability id announced when a build groups sheets by palette family.
pub const PALETTE_FAMILY_CAPABILITY: &str = "palette-family/v1";

/// One resolved family: its id, the palette file it came from, the raw bytes
/// digest (what the intent's `digest` records) and the parsed colour array.
/// The semantic digest is carried through untouched so colour-equal families
/// never lose their distinct meaning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFamily {
    pub id: String,
    pub palette_path: String,
    /// sha256 over the palette file bytes; matches the intent's `digest`.
    pub digest: String,
    /// sha256 over the semantic identity (ramps, cycles, special indices);
    /// matches the intent's `semantic_digest`.
    pub semantic_digest: String,
    pub transparent_index: u8,
    /// Ordered colour array exactly as the palette file lists it. Sheets pack
    /// per family; two families never share a sheet even when colours match.
    pub colors: Vec<String>,
    /// Named ramps, cycles and special indices parsed from the palette file,
    /// kept separately from the colours so identical colours with different
    /// semantics stay distinct.
    pub ramps: BTreeMap<String, Vec<u8>>,
    pub cycles: BTreeMap<String, Vec<u8>>,
    pub special: BTreeMap<String, u8>,
}

/// All families of one intent, keyed by family id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FamilyRegistry {
    families: BTreeMap<String, ResolvedFamily>,
}

impl FamilyRegistry {
    /// Resolve every family the intent declares. `root` is the directory the
    /// intent's relative palette paths resolve against (the step-200 spec
    /// root). Files are read through `read_bytes` so tests can inject.
    pub fn resolve(intent: &VisualIntent, root: &Path) -> Result<Self, String> {
        let mut families = BTreeMap::new();
        for family in &intent.palette_families {
            let resolved = resolve_one(family, root)?;
            families.insert(family.id.clone(), resolved);
        }
        Ok(Self { families })
    }

    pub fn get(&self, id: &str) -> Option<&ResolvedFamily> {
        self.families.get(id)
    }

    pub fn ids(&self) -> Vec<String> {
        self.families.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    pub fn len(&self) -> usize {
        self.families.len()
    }

    /// Stable summary for manifests and reports: id, both digests and the
    /// colour count. Ordered by family id, so the input order of the intent
    /// never changes the serialised result.
    pub fn summary(&self) -> Vec<serde_json::Value> {
        self.families
            .values()
            .map(|family| {
                serde_json::json!({
                    "id": family.id,
                    "digest": family.digest,
                    "semantic_digest": family.semantic_digest,
                    "colors": family.colors.len(),
                    "transparent_index": family.transparent_index,
                })
            })
            .collect()
    }
}

fn resolve_one(family: &PaletteFamily, root: &Path) -> Result<ResolvedFamily, String> {
    let path = resolve_palette_path(root, &family.palette)?;
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "palette family '{}': cannot read {}: {e}",
            family.id,
            path.display()
        )
    })?;
    let digest = crate::digest_bytes(&bytes);
    if digest != family.digest {
        return Err(format!(
            "palette family '{}' digest mismatch: {} is {}, the intent froze {}",
            family.id,
            family.palette,
            digest,
            family.digest
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("palette family '{}': {} is not UTF-8", family.id, family.palette))?;
    let palette: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "palette family '{}': {} is not valid JSON: {e}",
            family.id, family.palette
        )
    })?;
    let semantic = semantic_digest(&palette)?;
    if semantic != family.semantic_digest {
        return Err(format!(
            "palette family '{}' semantic digest mismatch: {} normalises to {}, the intent froze {}",
            family.id, family.palette, semantic, family.semantic_digest
        ));
    }
    let colors = palette
        .get("colors")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("palette family '{}' has no colors array", family.id))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("palette family '{}' has a non-string color", family.id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if colors.is_empty() || colors.len() > crate::policy::MAX_TEXT_COLORS {
        return Err(format!(
            "palette family '{}' has {} colors; must be 1..={}",
            family.id,
            colors.len(),
            crate::policy::MAX_TEXT_COLORS
        ));
    }
    if family.transparent_index as usize >= colors.len() {
        return Err(format!(
            "palette family '{}' transparent_index {} is outside its {} colors",
            family.id,
            family.transparent_index,
            colors.len()
        ));
    }
    let ramps = parse_index_map(palette.get("ramps"));
    let cycles = parse_index_map(palette.get("cycles"));
    let mut special = BTreeMap::new();
    if let Some(value) = palette.get("special").and_then(|value| value.as_object()) {
        for (key, entry) in value {
            if let Some(index) = entry.as_u64().and_then(|index| u8::try_from(index).ok()) {
                special.insert(key.clone(), index);
            }
        }
    }
    Ok(ResolvedFamily {
        id: family.id.clone(),
        palette_path: family.palette.clone(),
        digest,
        semantic_digest: family.semantic_digest.clone(),
        transparent_index: family.transparent_index,
        colors,
        ramps,
        cycles,
        special,
    })
}

/// Palette paths are intent-relative. Absolute paths and `..` segments are
/// refused so a family can never point outside the spec root.
fn resolve_palette_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err("palette family palette path is empty".into());
    }
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(format!(
            "palette family palette path '{relative}' must be relative to the spec root"
        ));
    }
    for component in candidate.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!(
                "palette family palette path '{relative}' must not escape the spec root"
            ));
        }
    }
    Ok(root.join(candidate))
}

/// sha256 over the canonical semantic projection: colours dropped, only the
/// meaning (ramps, cycles, special indices) kept. Two palettes with identical
/// semantics but different hex spellings normalise to the same digest, and a
/// recolour keeps the same colours here while the byte digest changes.
fn semantic_digest(palette: &serde_json::Value) -> Result<String, String> {
    let semantic = serde_json::json!({
        "ramps": palette.get("ramps").cloned().unwrap_or(serde_json::Value::Null),
        "cycles": palette.get("cycles").cloned().unwrap_or(serde_json::Value::Null),
        "special": palette.get("special").cloned().unwrap_or(serde_json::Value::Null),
    });
    let canonical =
        serde_json::to_vec(&semantic).map_err(|e| format!("semantic projection: {e}"))?;
    Ok(crate::digest_bytes(&canonical))
}

fn parse_index_map(value: Option<&serde_json::Value>) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let Some(entries) = value.and_then(|value| value.as_object()) else {
        return out;
    };
    for (key, entry) in entries {
        let indices = match entry {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_u64().and_then(|index| u8::try_from(index).ok()))
                .collect::<Vec<_>>(),
            serde_json::Value::Object(map) => {
                // Palette cycle entries carry `indices` plus timing metadata.
                map.get("indices")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                item.as_u64().and_then(|index| u8::try_from(index).ok())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        out.insert(key.clone(), indices);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pxl-family-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_palette(root: &Path, name: &str, colors: &[&str], ramps: serde_json::Value) -> String {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "v": 1,
            "id": name,
            "colors": colors,
            "ramps": ramps,
            "cycles": {},
            "special": {"transparent": 0, "outline": 1, "flash": 2},
        }))
        .unwrap();
        std::fs::write(root.join(name), text.as_bytes()).unwrap();
        crate::digest_bytes(text.as_bytes())
    }

    fn intent_with(family: PaletteFamily) -> VisualIntent {
        let mut intent = crate::visual::tests::valid_intent();
        intent.contract = crate::visual::VISUAL_INTENT_CONTRACT_V2.to_string();
        intent.palette_families = vec![family];
        intent
    }

    fn family_decl(id: &str, path: &str, digest: &str, semantic: &str) -> PaletteFamily {
        PaletteFamily {
            id: id.into(),
            palette: path.into(),
            digest: digest.into(),
            semantic_digest: semantic.into(),
            transparent_index: 0,
        }
    }

    #[test]
    fn resolves_family_and_keeps_semantics() {
        let root = temp_root("ok");
        let ramps = serde_json::json!({"main": [1, 2, 3]});
        let digest = write_palette(&root, "palette.json", &["#00000000", "#111111", "#222222"], ramps);
        let semantic = semantic_digest(&serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(root.join("palette.json")).unwrap(),
        ).unwrap())
        .unwrap();
        let intent = intent_with(family_decl("environment", "palette.json", &digest, &semantic));
        let registry = FamilyRegistry::resolve(&intent, &root).unwrap();
        let family = registry.get("environment").unwrap();
        assert_eq!(family.colors.len(), 3);
        assert_eq!(family.ramps.get("main").unwrap(), &[1, 2, 3]);
        assert_eq!(registry.summary().len(), 1);
    }

    #[test]
    fn stale_digest_fails_with_family_id() {
        let root = temp_root("stale");
        write_palette(&root, "palette.json", &["#00000000", "#111111"], serde_json::json!({"main": [1]}));
        let intent = intent_with(family_decl("environment", "palette.json", "sha256:0000000000000000000000000000000000000000000000000000000000000000", "sha256:0000000000000000000000000000000000000000000000000000000000000000"));
        let error = FamilyRegistry::resolve(&intent, &root).unwrap_err();
        assert!(error.contains("environment"), "{error}");
        assert!(error.contains("digest mismatch"), "{error}");
    }

    #[test]
    fn semantic_digest_mismatch_fails() {
        let root = temp_root("sem");
        let digest = write_palette(&root, "palette.json", &["#00000000", "#111111"], serde_json::json!({"main": [1]}));
        let intent = intent_with(family_decl("environment", "palette.json", &digest, "sha256:0000000000000000000000000000000000000000000000000000000000000000"));
        let error = FamilyRegistry::resolve(&intent, &root).unwrap_err();
        assert!(error.contains("semantic digest mismatch"), "{error}");
    }

    #[test]
    fn path_escape_is_refused() {
        let root = temp_root("escape");
        let intent = intent_with(family_decl("environment", "../outside.json", "sha256:0000000000000000000000000000000000000000000000000000000000000000", "sha256:0000000000000000000000000000000000000000000000000000000000000000"));
        let error = FamilyRegistry::resolve(&intent, &root).unwrap_err();
        assert!(error.contains("must not escape"), "{error}");
    }

    #[test]
    fn identical_colors_keep_distinct_identity() {
        let root = temp_root("twins");
        let ramps = serde_json::json!({"main": [1, 2]});
        let digest = write_palette(&root, "a.json", &["#00000000", "#111111", "#222222"], ramps.clone());
        let semantic = semantic_digest(&serde_json::json!({"ramps": ramps, "cycles": {}, "special": {"transparent": 0, "outline": 1, "flash": 2}})).unwrap();
        let mut intent = intent_with(family_decl("alpha", "a.json", &digest, &semantic));
        intent.palette_families.push(family_decl("beta", "a.json", &digest, &semantic));
        let registry = FamilyRegistry::resolve(&intent, &root).unwrap();
        assert_eq!(registry.len(), 2);
        // Same colours, two ids: the registry never merges them.
        assert_ne!(registry.get("alpha").unwrap().id, registry.get("beta").unwrap().id);
        assert_eq!(registry.get("alpha").unwrap().colors, registry.get("beta").unwrap().colors);
    }
}
