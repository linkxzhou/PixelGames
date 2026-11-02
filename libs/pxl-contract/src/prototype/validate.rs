//! Prototype library contracts: prototypes, surface recipes, asset instances
//! and the catalog lock that freezes the build input closure.
//!
//! Shape, surface, and instance are separate records so structure and material
//! vary independently. Nothing here produces pixels; step 300 expands a frozen
//! instance deterministically.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};


use super::types::*;
use super::digest::{sha256_digest, valid_digest, valid_id};

impl Prototype {
    /// Digest over the serialized record. Field order is fixed by the struct
    /// and all maps are ordered, so equal content gives an equal digest.
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("prototype serializes"))
    }

    pub fn reference(&self) -> PrototypeRef {
        PrototypeRef { id: self.id.clone(), version: self.version, digest: self.digest() }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != PROTOTYPE_CONTRACT {
            errors.push(format!("prototype '{who}': contract must be {PROTOTYPE_CONTRACT}"));
        }
        if !valid_id(&self.id) {
            errors.push(format!("prototype '{who}': id must be lowercase ascii words joined by '_'"));
        }
        if self.version == 0 {
            errors.push(format!("prototype '{who}': version must be at least 1"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("prototype '{who}': license is required"));
        }
        if !valid_digest(&self.source.digest) {
            errors.push(format!("prototype '{who}': source digest must be sha256"));
        }
        if !matches!(self.source.kind.as_str(), "ascii" | "shapes") || self.source.path.trim().is_empty() {
            errors.push(format!("prototype '{who}': source must be ascii or shapes with a path"));
        }

        let mut max_h = 0i32;
        let mut tier_ids = BTreeSet::new();
        if self.size_tiers.is_empty() {
            errors.push(format!("prototype '{who}': at least one size tier is required"));
        }
        for tier in &self.size_tiers {
            if !tier_ids.insert(tier.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate size tier '{}'", tier.id));
            }
            if tier.size.contains(&0) || tier.size.iter().any(|edge| *edge > MAX_TIER_EDGE) {
                errors.push(format!("prototype '{who}': tier '{}' must be 1..={MAX_TIER_EDGE} per edge", tier.id));
            }
            max_h = max_h.max(i32::from(tier.size[1]));
        }

        for (name, range) in &self.params {
            if !valid_id(name) {
                errors.push(format!("prototype '{who}': param name '{name}' is not a valid id"));
            }
            errors.extend(range.problems(name).into_iter().map(|e| format!("prototype '{who}': {e}")));
        }

        let mut region_ids = BTreeSet::new();
        if self.regions.is_empty() {
            errors.push(format!("prototype '{who}': at least one region is required"));
        }
        for region in &self.regions {
            if !valid_id(region) || !region_ids.insert(region.as_str()) {
                errors.push(format!("prototype '{who}': region '{region}' is invalid or duplicated"));
            }
        }

        let mut connector_ids = BTreeSet::new();
        for connector in &self.connectors {
            if !connector_ids.insert(connector.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate connector '{}'", connector.id));
            }
            if !matches!(connector.dir.as_str(), "N" | "E" | "S" | "W") {
                errors.push(format!("prototype '{who}': connector '{}' dir must be N/E/S/W", connector.id));
            }
            if connector.width == 0 || connector.accepts.is_empty() {
                errors.push(format!("prototype '{who}': connector '{}' needs a width and accepted kinds", connector.id));
            }
        }

        if self.footprint.visual[2] <= 0 || self.footprint.visual[3] <= 0 {
            errors.push(format!("prototype '{who}': visual bounds must have positive size"));
        }
        if max_h > 0 && (self.footprint.sort_y < 0 || self.footprint.sort_y > max_h) {
            errors.push(format!("prototype '{who}': sort_y must lie within the tallest tier"));
        }
        if self.footprint.collision.iter().any(|r| r[2] <= 0 || r[3] <= 0) {
            errors.push(format!("prototype '{who}': collision boxes must have positive size"));
        }

        let mut layer_ids = BTreeSet::new();
        for layer in &self.layers {
            if !layer_ids.insert(layer.id.as_str()) {
                errors.push(format!("prototype '{who}': duplicate layer '{}'", layer.id));
            }
            if layer.regions.is_empty() || layer.regions.iter().any(|r| !region_ids.contains(r.as_str())) {
                errors.push(format!("prototype '{who}': layer '{}' references an unknown region", layer.id));
            }
        }

        for (n, rule) in self.variants.iter().enumerate() {
            match self.params.get(&rule.param) {
                None => errors.push(format!("prototype '{who}': variant {n} uses unknown param '{}'", rule.param)),
                Some(range) => {
                    if let Some(message) = range.reject(&rule.param, &rule.equals) {
                        errors.push(format!("prototype '{who}': variant {n}: {message}"));
                    }
                }
            }
            if !region_ids.contains(rule.region.as_str()) {
                errors.push(format!("prototype '{who}': variant {n} targets unknown region '{}'", rule.region));
            }
            if rule.rows.is_empty() || rule.rows.iter().any(|r| r.is_empty()) {
                errors.push(format!("prototype '{who}': variant {n} needs non-empty rows"));
            }
        }

        if self.requires.iter().any(|r| r == &self.id) {
            errors.push(format!("prototype '{who}': cannot require itself"));
        }
        errors
    }
}

impl SurfaceRecipe {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("surface serializes"))
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.id;
        if self.contract != SURFACE_RECIPE_CONTRACT {
            errors.push(format!("surface '{who}': contract must be {SURFACE_RECIPE_CONTRACT}"));
        }
        if !valid_id(&self.id) || !valid_id(&self.material_id) {
            errors.push(format!("surface '{who}': id and material_id must be valid ids"));
        }
        if self.version == 0 {
            errors.push(format!("surface '{who}': version must be at least 1"));
        }
        if self.applies_to.is_empty() {
            errors.push(format!("surface '{who}': applies_to must name at least one region kind"));
        }
        if self.recipe.trim().is_empty() {
            errors.push(format!("surface '{who}': recipe name is required"));
        }
        if self.scale_px == 0 {
            errors.push(format!("surface '{who}': scale_px must be positive"));
        }
        if self.license.trim().is_empty() {
            errors.push(format!("surface '{who}': license is required"));
        }
        errors
    }
}

impl AssetInstance {
    pub fn digest(&self) -> String {
        sha256_digest(&serde_json::to_vec(self).expect("instance serializes"))
    }

    /// Checks the instance against the exact prototype it points to.
    /// `surfaces` maps surface id -> recipe, so missing recipes are reported.
    pub fn validate_against(
        &self,
        prototype: &Prototype,
        surfaces: &BTreeMap<String, SurfaceRecipe>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let who = &self.instance_id;
        if self.contract != ASSET_INSTANCE_CONTRACT {
            errors.push(format!("instance '{who}': contract must be {ASSET_INSTANCE_CONTRACT}"));
        }
        if !valid_id(&self.instance_id) {
            errors.push(format!("instance '{who}': instance_id must be a valid id"));
        }
        if self.prototype.id != prototype.id || self.prototype.version != prototype.version {
            errors.push(format!(
                "instance '{who}': points at {}@{} but prototype is {}@{}",
                self.prototype.id, self.prototype.version, prototype.id, prototype.version
            ));
        } else if self.prototype.digest != prototype.digest() {
            errors.push(format!("instance '{who}': prototype digest does not match the library entry"));
        }
        if !prototype.size_tiers.iter().any(|t| t.id == self.size_tier) {
            errors.push(format!("instance '{who}': unknown size tier '{}'", self.size_tier));
        }
        for (name, value) in &self.params {
            match prototype.params.get(name) {
                None => errors.push(format!("instance '{who}': unknown param '{name}'")),
                Some(range) => errors.extend(range.reject(name, value).map(|e| format!("instance '{who}': {e}"))),
            }
        }
        for (region, surface_id) in &self.surfaces {
            if !prototype.regions.contains(region) {
                errors.push(format!("instance '{who}': surface set on unknown region '{region}'"));
            }
            if !surfaces.contains_key(surface_id) {
                errors.push(format!("instance '{who}': surface '{surface_id}' is not in the catalog"));
            }
        }
        for patch in &self.patches {
            if !prototype.regions.contains(&patch.region) {
                errors.push(format!("instance '{who}': patch targets unknown region '{}'", patch.region));
            }
            if patch.rows.is_empty() {
                errors.push(format!("instance '{who}': patch for '{}' has no rows", patch.region));
            }
        }
        errors
    }
}

impl CatalogLock {
    pub fn compute_digest(entries: &[LockedEntry]) -> String {
        sha256_digest(&serde_json::to_vec(entries).expect("entries serialize"))
    }

    /// Sorts entries by id so the lock does not depend on discovery order.
    pub fn seal(mut entries: Vec<LockedEntry>) -> Self {
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        let digest = Self::compute_digest(&entries);
        Self { contract: CATALOG_LOCK_CONTRACT.into(), entries, digest }
    }

    /// Verifies ordering, digests, dependency closure and (optionally) review state.
    pub fn verify(&self, require_approved: bool) -> Vec<String> {
        let mut errors = Vec::new();
        if self.contract != CATALOG_LOCK_CONTRACT {
            errors.push(format!("catalog lock: contract must be {CATALOG_LOCK_CONTRACT}"));
        }
        let mut ids = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for entry in &self.entries {
            if !ids.insert(entry.id.as_str()) {
                errors.push(format!("catalog lock: duplicate entry '{}'", entry.id));
            }
            if previous.is_some_and(|p| p >= entry.id.as_str()) {
                errors.push(format!("catalog lock: entry '{}' is out of order", entry.id));
            }
            previous = Some(entry.id.as_str());
            if !valid_digest(&entry.digest) {
                errors.push(format!("catalog lock: entry '{}' has an invalid digest", entry.id));
            }
            if require_approved && entry.review != ReviewState::Approved {
                errors.push(format!("catalog lock: entry '{}' is not approved ({:?})", entry.id, entry.review));
            }
        }
        for entry in &self.entries {
            for dep in &entry.requires {
                if !ids.contains(dep.as_str()) {
                    errors.push(format!("catalog lock: '{}' requires missing entry '{dep}'", entry.id));
                }
            }
        }
        if let Some(id) = self.find_cycle() {
            errors.push(format!("catalog lock: dependency cycle through '{id}'"));
        }
        if self.digest != Self::compute_digest(&self.entries) {
            errors.push("catalog lock: digest does not match its entries".into());
        }
        errors
    }

    /// Returns an entry id that lies on a cycle, if any.
    pub fn find_cycle(&self) -> Option<String> {
        let deps: BTreeMap<&str, &[String]> =
            self.entries.iter().map(|e| (e.id.as_str(), e.requires.as_slice())).collect();
        // 0 = unvisited, 1 = on the current path, 2 = done
        let mut state: BTreeMap<&str, u8> = BTreeMap::new();

        fn visit<'a>(
            id: &'a str,
            deps: &BTreeMap<&'a str, &'a [String]>,
            state: &mut BTreeMap<&'a str, u8>,
        ) -> Option<String> {
            match state.get(id).copied().unwrap_or(0) {
                1 => return Some(id.to_string()),
                2 => return None,
                _ => {}
            }
            state.insert(id, 1);
            if let Some(next) = deps.get(id) {
                for dep in next.iter() {
                    if deps.contains_key(dep.as_str()) {
                        if let Some(found) = visit(dep.as_str(), deps, state) {
                            return Some(found);
                        }
                    }
                }
            }
            state.insert(id, 2);
            None
        }

        for id in deps.keys().copied() {
            if let Some(found) = visit(id, &deps, &mut state) {
                return Some(found);
            }
        }
        None
    }
}

