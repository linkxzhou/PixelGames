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
    CompiledScene, DecorLayer, Module, SceneInstance, TileDef, Tileset, COMPILED_SCENE_CONTRACT,
    MAX_EXPANDED_INSTANCES, MAX_EXPANDED_PIXELS, MAX_MODULE_DEPTH, MASK_E, MASK_N, MASK_NE,
    MASK_NW, MASK_S, MASK_SE, MASK_SW, MASK_W,
};
use pxl_contract::prototype::{ReviewState, SceneKit};
use pxl_core::{PxlError, PxlResult};

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

/// How one mask was resolved for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskResolution {
    /// Drawn exactly by this tile.
    Exact(String),
    /// Approximated by this tile; the report must say so.
    Approximate(String),
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
    if let Some(cycle) = find_cycle(&library.modules, root) {
        return Err(PxlError::semantic(format!("module cycle through '{cycle}'")));
    }

    let mut report = CompileReport::default();
    let mut instances: Vec<SceneInstance> = Vec::new();
    let mut collision: BTreeMap<String, String> = BTreeMap::new();
    let mut depends: BTreeMap<String, String> = BTreeMap::new();
    let mut used_ids: BTreeSet<String> = BTreeSet::new();

    expand_module(
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

#[allow(clippy::too_many_arguments)]
fn expand_module(
    library: &ModuleLibrary,
    kits: &dyn KitResolver,
    module: &Module,
    origin: [i32; 2],
    seed: u64,
    budget: &SceneBudget,
    report: &mut CompileReport,
    instances: &mut Vec<SceneInstance>,
    collision: &mut BTreeMap<String, String>,
    depends: &mut BTreeMap<String, String>,
    used_ids: &mut BTreeSet<String>,
    depth: u32,
    tile_px: u16,
    path: &str,
) -> PxlResult<()> {
    if depth >= budget.max_depth {
        return Err(PxlError::semantic(format!(
            "module nesting reaches the {} depth limit at '{path}'",
            budget.max_depth
        )));
    }
    report.max_depth_reached = report.max_depth_reached.max(depth + 1);
    depends.insert(module.id.clone(), module.digest());

    // Terrain layers: compute each cell's 47-mask from same-layer
    // neighbours and resolve the tile through the tileset's coverage.
    // Exact, approximate and missing masks are tracked separately; a gap
    // fails the whole compilation, never a silent default tile.
    for layer in &module.terrain {
        let set = library.tilesets.get(&layer.tileset).ok_or_else(|| {
            PxlError::semantic(format!(
                "module '{}': terrain layer references missing tileset '{}'",
                module.id, layer.tileset
            ))
        })?;
        depends.insert(set.id.clone(), set.digest());
        let cells: BTreeSet<[i32; 2]> = layer.cells.iter().copied().collect();
        for &at_cell in &layer.cells {
            let mask = cell_mask(
                module.size[0] as i32,
                module.size[1] as i32,
                at_cell[0],
                at_cell[1],
                |x, y| cells.contains(&[x, y]),
            );
            match tile_for_mask(set, mask) {
                Some(MaskResolution::Exact(tile_id)) => {
                    report.exact_masks.insert(mask);
                    push_tile_instance(
                        set, &tile_id, mask, seed, &module.id, at_cell, origin, tile_px, budget,
                        report, instances, used_ids, None,
                    )?;
                }
                Some(MaskResolution::Approximate(tile_id)) => {
                    report
                        .approximated_masks
                        .entry(mask)
                        .or_insert_with(|| tile_id.clone());
                    push_tile_instance(
                        set, &tile_id, mask, seed, &module.id, at_cell, origin, tile_px, budget,
                        report, instances, used_ids, Some(true),
                    )?;
                }
                None => {
                    report
                        .missing_masks
                        .entry(set.id.clone())
                        .or_default()
                        .push(mask);
                }
            }
        }
    }

    // Hand-picked tiles: the author chose the exact tile, no mask involved.
    for cell in &module.tiles {
        let set = library.tilesets.get(&cell.tileset).ok_or_else(|| {
            PxlError::semantic(format!("module '{}': missing tileset '{}'", module.id, cell.tileset))
        })?;
        let tile = set.tiles.iter().find(|t| t.id == cell.tile).ok_or_else(|| {
            PxlError::semantic(format!(
                "tileset '{}' has no tile '{}'",
                set.id, cell.tile
            ))
        })?;
        let variant = pick_variant(set, tile, seed, &module.id, cell.at);
        let resource_id = variant_resource(tile, variant);
        let at = [
            origin[0] + cell.at[0] * i32::from(tile_px),
            origin[1] + cell.at[1] * i32::from(tile_px),
        ];
        let px = u64::from(tile_px) * u64::from(tile_px);
        report.pixels += px;
        if report.instances + 1 > budget.max_instances {
            return Err(PxlError::semantic(format!(
                "scene exceeds the {} instance budget at module '{}'",
                budget.max_instances, module.id
            )));
        }
        report.instances += 1;
        let id = unique_id(used_ids, &format!("inst-{}", resource_id));
        instances.push(SceneInstance {
            id,
            resource_id,
            layer: DecorLayer::Back,
            sort_y: at[1] + i32::from(tile_px),
            at,
            mirror: None,
        });
        depends.insert(set.id.clone(), set.digest());
    }

    // Decorations: pixel-anchored, independent of tiles and collision.
    for decor in &module.decor {
        let resource_id = match (&decor.resource_id, &decor.scene_kit) {
            (Some(id), None) => id.clone(),
            (None, Some(kit_id)) => {
                // Scene-kit decorations expand into the kit's placements.
                let kit = library.kits.get(kit_id).ok_or_else(|| {
                    PxlError::semantic(format!(
                        "module '{}': decor '{}' references missing kit '{kit_id}'",
                        module.id, decor.id
                    ))
                })?;
                depends.insert(kit.id.clone(), kit.digest());
                for placement in kits.resolve(kit_id)? {
                    let at = [origin[0] + decor.at[0] + placement.at[0], origin[1] + decor.at[1] + placement.at[1]];
                    if report.instances + 1 > budget.max_instances {
                        return Err(PxlError::semantic(format!(
                            "scene exceeds the {} instance budget at decor '{}'",
                            budget.max_instances, decor.id
                        )));
                    }
                    report.instances += 1;
                    let id = unique_id(used_ids, &format!("inst-{}", placement.resource_id));
                    instances.push(SceneInstance {
                        id,
                        resource_id: placement.resource_id,
                        layer: placement.layer,
                        sort_y: at[1] + placement.sort_y,
                        at,
                        mirror: None,
                    });
                }
                continue;
            }
            _ => {
                return Err(PxlError::semantic(format!(
                    "module '{}': decor '{}' needs exactly one of resource_id or scene_kit",
                    module.id, decor.id
                )))
            }
        };
        if report.instances + 1 > budget.max_instances {
            return Err(PxlError::semantic(format!(
                "scene exceeds the {} instance budget at decor '{}'",
                budget.max_instances, decor.id
            )));
        }
        report.instances += 1;
        let at = [origin[0] + decor.at[0], origin[1] + decor.at[1]];
        let id = unique_id(used_ids, &format!("inst-{}", resource_id));
        instances.push(SceneInstance {
            id,
            resource_id,
            layer: decor.layer,
            sort_y: at[1],
            at,
            mirror: None,
        });
    }

    // Collision: independent cells; decor edits never touch these.
    for cell in &module.collision {
        let x = origin[0] / i32::from(tile_px) + cell.at[0];
        let y = origin[1] / i32::from(tile_px) + cell.at[1];
        collision.insert(format!("{x},{y}"), cell.kind.clone());
    }

    // Nested modules: recurse with a bounded depth and offset origins.
    for cell in &module.modules {
        let nested = &library.modules[&cell.module];
        let at = [
            origin[0] + cell.at[0] * i32::from(tile_px),
            origin[1] + cell.at[1] * i32::from(tile_px),
        ];
        expand_module(
            library,
            kits,
            nested,
            at,
            seed,
            budget,
            report,
            instances,
            collision,
            depends,
            used_ids,
            depth + 1,
            tile_px,
            &format!("{path}/{}", cell.module),
        )?;
    }
    Ok(())
}

/// Stable variant choice: seed, module id, tileset, tile and cell. Equal
/// inputs always choose the same variant, matching the runtime rule that
/// placement decides, never the frame counter or load order.
fn pick_variant(
    set: &Tileset,
    tile: &TileDef,
    seed: u64,
    module_id: &str,
    at: [i32; 2],
) -> Option<String> {
    if tile.variants.is_empty() {
        return None;
    }
    let index = stable_pick(seed, &format!("{}|{}|{}", module_id, set.id, tile.id), at, tile.variants.len());
    Some(tile.variants[index].clone())
}

/// Push one resolved terrain/tile cell as a scene instance, respecting the
/// instance budget and recording pixels.
#[allow(clippy::too_many_arguments)]
fn push_tile_instance(
    set: &Tileset,
    tile_id: &str,
    _mask: u8,
    seed: u64,
    module_id: &str,
    at_cell: [i32; 2],
    origin: [i32; 2],
    tile_px: u16,
    budget: &SceneBudget,
    report: &mut CompileReport,
    instances: &mut Vec<SceneInstance>,
    used_ids: &mut BTreeSet<String>,
    _approximate: Option<bool>,
) -> PxlResult<()> {
    let tile = set
        .tiles
        .iter()
        .find(|t| t.id == tile_id)
        .ok_or_else(|| PxlError::semantic(format!("tileset '{}' has no tile '{tile_id}'", set.id)))?;
    let variant = pick_variant(set, tile, seed, module_id, at_cell);
    let resource_id = variant_resource(tile, variant);
    let at = [
        origin[0] + at_cell[0] * i32::from(tile_px),
        origin[1] + at_cell[1] * i32::from(tile_px),
    ];
    report.pixels += u64::from(tile_px) * u64::from(tile_px);
    if report.instances + 1 > budget.max_instances {
        return Err(PxlError::semantic(format!(
            "scene exceeds the {} instance budget at module '{module_id}'",
            budget.max_instances
        )));
    }
    report.instances += 1;
    let id = unique_id(used_ids, &format!("inst-{}", resource_id));
    instances.push(SceneInstance {
        id,
        resource_id,
        layer: DecorLayer::Back,
        sort_y: at[1] + i32::from(tile_px),
        at,
        mirror: None,
    });
    Ok(())
}

fn variant_resource(tile: &TileDef, variant: Option<String>) -> String {
    match variant {
        None => tile.resource_id.clone(),
        Some(v) => format!("{}#{v}", tile.resource_id),
    }
}

/// FNV-1a + avalanche mix, the same shape as `scene-select.js` `pick`, so the
/// Rust compiler and the JS runtime agree on which variant a cell draws.
fn stable_pick(seed: u64, id: &str, at: [i32; 2], count: usize) -> usize {
    let text = format!("{}|{}|{}|{}", seed, id, at[0], at[1]);
    let mut h: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        h ^= u32::from(byte);
        h = h.wrapping_mul(0x0100_0193);
    }
    let mut v = u64::from(h);
    v ^= v >> 16;
    v = v.wrapping_mul(0x85eb_ca6b);
    v ^= v >> 13;
    v = v.wrapping_mul(0xc2b2_ae35);
    v ^= v >> 16;
    (v & 0xffff_ffff) as usize % count
}

fn unique_id(used: &mut BTreeSet<String>, base: &str) -> String {
    let mut id = base.to_string();
    let mut n = 1u32;
    while !used.insert(id.clone()) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Depth-first cycle search from `root`.
fn find_cycle(modules: &BTreeMap<String, Module>, root: &str) -> Option<String> {
    fn visit<'a>(
        id: &'a str,
        modules: &'a BTreeMap<String, Module>,
        state: &mut BTreeMap<&'a str, u8>,
        path: &mut Vec<String>,
    ) -> Option<String> {
        match state.get(id).copied() {
            Some(1) => return Some(id.to_string()),
            Some(2) => return None,
            _ => {}
        }
        state.insert(id, 1);
        path.push(id.to_string());
        if let Some(module) = modules.get(id) {
            for cell in &module.modules {
                if modules.contains_key(&cell.module) {
                    if let Some(found) = visit(&cell.module, modules, state, path) {
                        return Some(found);
                    }
                }
            }
        }
        state.insert(id, 2);
        path.pop();
        None
    }
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    let mut path = Vec::new();
    visit(root, modules, &mut state, &mut path)
}

/// Resolve the 47-mask for one cell of a terrain grid. `same(x, y)` reports
/// whether the cell belongs to the terrain being drawn; out-of-bounds cells
/// are not the same terrain. Shared fixtures keep this in step with
/// `scene-select.js neighbourMask`.
pub fn cell_mask<F>(width: i32, height: i32, x: i32, y: i32, same: F) -> u8
where
    F: Fn(i32, i32) -> bool,
{
    let look = |dx: i32, dy: i32| -> bool {
        let (nx, ny) = (x + dx, y + dy);
        nx >= 0 && ny >= 0 && nx < width && ny < height && same(nx, ny)
    };
    let (n, e, s, w) = (look(0, -1), look(1, 0), look(0, 1), look(-1, 0));
    let mut raw = 0u8;
    if n {
        raw |= MASK_N;
    }
    if e {
        raw |= MASK_E;
    }
    if s {
        raw |= MASK_S;
    }
    if w {
        raw |= MASK_W;
    }
    if look(1, -1) {
        raw |= MASK_NE;
    }
    if look(1, 1) {
        raw |= MASK_SE;
    }
    if look(-1, 1) {
        raw |= MASK_SW;
    }
    if look(-1, -1) {
        raw |= MASK_NW;
    }
    pxl_contract::tileset::reduce_mask(raw)
}

/// Resolve which tile draws one mask: exact first, approximation second, and
/// `None` only when the tileset has neither — the caller must report that gap.
pub fn tile_for_mask(set: &Tileset, mask: u8) -> Option<MaskResolution> {
    if let Some(tile) = set.tiles.iter().find(|t| t.exact_masks.contains(&mask)) {
        return Some(MaskResolution::Exact(tile.id.clone()));
    }
    if let Some(tile) = set.tiles.iter().find(|t| t.approximate_masks.contains(&mask)) {
        return Some(MaskResolution::Approximate(tile.id.clone()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_contract::tileset::{
        CollisionCell, Decor, ModuleCell, TerrainLayer, TileCell, TileDef, TILESET_CONTRACT,
    };

    /// Every reachable mask except the all-same one, for approximation tests.
    fn reachable_approx() -> Vec<u8> {
        pxl_contract::tileset::reachable_masks()
            .into_iter()
            .filter(|m| *m != 255)
            .collect()
    }
    use pxl_contract::prototype::KitItem;

    struct NoKits;
    impl KitResolver for NoKits {
        fn resolve(&self, kit_id: &str) -> PxlResult<Vec<KitPlacement>> {
            Err(PxlError::semantic(format!("kit '{kit_id}' is not resolvable in this test")))
        }
    }

    fn grass_tileset() -> Tileset {
        Tileset {
            contract: TILESET_CONTRACT.into(),
            id: "terrain_grass".into(),
            version: 1,
            palette_family: "environment".into(),
            tile_size: 16,
            tiles: vec![TileDef {
                id: "grass_fill".into(),
                resource_id: "res-env-grass-fill".into(),
                exact_masks: (0..=255u8).filter(|b| pxl_contract::tileset::reduce_mask(*b) == 255).collect(),
                approximate_masks: vec![],
                variants: vec![],
            }],
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn plaza_module() -> Module {
        Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "landmark_plaza".into(),
            version: 1,
            size: [2, 2],
            terrain: vec![],
            tiles: vec![
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [1, 0] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 1] },
                TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [1, 1] },
            ],
            modules: vec![],
            decor: vec![Decor {
                id: "fountain".into(),
                at: [8, 8],
                resource_id: Some("res-env-fountain".into()),
                scene_kit: None,
                layer: DecorLayer::Front,
            }],
            collision: vec![CollisionCell { at: [1, 1], kind: "solid".into() }],
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn library() -> ModuleLibrary {
        ModuleLibrary {
            tilesets: BTreeMap::from([("terrain_grass".into(), grass_tileset())]),
            modules: BTreeMap::from([("landmark_plaza".into(), plaza_module())]),
            kits: BTreeMap::new(),
        }
    }

    #[test]
    fn a_simple_module_expands_deterministically_with_collision_kept_separate() {
        let lib = library();
        let (scene, report) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.instances.len(), 5, "4 tiles + 1 decor");
        assert_eq!(report.instances, 5);
        assert_eq!(report.pixels, 4 * 16 * 16);
        assert_eq!(scene.collision.get("1,1").map(String::as_str), Some("solid"));

        // Decoration is pixel-anchored at its declared offset, front layer.
        let fountain = scene.instances.iter().find(|i| i.resource_id.contains("fountain")).unwrap();
        assert_eq!(fountain.at, [8, 8]);
        assert_eq!(fountain.layer, DecorLayer::Front);

        // Determinism: same inputs, byte-identical scene.
        let (again, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.digest(), again.digest());

        // Collision is untouched when decor changes.
        let mut edited = library();
        edited.modules.get_mut("landmark_plaza").unwrap().decor[0].at = [4, 4];
        let (moved, _) = compile_scene(
            &edited,
            &NoKits,
            "landmark_plaza",
            "scene_plaza",
            [32, 32],
            7,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(moved.collision, scene.collision, "moving decor must not move collision");
    }

    #[test]
    fn nested_modules_expand_with_offsets_and_depth_is_bounded() {
        let mut lib = library();
        let mut plaza = plaza_module();
        plaza.modules.push(ModuleCell { module: "landmark_plaza".into(), at: [2, 0], variant: None });
        // Self-nesting is a structural error…
        let errors = plaza.validate();
        assert!(errors.iter().any(|e| e.contains("cannot nest itself")), "{errors:?}");
        // …so use a distinct wrapper module for the depth test.
        let plaza = plaza_module();
        lib.modules.insert("landmark_plaza".into(), plaza);
        let wrapper = Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "wrapper".into(),
            version: 1,
            size: [6, 2],
            terrain: vec![],
            tiles: vec![],
            modules: vec![
                ModuleCell { module: "landmark_plaza".into(), at: [0, 0], variant: None },
                ModuleCell { module: "landmark_plaza".into(), at: [2, 0], variant: None },
            ],
            decor: vec![],
            collision: vec![],
            license: "original".into(),
            review: ReviewState::Approved,
        };
        lib.modules.insert("wrapper".into(), wrapper);

        let (scene, report) = compile_scene(
            &lib,
            &NoKits,
            "wrapper",
            "scene_wrapped",
            [96, 32],
            3,
            SceneBudget::default(),
        )
        .unwrap();
        // 2 plazas × (4 tiles + 1 decor).
        assert_eq!(scene.instances.len(), 10);
        assert_eq!(report.max_depth_reached, 2);
        // Both plazas expanded; the second is offset by 2 cells = 32px.
        let fountains: Vec<&SceneInstance> = scene
            .instances
            .iter()
            .filter(|i| i.resource_id.contains("fountain"))
            .collect();
        assert_eq!(fountains.len(), 2);
        let xs: BTreeSet<i32> = fountains.iter().map(|f| f.at[0]).collect();
        assert_eq!(xs, BTreeSet::from([8, 40]));

        // A two-module cycle (loop_a → loop_b → loop_a) is caught before expansion.
        let base = Module {
            contract: pxl_contract::tileset::MODULE_CONTRACT.into(),
            id: "cell".into(),
            version: 1,
            size: [1, 1],
            terrain: vec![],
            tiles: vec![TileCell { tileset: "terrain_grass".into(), tile: "grass_fill".into(), at: [0, 0] }],
            modules: vec![],
            decor: vec![],
            collision: vec![],
            license: "original".into(),
            review: ReviewState::Approved,
        };
        let mut loop_a = base.clone();
        loop_a.id = "loop_a".into();
        loop_a.modules = vec![ModuleCell { module: "loop_b".into(), at: [0, 0], variant: None }];
        loop_a.tiles.clear();
        let mut loop_b = base.clone();
        loop_b.id = "loop_b".into();
        loop_b.modules = vec![ModuleCell { module: "loop_a".into(), at: [0, 0], variant: None }];
        loop_b.tiles.clear();
        let mut cyclic = library();
        cyclic.modules.insert("loop_a".into(), loop_a);
        cyclic.modules.insert("loop_b".into(), loop_b);
        let err = compile_scene(
            &cyclic,
            &NoKits,
            "loop_a",
            "scene_cycle",
            [16, 16],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("cycle"), "{text}");

        // depth budget: a chain deeper than the limit fails closed.
        let mut depth_lib = library();
        depth_lib.modules.insert("deep".into(), base.clone());
        for n in 0..10 {
            let mut m = base.clone();
            m.id = format!("deep_{n}");
            m.modules = vec![ModuleCell {
                module: if n == 0 { "deep".into() } else { format!("deep_{}", n - 1) },
                at: [0, 0],
                variant: None,
            }];
            depth_lib.modules.insert(m.id.clone(), m);
        }
        let err = compile_scene(
            &depth_lib,
            &NoKits,
            "deep_9",
            "scene_deep",
            [16, 16],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("depth limit"), "{text}");
    }

    #[test]
    fn missing_tileset_coverage_is_an_error_not_a_default_tile() {
        let mut lib = library();
        let set = lib.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].exact_masks = vec![255]; // only the all-same mask
        set.tiles[0].approximate_masks = vec![MASK_N | MASK_E | MASK_S | MASK_W];

        // A single terrain cell still works: its own mask is all-same.
        let mut plaza = plaza_module();
        plaza.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0]],
        }];
        plaza.tiles.clear();
        plaza.decor.clear();
        plaza.collision.clear();
        lib.modules.insert("landmark_plaza".into(), plaza);

        // But a 2x2 terrain grid needs edge masks beyond the approximation
        // list; coverage gaps surface as missing masks and fail the build.
        let mut two = plaza_module();
        two.id = "two_cells".into();
        two.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0], [1, 0], [0, 1], [1, 1]],
        }];
        two.tiles.clear();
        two.modules.clear();
        lib.modules.insert("two_cells".into(), two);

        let err = compile_scene(
            &lib,
            &NoKits,
            "two_cells",
            "scene_gap",
            [32, 32],
            1,
            SceneBudget::default(),
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("do not cover every mask"), "{text}");

        // Exact vs approximate are reported separately: rerun with full
        // approximation coverage and the report shows the approximated masks.
        let mut full = library();
        let set = full.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].exact_masks = vec![255];
        set.tiles[0].approximate_masks = reachable_approx();
        let mut two = plaza_module();
        two.id = "two_cells".into();
        two.terrain = vec![TerrainLayer {
            tileset: "terrain_grass".into(),
            cells: vec![[0, 0], [1, 0], [0, 1], [1, 1]],
        }];
        two.tiles.clear();
        two.modules.clear();
        full.modules.insert("two_cells".into(), two);
        let (_, report) = compile_scene(
            &full,
            &NoKits,
            "two_cells",
            "scene_gap_approx",
            [32, 32],
            1,
            SceneBudget::default(),
        )
        .unwrap();
        assert!(
            !report.approximated_masks.is_empty(),
            "approximated masks must be reported separately: {:?}",
            report.approximated_masks
        );
        assert!(report.missing_masks.is_empty());
    }

    #[test]
    fn variant_choice_is_stable_and_seed_dependent() {
        let mut lib = library();
        let set = lib.tilesets.get_mut("terrain_grass").unwrap();
        set.tiles[0].variants = vec!["a".into(), "b".into()];
        let (scene, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            11,
            SceneBudget::default(),
        )
        .unwrap();
        let tags: Vec<&str> = scene
            .instances
            .iter()
            .filter(|i| i.resource_id.contains('#'))
            .map(|i| i.resource_id.split('#').nth(1).unwrap())
            .collect();
        assert_eq!(tags.len(), 4, "every tile instance carries a variant: {tags:?}");

        // Same seed: identical choices.
        let (again, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            11,
            SceneBudget::default(),
        )
        .unwrap();
        assert_eq!(scene.digest(), again.digest());

        // Different seed: at least one choice differs (statistically certain
        // for 4 cells × 2 variants with avalanche mixing).
        let (other, _) = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_variants",
            [32, 32],
            12,
            SceneBudget::default(),
        )
        .unwrap();
        assert_ne!(scene.digest(), other.digest());
    }

    #[test]
    fn budget_limits_fail_closed() {
        let lib = library();
        let err = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_tiny",
            [32, 32],
            1,
            SceneBudget { max_instances: 3, max_pixels: 1024, max_depth: 8 },
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("instance budget"), "{text}");

        let err = compile_scene(
            &lib,
            &NoKits,
            "landmark_plaza",
            "scene_tiny",
            [32, 32],
            1,
            SceneBudget { max_instances: 100, max_pixels: 100, max_depth: 8 },
        )
        .unwrap_err();
        let text = err.message.clone();
        assert!(text.contains("pixel"), "{text}");
    }

    #[test]
    fn mask_helpers_match_the_frozen_47_tile_reduction() {
        // A 3x3 all-same grid: the centre cell is fully surrounded.
        let mask = cell_mask(3, 3, 1, 1, |_, _| true);
        assert_eq!(mask, 255);
        // Top-left corner of a 2x2 all-same grid: E, S and SE set.
        let mask = cell_mask(2, 2, 0, 0, |_, _| true);
        assert_eq!(mask, MASK_E | MASK_S | MASK_SE);
        // A diagonal neighbour without both orthogonal neighbours is reduced away.
        let mask = cell_mask(3, 3, 1, 1, |x, y| (x == 0 && y == 0) || (x == 1 && y == 1));
        assert_eq!(mask & MASK_NW, 0, "NW is not counted unless N and W are both same");

        let set = grass_tileset();
        assert!(matches!(
            tile_for_mask(&set, 255),
            Some(MaskResolution::Exact(_))
        ));
        let mut partial = set.clone();
        partial.tiles[0].exact_masks = vec![];
        partial.tiles[0].approximate_masks = vec![255];
        assert!(matches!(
            tile_for_mask(&partial, 255),
            Some(MaskResolution::Approximate(_))
        ));
        assert!(tile_for_mask(&partial, 12).is_none());
    }

    #[test]
    fn scene_kit_decor_expands_through_the_resolver() {
        let mut lib = library();
        lib.kits.insert(
            "porch_group".into(),
            SceneKit {
                contract: pxl_contract::prototype::SCENE_KIT_CONTRACT.into(),
                id: "porch_group".into(),
                version: 1,
                bounds: [32, 32],
                items: vec![
                    KitItem { prototype: "door_wood".into(), at: [0, 0], params: BTreeMap::new() },
                    KitItem { prototype: "flowerpot".into(), at: [16, 24], params: BTreeMap::new() },
                ],
                license: "original".into(),
                review: ReviewState::Approved,
            },
        );
        let mut plaza = plaza_module();
        plaza.decor.push(Decor {
            id: "porch".into(),
            at: [2, 2],
            resource_id: None,
            scene_kit: Some("porch_group".into()),
            layer: DecorLayer::Back,
        });
        lib.modules.insert("landmark_plaza".into(), plaza);

        struct Porch;
        impl KitResolver for Porch {
            fn resolve(&self, kit_id: &str) -> PxlResult<Vec<KitPlacement>> {
                assert_eq!(kit_id, "porch_group");
                Ok(vec![
                    KitPlacement { resource_id: "res-env-door".into(), layer: DecorLayer::Front, sort_y: 32, at: [0, 0] },
                    KitPlacement { resource_id: "res-env-flowerpot".into(), layer: DecorLayer::Front, sort_y: 24, at: [16, 24] },
                ])
            }
        }

        let (scene, _) = compile_scene(
            &lib,
            &Porch,
            "landmark_plaza",
            "scene_kit",
            [32, 32],
            5,
            SceneBudget::default(),
        )
        .unwrap();
        // 4 tiles + fountain + 2 kit placements.
        assert_eq!(scene.instances.len(), 7);
        let door = scene.instances.iter().find(|i| i.resource_id == "res-env-door").unwrap();
        assert_eq!(door.at, [2, 2]);
        let pot = scene.instances.iter().find(|i| i.resource_id == "res-env-flowerpot").unwrap();
        assert_eq!(pot.at, [18, 26]);
        assert!(scene.depends.contains_key("porch_group"));
    }
}
