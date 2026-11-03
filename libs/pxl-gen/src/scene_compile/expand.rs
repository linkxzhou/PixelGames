//! Recursive module expansion and tile-instance helpers.
use std::collections::{BTreeMap, BTreeSet};

use pxl_contract::tileset::{
    DecorLayer, Module, SceneInstance, TileDef, Tileset,
};
use pxl_core::{PxlError, PxlResult};

use super::{
    cell_mask, tile_for_mask, CompileReport, KitResolver, MaskResolution,
    ModuleLibrary, SceneBudget,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn expand_module(
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
pub(super) fn find_cycle(modules: &BTreeMap<String, Module>, root: &str) -> Option<String> {
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
