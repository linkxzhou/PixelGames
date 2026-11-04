//! Read-only search over the prototype library for step 200.
//!
//! The model never gets file paths. It filters by view, kind, tier and size,
//! reads one entry by id, and asks whether a draft instance is compatible. All
//! reads stay under the approved library roots handed in by the host.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pxl_contract::prototype::{
    AssetInstance, Prototype, PrototypeKind, ReviewState, SurfaceRecipe, ViewKind,
};
use serde_json::{json, Value};

/// How many candidates a search returns. Small on purpose: the prompt carries
/// a handful of options, never the whole library.
pub const MAX_RESULTS: usize = 5;

#[derive(Debug, Default)]
pub struct Catalog {
    pub prototypes: BTreeMap<String, Prototype>,
    pub surfaces: BTreeMap<String, SurfaceRecipe>,
    pub problems: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Query {
    pub view: Option<ViewKind>,
    pub kind: Option<PrototypeKind>,
    /// Required tier edge: some tier of the prototype must fit within this box.
    pub max_size: Option<[u16; 2]>,
    /// Case-insensitive substring over id and region names.
    pub text: Option<String>,
    /// Include entries a human has not approved. Off by default.
    pub include_pending: bool,
}

fn read_dir_json<T: serde::de::DeserializeOwned>(
    dir: &Path,
    what: &str,
    problems: &mut Vec<String>,
) -> Vec<(String, T)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        match fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| {
            serde_json::from_slice::<T>(&bytes).map_err(|e| e.to_string())
        }) {
            Ok(value) => out.push((stem, value)),
            Err(e) => problems.push(format!("{what} '{stem}': {e}")),
        }
    }
    out
}

impl Catalog {
    /// Load every library root. Later roots never override earlier ones: a
    /// duplicate id is reported instead of silently shadowing.
    pub fn load(roots: &[PathBuf]) -> Self {
        let mut catalog = Catalog::default();
        for root in roots {
            for (stem, proto) in read_dir_json::<Prototype>(&root.join("prototype"), "prototype", &mut catalog.problems) {
                if stem != proto.id {
                    catalog.problems.push(format!("prototype file '{stem}.json' declares id '{}'", proto.id));
                    continue;
                }
                let errors = proto.validate();
                if !errors.is_empty() {
                    catalog.problems.extend(errors);
                    continue;
                }
                if catalog.prototypes.insert(proto.id.clone(), proto).is_some() {
                    catalog.problems.push(format!("duplicate prototype id '{stem}' across library roots"));
                }
            }
            for (stem, surface) in read_dir_json::<SurfaceRecipe>(&root.join("surface"), "surface", &mut catalog.problems) {
                if stem != surface.id {
                    catalog.problems.push(format!("surface file '{stem}.json' declares id '{}'", surface.id));
                    continue;
                }
                let errors = surface.validate();
                if !errors.is_empty() {
                    catalog.problems.extend(errors);
                    continue;
                }
                if catalog.surfaces.insert(surface.id.clone(), surface).is_some() {
                    catalog.problems.push(format!("duplicate surface id '{stem}' across library roots"));
                }
            }
        }
        catalog
    }

    pub fn is_empty(&self) -> bool {
        self.prototypes.is_empty() && self.surfaces.is_empty()
    }

    fn visible(review: ReviewState, include_pending: bool) -> bool {
        match review {
            ReviewState::Approved => true,
            ReviewState::Pending => include_pending,
            ReviewState::Rejected => false,
        }
    }

    /// Filter by hard constraints first, then order by id so the result is
    /// stable. Returns at most [`MAX_RESULTS`] compact summaries plus the total
    /// match count, so the model knows when it should narrow the query.
    pub fn search(&self, query: &Query) -> Value {
        let needle = query.text.as_ref().map(|t| t.to_lowercase());
        let mut matches: Vec<&Prototype> = self
            .prototypes
            .values()
            .filter(|p| Self::visible(p.review, query.include_pending))
            .filter(|p| query.view.map_or(true, |v| p.view == v))
            .filter(|p| query.kind.map_or(true, |k| p.kind == k))
            .filter(|p| {
                query.max_size.map_or(true, |[w, h]| {
                    p.size_tiers.iter().any(|t| t.size[0] <= w && t.size[1] <= h)
                })
            })
            .filter(|p| {
                needle.as_ref().map_or(true, |n| {
                    p.id.contains(n.as_str()) || p.regions.iter().any(|r| r.contains(n.as_str()))
                })
            })
            .collect();
        matches.sort_by(|a, b| a.id.cmp(&b.id));
        let total = matches.len();
        let shown: Vec<Value> = matches
            .into_iter()
            .take(MAX_RESULTS)
            .map(|p| {
                json!({
                    "id": p.id,
                    "version": p.version,
                    "kind": p.kind,
                    "view": p.view,
                    "tiers": p.size_tiers.iter().map(|t| json!({"id": t.id, "size": t.size})).collect::<Vec<_>>(),
                    "params": p.params.keys().collect::<Vec<_>>(),
                    "regions": p.regions,
                    "requires": p.requires,
                    "review": p.review,
                })
            })
            .collect();
        json!({ "total": total, "shown": shown.len(), "results": shown, "truncated": total > shown.len() })
    }

    /// Full record for one prototype or surface, with its digest so an
    /// instance can pin it. Unknown ids are an error, not an empty result.
    pub fn read(&self, id: &str, include_pending: bool) -> Result<Value, String> {
        if let Some(p) = self.prototypes.get(id) {
            if !Self::visible(p.review, include_pending) {
                return Err(format!("'{id}' is {:?} and not available for new specs", p.review));
            }
            return Ok(json!({ "entry": "prototype", "digest": p.digest(), "record": p }));
        }
        if let Some(s) = self.surfaces.get(id) {
            if !Self::visible(s.review, include_pending) {
                return Err(format!("'{id}' is {:?} and not available for new specs", s.review));
            }
            return Ok(json!({ "entry": "surface", "digest": s.digest(), "record": s }));
        }
        Err(format!("'{id}' is not in the prototype library"))
    }

    /// Check a draft instance against the library and report every problem at
    /// once, including entries a human has not approved.
    pub fn check_instance(&self, instance: &AssetInstance, include_pending: bool) -> Vec<String> {
        let Some(proto) = self.prototypes.get(&instance.prototype.id) else {
            return vec![format!("prototype '{}' is not in the library", instance.prototype.id)];
        };
        let mut problems = instance.validate_against(proto, &self.surfaces);
        if !Self::visible(proto.review, include_pending) {
            problems.push(format!("prototype '{}' is {:?}", proto.id, proto.review));
        }
        for surface_id in instance.surfaces.values() {
            if let Some(surface) = self.surfaces.get(surface_id) {
                if !Self::visible(surface.review, include_pending) {
                    problems.push(format!("surface '{surface_id}' is {:?}", surface.review));
                }
            }
        }
        for dep in &proto.requires {
            if !self.prototypes.contains_key(dep) && !self.surfaces.contains_key(dep) {
                problems.push(format!("dependency '{dep}' of '{}' is missing", proto.id));
            }
        }
        problems
    }
}

pub fn parse_view(text: &str) -> Result<ViewKind, String> {
    serde_json::from_value(Value::String(text.into()))
        .map_err(|_| format!("view must be top_down or side, got '{text}'"))
}

pub fn parse_kind(text: &str) -> Result<PrototypeKind, String> {
    serde_json::from_value(Value::String(text.into())).map_err(|_| {
        format!("kind must be one of building, plant, crop, terrain, prop, fence, scene_kit; got '{text}'")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_contract::prototype::{
        sha256_digest, Connector, DrawLayer, Footprint, LayerSplit, ParamRange, ParamValue,
        PrototypeSource, SizeTier, ASSET_INSTANCE_CONTRACT, PROTOTYPE_CONTRACT,
        SURFACE_RECIPE_CONTRACT,
    };

    fn proto(id: &str, kind: PrototypeKind, view: ViewKind, edge: u16, review: ReviewState) -> Prototype {
        Prototype {
            contract: PROTOTYPE_CONTRACT.into(),
            id: id.into(),
            version: 1,
            kind,
            view,
            size_tiers: vec![SizeTier { id: "m".into(), size: [edge, edge] }],
            params: BTreeMap::from([("tall".into(), ParamRange::Flag { default: false })]),
            regions: vec!["body".into()],
            connectors: vec![Connector { id: "c".into(), at: [0, 0], dir: "S".into(), width: 1, accepts: vec!["path".into()] }],
            footprint: Footprint { visual: [0, 0, 8, 8], sort_y: 1, collision: Vec::new(), interact: None },
            layers: vec![LayerSplit { id: "l".into(), regions: vec!["body".into()], layer: DrawLayer::Back }],
            variants: Vec::new(),
            source: PrototypeSource { kind: "ascii".into(), path: format!("{id}.txt"), digest: sha256_digest(id.as_bytes()) },
            requires: Vec::new(),
            license: "original".into(),
            review,
        }
    }

    fn surface(id: &str) -> SurfaceRecipe {
        SurfaceRecipe {
            contract: SURFACE_RECIPE_CONTRACT.into(),
            id: id.into(),
            version: 1,
            material_id: "stone".into(),
            applies_to: vec!["body".into()],
            recipe: "recolour".into(),
            params: BTreeMap::new(),
            scale_px: 2,
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn catalog() -> Catalog {
        let mut c = Catalog::default();
        for p in [
            proto("barn", PrototypeKind::Building, ViewKind::TopDown, 48, ReviewState::Approved),
            proto("cottage", PrototypeKind::Building, ViewKind::TopDown, 32, ReviewState::Approved),
            proto("palace_arch", PrototypeKind::Building, ViewKind::Side, 64, ReviewState::Approved),
            proto("oak", PrototypeKind::Plant, ViewKind::TopDown, 32, ReviewState::Approved),
            proto("draft_shed", PrototypeKind::Building, ViewKind::TopDown, 16, ReviewState::Pending),
            proto("old_hut", PrototypeKind::Building, ViewKind::TopDown, 16, ReviewState::Rejected),
        ] {
            c.prototypes.insert(p.id.clone(), p);
        }
        c.surfaces.insert("slate".into(), surface("slate"));
        c
    }

    #[test]
    fn search_applies_hard_filters_and_hides_unapproved_by_default() {
        let c = catalog();
        let out = c.search(&Query { view: Some(ViewKind::TopDown), kind: Some(PrototypeKind::Building), ..Default::default() });
        let ids: Vec<&str> = out["results"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["barn", "cottage"]);
        assert_eq!(out["total"], 2);

        let with_pending = c.search(&Query { include_pending: true, kind: Some(PrototypeKind::Building), view: Some(ViewKind::TopDown), ..Default::default() });
        assert_eq!(with_pending["total"], 3, "pending shows only on request");
        let all_ids = serde_json::to_string(&with_pending).unwrap();
        assert!(!all_ids.contains("old_hut"), "rejected entries never appear");
    }

    #[test]
    fn search_size_and_text_constraints() {
        let c = catalog();
        let small = c.search(&Query { max_size: Some([32, 32]), view: Some(ViewKind::TopDown), ..Default::default() });
        let ids: Vec<&str> = small["results"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["cottage", "oak"]);

        let by_text = c.search(&Query { text: Some("ARCH".into()), ..Default::default() });
        assert_eq!(by_text["total"], 1);
    }

    #[test]
    fn results_are_capped_and_flag_truncation() {
        let mut c = Catalog::default();
        for i in 0..9 {
            let p = proto(&format!("crate_{i}"), PrototypeKind::Prop, ViewKind::TopDown, 16, ReviewState::Approved);
            c.prototypes.insert(p.id.clone(), p);
        }
        let out = c.search(&Query::default());
        assert_eq!(out["total"], 9);
        assert_eq!(out["shown"], MAX_RESULTS);
        assert_eq!(out["truncated"], true);
    }

    #[test]
    fn read_returns_a_pinnable_digest_and_refuses_unknown_or_unavailable() {
        let c = catalog();
        let barn = c.read("barn", false).unwrap();
        assert_eq!(barn["entry"], "prototype");
        assert_eq!(barn["digest"], c.prototypes["barn"].digest());
        assert_eq!(c.read("slate", false).unwrap()["entry"], "surface");
        assert!(c.read("ghost", false).unwrap_err().contains("not in the prototype library"));
        assert!(c.read("draft_shed", false).is_err());
        assert!(c.read("draft_shed", true).is_ok());
        assert!(c.read("old_hut", true).is_err(), "rejected is never readable");
    }

    #[test]
    fn instance_check_reports_everything_in_one_pass() {
        let c = catalog();
        let barn = &c.prototypes["barn"];
        let mut inst = AssetInstance {
            contract: ASSET_INSTANCE_CONTRACT.into(),
            instance_id: "north_barn".into(),
            prototype: barn.reference(),
            size_tier: "m".into(),
            params: BTreeMap::from([("tall".into(), ParamValue::Flag(true))]),
            surfaces: BTreeMap::from([("body".into(), "slate".into())]),
            seed: 3,
            patches: Vec::new(),
        };
        assert_eq!(c.check_instance(&inst, false), Vec::<String>::new());

        inst.size_tier = "xl".into();
        inst.params.insert("colour".into(), ParamValue::Text("red".into()));
        inst.surfaces.insert("body".into(), "marble".into());
        let problems = c.check_instance(&inst, false).join("\n");
        for needle in ["unknown size tier", "unknown param 'colour'", "not in the catalog"] {
            assert!(problems.contains(needle), "missing '{needle}' in:\n{problems}");
        }
    }

    #[test]
    fn pending_prototype_is_flagged_for_a_new_instance() {
        let c = catalog();
        let shed = &c.prototypes["draft_shed"];
        let inst = AssetInstance {
            contract: ASSET_INSTANCE_CONTRACT.into(),
            instance_id: "shed_a".into(),
            prototype: shed.reference(),
            size_tier: "m".into(),
            params: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            seed: 1,
            patches: Vec::new(),
        };
        assert!(c.check_instance(&inst, false).iter().any(|p| p.contains("Pending")));
        assert!(c.check_instance(&inst, true).is_empty());
    }

    #[test]
    fn load_reads_json_files_and_rejects_mismatched_names() {
        let dir = std::env::temp_dir().join(format!("pxl-200-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("prototype")).unwrap();
        fs::create_dir_all(dir.join("surface")).unwrap();
        let good = proto("barn", PrototypeKind::Building, ViewKind::TopDown, 48, ReviewState::Approved);
        fs::write(dir.join("prototype/barn.json"), serde_json::to_string(&good).unwrap()).unwrap();
        // File name does not match the declared id.
        fs::write(dir.join("prototype/shed.json"), serde_json::to_string(&good).unwrap()).unwrap();
        fs::write(dir.join("surface/slate.json"), serde_json::to_string(&surface("slate")).unwrap()).unwrap();
        fs::write(dir.join("prototype/broken.json"), "{ not json").unwrap();

        let c = Catalog::load(&[dir.clone()]);
        assert_eq!(c.prototypes.len(), 1);
        assert_eq!(c.surfaces.len(), 1);
        let problems = c.problems.join("\n");
        assert!(problems.contains("declares id 'barn'"), "{problems}");
        assert!(problems.contains("prototype 'broken'"), "{problems}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_ids_across_roots_are_reported_not_shadowed() {
        let make = |tag: &str| {
            let dir = std::env::temp_dir().join(format!("pxl-200-dup-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("prototype")).unwrap();
            let p = proto("barn", PrototypeKind::Building, ViewKind::TopDown, 48, ReviewState::Approved);
            fs::write(dir.join("prototype/barn.json"), serde_json::to_string(&p).unwrap()).unwrap();
            dir
        };
        let (a, b) = (make("a"), make("b"));
        let c = Catalog::load(&[a.clone(), b.clone()]);
        assert!(c.problems.iter().any(|p| p.contains("duplicate prototype id 'barn'")), "{:?}", c.problems);
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn view_and_kind_parsers_reject_unknown_values() {
        assert!(parse_view("top_down").is_ok());
        assert!(parse_view("isometric").is_err());
        assert!(parse_kind("fence").is_ok());
        assert!(parse_kind("spaceship").is_err());
    }
}
