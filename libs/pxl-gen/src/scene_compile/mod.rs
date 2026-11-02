//! Deterministic module compilation and scene expansion (P05).
//!
//! The compiler walks a module tree with fixed rules: tiles resolve through
//! their tileset's 47-mask coverage (exact, approximate and missing are
//! reported separately — a gap is never a silent default tile), nested
//! modules recurse with a hard depth limit, decorations are pixel-anchored
//! and independent of collision, and variants are chosen from a stable seed,
//! position and resource id. Depth, instance count and pixel budget fail
//! closed before any exponential input can hang the build.


use std::collections::{BTreeMap, BTreeSet};

use pxl_contract::tileset::{
    CompiledScene, DecorLayer, Module, SceneInstance, Tileset, COMPILED_SCENE_CONTRACT,
    MAX_EXPANDED_INSTANCES, MAX_EXPANDED_PIXELS, MAX_MODULE_DEPTH,
};
use pxl_contract::prototype::{ReviewState, SceneKit};
use pxl_core::{PxlError, PxlResult};

mod expand;
mod mask;

pub use mask::{cell_mask, tile_for_mask, MaskResolution};

/// Everything the compiler needs to resolve references: tilesets by id,
/// modules by id and scene kits by id. Kits contribute their prototype
/// placements through the caller-provided resolver; the compiler itself
/// never reads files.
pub struct ModuleLibrary {
    pub tilesets: BTreeMap<String, Tileset>,
    pub modules: BTreeMap<String, Module>,
    pub kits: BTreeMap<String, SceneKit>,
}

/// Per-kit prototype resolution: the caller maps a kit id to the instances
/// the kit's prototypes expand to (already reduced to resource ids). This
/// keeps pixel-producing expansion in step 300 while the compiler stays pure.
pub trait KitResolver {
    /// Resolve one kit placement into draw instances relative to the kit's
    /// own origin: (resource_id, layer, sort_y, dx, dy).
    fn resolve(&self, kit_id: &str) -> PxlResult<Vec<KitPlacement>>;
}

/// A kit member placement, relative to the kit origin.
#[derive(Debug, Clone)]
pub struct KitPlacement {
    pub resource_id: String,
    pub layer: DecorLayer,
    pub sort_y: i32,
    pub at: [i32; 2],
}

/// Compile-time budget, checked during expansion.
#[derive(Debug, Clone, Copy)]
pub struct SceneBudget {
    pub max_instances: usize,
    pub max_pixels: u64,
    pub max_depth: u32,
}

impl Default for SceneBudget {
    fn default() -> Self {
        Self {
            max_instances: MAX_EXPANDED_INSTANCES,
            max_pixels: MAX_EXPANDED_PIXELS,
            max_depth: MAX_MODULE_DEPTH,
        }
    }
}

/// Diagnostic report of one compilation.
#[derive(Debug, Default)]
pub struct CompileReport {
    pub instances: usize,
    pub exact_masks: BTreeSet<u8>,
    pub approximated_masks: BTreeMap<u8, String>,
    /// Masks a tileset gap left unresolved; expansion fails when non-empty.
    pub missing_masks: BTreeMap<String, Vec<u8>>,
    pub max_depth_reached: u32,
    pub pixels: u64,
}


/// Expand `root` at the scene origin into a compiled scene.
///
/// `tile_px` is the tile edge length shared by every tileset in the library
/// (mixed tile sizes inside one scene are a compile error). `bounds` is the
/// scene size in pixels. The output is a pure function of the library, the
/// seed and the root module — same inputs, byte-identical scene.
pub fn compile_scene(
    library: &ModuleLibrary,
    kits: &dyn KitResolver,
    root: &str,
    scene_id: &str,
    bounds: [u32; 2],
    seed: u64,
    budget: SceneBudget,
) -> PxlResult<(CompiledScene, CompileReport)> {
    let root_module = library.modules.get(root).ok_or_else(|| {
        PxlError::semantic(format!("module '{root}' is not in the library"))
    })?;
    let problems = root_module.validate();
    if !problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "module '{root}' is invalid: {}",
            problems.join("; ")
        )));
    }

    // Every tileset must share one tile size and be structurally valid.
    let mut tile_px: Option<u16> = None;
    for set in library.tilesets.values() {
        let problems = set.validate();
        if !problems.is_empty() {
            return Err(PxlError::semantic(format!("tileset invalid: {}", problems.join("; "))));
        }
        match tile_px {
            None => tile_px = Some(set.tile_size),
            Some(size) if size == set.tile_size => {}
            Some(size) => {
                return Err(PxlError::semantic(format!(
                    "tileset '{}' is {}px but the scene uses {}px tiles; mixed tile sizes are not supported",
                    set.id, set.tile_size, size
                )))
            }
        }
    }
    let tile_px = tile_px.ok_or_else(|| {
        PxlError::semantic("the library has no tilesets; a scene needs at least one")
    })?;

    // Dependency cycle check across modules before any expansion.
    for module in library.modules.values() {
        for cell in &module.modules {
            if !library.modules.contains_key(&cell.module) {
                return Err(PxlError::semantic(format!(
                    "module '{}' nests missing module '{}'",
                    module.id, cell.module
                )));
            }
        }
    }
    if let Some(cycle) = expand::find_cycle(&library.modules, root) {
        return Err(PxlError::semantic(format!("module cycle through '{cycle}'")));
    }

    let mut report = CompileReport::default();
    let mut instances: Vec<SceneInstance> = Vec::new();
    let mut collision: BTreeMap<String, String> = BTreeMap::new();
    let mut depends: BTreeMap<String, String> = BTreeMap::new();
    let mut used_ids: BTreeSet<String> = BTreeSet::new();

    expand::expand_module(
        library,
        kits,
        root_module,
        [0, 0],
        seed,
        &budget,
        &mut report,
        &mut instances,
        &mut collision,
        &mut depends,
        &mut used_ids,
        0,
        tile_px,
        root,
    )?;

    if !report.missing_masks.is_empty() {
        let detail: Vec<String> = report
            .missing_masks
            .iter()
            .map(|(set, masks)| {
                format!(
                    "tileset '{set}': {}",
                    masks.iter().map(|m| pxl_contract::tileset::describe_mask(*m)).collect::<Vec<_>>().join(", ")
                )
            })
            .collect();
        return Err(PxlError::semantic(format!(
            "tilesets do not cover every mask the scene needs: {}; add the missing shapes or list explicit approximations",
            detail.join("; ")
        ))
        .with_hint("a missing connection mask is a gap in the art; it is never filled with a default tile"));
    }

    if instances.is_empty() {
        return Err(PxlError::semantic(format!(
            "module '{root}' expands to nothing; a scene needs at least one instance"
        )));
    }
    if report.instances > budget.max_instances {
        return Err(PxlError::semantic(format!(
            "scene expands to {} instances, over the {} budget",
            report.instances, budget.max_instances
        )));
    }
    if report.pixels > budget.max_pixels {
        return Err(PxlError::semantic(format!(
            "scene expands to {} pixels, over the {} budget",
            report.pixels, budget.max_pixels
        )));
    }

    let scene = CompiledScene {
        contract: COMPILED_SCENE_CONTRACT.into(),
        id: scene_id.to_string(),
        version: 1,
        bounds,
        instances,
        collision,
        seed,
        depends,
        license: "original".into(),
        review: ReviewState::Pending,
    };
    let problems = scene.validate();
    if !problems.is_empty() {
        return Err(PxlError::semantic(format!(
            "compiled scene is invalid: {}",
            problems.join("; ")
        )));
    }
    report.instances = scene.instances.len();
    Ok((scene, report))
}



#[cfg(test)]
mod tests;
