//! Deterministic expansion of a frozen prototype instance into one shapes file.
//!
//! The expander never invents geometry. It starts from the artist-authored base
//! shapes, overlays the structural variants whose parameter value was selected,
//! applies explicit per-region patches, and recolours regions through the
//! instance's surface choices. Equal inputs always give equal output, and any
//! reference that does not resolve is an error rather than a silent fallback.

use std::collections::{BTreeMap, BTreeSet};

use pxl_contract::prototype::{AssetInstance, ParamValue, Prototype, SurfaceRecipe};
use pxl_core::{decode_index, PxlError, PxlResult};
use pxl_format::{DrawCmd, ShapeRegion, ShapesFile};

/// Result of expanding one instance.
#[derive(Debug, Clone)]
pub struct Expanded {
    pub shapes: ShapesFile,
    /// Variant rules that fired, as `param=value@region`, in declaration order.
    pub applied_variants: Vec<String>,
    /// Region -> surface recipe id, as chosen by the instance.
    pub surfaces: BTreeMap<String, String>,
}

/// Bounding box `(x0, y0, x1, y1)` (inclusive) of the set pixels in a region mask.
fn region_bbox(rows: &[String]) -> Option<(usize, usize, usize, usize)> {
    let mut found: Option<(usize, usize, usize, usize)> = None;
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == '.' || ch == '0' {
                continue;
            }
            found = Some(match found {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    found
}

fn semantic(message: String) -> PxlError {
    PxlError::semantic(message)
}

/// The value a parameter takes in this instance: the instance's own choice,
/// otherwise the prototype's declared default.
fn effective_param(
    prototype: &Prototype,
    instance: &AssetInstance,
    name: &str,
) -> Option<ParamValue> {
    if let Some(value) = instance.params.get(name) {
        return Some(value.clone());
    }
    use pxl_contract::prototype::ParamRange;
    prototype.params.get(name).map(|range| match range {
        ParamRange::Int { default, .. } => ParamValue::Int(*default),
        ParamRange::Choice { default, .. } => ParamValue::Text(default.clone()),
        ParamRange::Flag { default } => ParamValue::Flag(*default),
    })
}

/// Overlay `rows` on `base` with the overlay's top-left at `(ox, oy)`.
/// `.` leaves the pixel; anything else replaces it. Pixels that would fall
/// outside the canvas are an error, never clipped silently.
fn overlay(
    pixels: &mut [Vec<char>],
    rows: &[String],
    ox: usize,
    oy: usize,
    what: &str,
) -> PxlResult<()> {
    let height = pixels.len();
    let width = pixels.first().map(Vec::len).unwrap_or(0);
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch == '.' {
                continue;
            }
            decode_index(ch)?;
            let (x, y) = (ox + dx, oy + dy);
            if x >= width || y >= height {
                return Err(semantic(format!(
                    "{what}: pixel at ({x},{y}) falls outside the {width}x{height} canvas"
                )));
            }
            pixels[y][x] = ch;
        }
    }
    Ok(())
}

/// Expand `instance` against its `prototype`, starting from `base`.
///
/// `surfaces` is the catalog of surface recipes; `surface_palette` maps a surface
/// id to the palette indices that replace each source index inside the region
/// (a recolour table). Regions without a surface keep the artist's colours.
pub fn expand_instance(
    prototype: &Prototype,
    instance: &AssetInstance,
    base: &ShapesFile,
    surfaces: &BTreeMap<String, SurfaceRecipe>,
    surface_palette: &BTreeMap<String, BTreeMap<char, char>>,
) -> PxlResult<Expanded> {
    let problems = instance.validate_against(prototype, surfaces);
    if !problems.is_empty() {
        return Err(semantic(format!("instance cannot be expanded: {}", problems.join("; "))));
    }
    let tier = prototype
        .size_tiers
        .iter()
        .find(|tier| tier.id == instance.size_tier)
        .ok_or_else(|| semantic(format!("unknown size tier '{}'", instance.size_tier)))?;
    if base.w != tier.size[0] || base.h != tier.size[1] {
        return Err(semantic(format!(
            "base shapes are {}x{} but tier '{}' is {}x{}",
            base.w, base.h, tier.id, tier.size[0], tier.size[1]
        )));
    }
    let declared: BTreeSet<&str> = prototype.regions.iter().map(String::as_str).collect();
    for name in base.regions.keys() {
        if !declared.contains(name.as_str()) {
            return Err(semantic(format!("base shapes define undeclared region '{name}'")));
        }
    }
    for name in &prototype.regions {
        if !base.regions.contains_key(name) {
            return Err(semantic(format!("base shapes are missing declared region '{name}'")));
        }
    }

    // Rasterise the base pixels from the `pixels` draw commands only; anything
    // else would make expansion depend on a second renderer.
    let (w, h) = (base.w as usize, base.h as usize);
    let mut canvas = vec![vec!['.'; w]; h];
    for (n, cmd) in base.draw.iter().enumerate() {
        match cmd {
            DrawCmd::Pixels { pixels, x, y, .. } => {
                let ox = usize::try_from(*x).map_err(|_| semantic(format!("draw[{n}]: negative x")))?;
                let oy = usize::try_from(*y).map_err(|_| semantic(format!("draw[{n}]: negative y")))?;
                overlay(&mut canvas, pixels, ox, oy, &format!("draw[{n}]"))?;
            }
            _ => {
                return Err(semantic(format!(
                    "draw[{n}]: prototype bases must use 'pixels' commands so expansion is exact"
                )))
            }
        }
    }

    let regions: BTreeMap<String, Vec<String>> =
        base.regions.iter().map(|(k, v)| (k.clone(), v.pixels.clone())).collect();
    let mut applied = Vec::new();

    for rule in &prototype.variants {
        let value = effective_param(prototype, instance, &rule.param)
            .ok_or_else(|| semantic(format!("variant uses unknown param '{}'", rule.param)))?;
        if value != rule.equals {
            continue;
        }
        let mask = regions
            .get(&rule.region)
            .ok_or_else(|| semantic(format!("variant targets unknown region '{}'", rule.region)))?;
        let (x0, y0, ..) = region_bbox(mask)
            .ok_or_else(|| semantic(format!("region '{}' is empty; a variant cannot anchor to it", rule.region)))?;
        overlay(&mut canvas, &rule.rows, x0, y0, &format!("variant {}@{}", rule.param, rule.region))?;
        applied.push(format!("{}={:?}@{}", rule.param, value, rule.region));
    }

    for patch in &instance.patches {
        let mask = regions
            .get(&patch.region)
            .ok_or_else(|| semantic(format!("patch targets unknown region '{}'", patch.region)))?;
        let (x0, y0, ..) = region_bbox(mask)
            .ok_or_else(|| semantic(format!("region '{}' is empty; a patch cannot anchor to it", patch.region)))?;
        overlay(&mut canvas, &patch.rows, x0, y0, &format!("patch@{}", patch.region))?;
    }

    // Recolour per surface, only inside that region's mask.
    for (region, surface_id) in &instance.surfaces {
        let Some(table) = surface_palette.get(surface_id) else {
            return Err(semantic(format!(
                "surface '{surface_id}' has no recolour table for region '{region}'"
            )));
        };
        let mask = regions.get(region).expect("validated against prototype regions");
        for (y, row) in mask.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == '0' {
                    continue;
                }
                if let Some(replacement) = table.get(&canvas[y][x]) {
                    canvas[y][x] = *replacement;
                }
            }
        }
    }

    let rows: Vec<String> = canvas.iter().map(|row| row.iter().collect()).collect();
    let shapes = ShapesFile {
        w: base.w,
        h: base.h,
        palette: base.palette.clone(),
        mirror: base.mirror.clone(),
        draw: vec![DrawCmd::Pixels { pixels: rows, x: 0, y: 0, mirror: None }],
        regions: regions
            .into_iter()
            .map(|(name, pixels)| (name, ShapeRegion { pixels }))
            .collect(),
    };
    Ok(Expanded {
        shapes,
        applied_variants: applied,
        surfaces: instance.surfaces.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_contract::prototype::{
        sha256_digest, Connector, DrawLayer, Footprint, LayerSplit, ParamRange, PrototypeKind,
        PrototypeSource, ReviewState, RegionPatch, SizeTier, SurfaceRecipe, VariantRule, ViewKind,
        ASSET_INSTANCE_CONTRACT, PROTOTYPE_CONTRACT, SURFACE_RECIPE_CONTRACT,
    };

    fn text(error: &PxlError) -> String {
        error.message.clone()
    }

    fn pixels(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|r| r.to_string()).collect()
    }

    fn base() -> ShapesFile {
        ShapesFile {
            w: 6,
            h: 4,
            palette: None,
            mirror: None,
            draw: vec![DrawCmd::Pixels {
                pixels: pixels(&["..22..", ".2222.", "333333", "3.44.3"]),
                x: 0,
                y: 0,
                mirror: None,
            }],
            regions: BTreeMap::from([
                ("roof".into(), ShapeRegion { pixels: pixels(&["..11..", ".1111.", "000000", "000000"]) }),
                ("wall".into(), ShapeRegion { pixels: pixels(&["......", "......", "111111", "1.11.1"]) }),
            ]),
        }
    }

    fn prototype() -> Prototype {
        Prototype {
            contract: PROTOTYPE_CONTRACT.into(),
            id: "tiny_house".into(),
            version: 1,
            kind: PrototypeKind::Building,
            view: ViewKind::TopDown,
            size_tiers: vec![SizeTier { id: "s".into(), size: [6, 4] }],
            params: BTreeMap::from([("has_chimney".into(), ParamRange::Flag { default: false })]),
            regions: vec!["roof".into(), "wall".into()],
            connectors: vec![Connector { id: "door".into(), at: [3, 3], dir: "S".into(), width: 2, accepts: vec!["path".into()] }],
            footprint: Footprint { visual: [0, 0, 6, 4], sort_y: 3, collision: vec![[0, 2, 6, 2]], interact: None },
            layers: vec![LayerSplit { id: "roof_front".into(), regions: vec!["roof".into()], layer: DrawLayer::Front }],
            variants: vec![VariantRule {
                param: "has_chimney".into(),
                equals: ParamValue::Flag(true),
                region: "roof".into(),
                rows: pixels(&[".9"]),
            }],
            source: PrototypeSource { kind: "shapes".into(), path: "tiny_house.shapes.json".into(), digest: sha256_digest(b"x") },
            requires: vec!["roof_tile".into()],
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn surface() -> SurfaceRecipe {
        SurfaceRecipe {
            contract: SURFACE_RECIPE_CONTRACT.into(),
            id: "roof_tile".into(),
            version: 1,
            material_id: "tile".into(),
            applies_to: vec!["roof".into()],
            recipe: "recolour".into(),
            params: BTreeMap::new(),
            scale_px: 2,
            license: "original".into(),
            review: ReviewState::Approved,
        }
    }

    fn catalog() -> BTreeMap<String, SurfaceRecipe> {
        BTreeMap::from([("roof_tile".into(), surface())])
    }

    fn recolour() -> BTreeMap<String, BTreeMap<char, char>> {
        BTreeMap::from([("roof_tile".into(), BTreeMap::from([('2', '5')]))])
    }

    fn instance(proto: &Prototype) -> AssetInstance {
        AssetInstance {
            contract: ASSET_INSTANCE_CONTRACT.into(),
            instance_id: "house_a".into(),
            prototype: proto.reference(),
            size_tier: "s".into(),
            params: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            seed: 1,
            patches: Vec::new(),
        }
    }

    fn canvas_rows(expanded: &Expanded) -> Vec<String> {
        match &expanded.shapes.draw[0] {
            DrawCmd::Pixels { pixels, .. } => pixels.clone(),
            _ => panic!("expected pixels"),
        }
    }

    #[test]
    fn default_instance_reproduces_the_artists_base_exactly() {
        let proto = prototype();
        let out = expand_instance(&proto, &instance(&proto), &base(), &catalog(), &recolour()).unwrap();
        assert_eq!(canvas_rows(&out), pixels(&["..22..", ".2222.", "333333", "3.44.3"]));
        assert!(out.applied_variants.is_empty());
    }

    #[test]
    fn selected_variant_overlays_from_the_region_corner_and_leaves_the_rest() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.params.insert("has_chimney".into(), ParamValue::Flag(true));
        let out = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        // The roof mask spans rows "..11.." and ".1111.", so its bounding box
        // starts at x=1. The overlay ".9" leaves its first cell alone and writes
        // '9' one column to the right, at x=2, replacing that base pixel.
        assert_eq!(canvas_rows(&out), pixels(&["..92..", ".2222.", "333333", "3.44.3"]));
        assert_eq!(out.applied_variants, vec!["has_chimney=Flag(true)@roof".to_string()]);
    }

    #[test]
    fn unselected_variant_does_not_fire() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.params.insert("has_chimney".into(), ParamValue::Flag(false));
        let out = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        assert!(out.applied_variants.is_empty());
        assert_eq!(canvas_rows(&out)[0], "..22..");
    }

    #[test]
    fn expansion_is_deterministic() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.surfaces.insert("roof".into(), "roof_tile".into());
        let a = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        let b = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        assert_eq!(canvas_rows(&a), canvas_rows(&b));
    }

    #[test]
    fn surface_recolours_only_inside_its_region() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.surfaces.insert("roof".into(), "roof_tile".into());
        let out = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        // '2' inside the roof mask becomes '5'; the wall keeps '3'/'4'.
        assert_eq!(canvas_rows(&out), pixels(&["..55..", ".5555.", "333333", "3.44.3"]));
    }

    #[test]
    fn patch_anchors_to_the_region_bounding_box() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.patches.push(RegionPatch { region: "wall".into(), rows: pixels(&["7"]) });
        let out = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap();
        // wall bbox top-left is (0,2).
        assert_eq!(canvas_rows(&out)[2], "733333");
    }

    #[test]
    fn patch_falling_off_the_canvas_is_an_error_not_a_clip() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.patches.push(RegionPatch { region: "wall".into(), rows: pixels(&["......7"]) });
        let error = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap_err();
        assert!(text(&error).contains("falls outside"), "{}", text(&error));
    }

    #[test]
    fn stale_or_invalid_instances_are_rejected_before_any_pixel_work() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.prototype.digest = sha256_digest(b"stale");
        let error = expand_instance(&proto, &inst, &base(), &catalog(), &recolour()).unwrap_err();
        assert!(text(&error).contains("digest does not match"), "{}", text(&error));
    }

    #[test]
    fn base_must_match_the_declared_tier_and_regions() {
        let proto = prototype();
        let inst = instance(&proto);
        let mut wrong_size = base();
        wrong_size.w = 8;
        assert!(expand_instance(&proto, &inst, &wrong_size, &catalog(), &recolour()).is_err());

        let mut missing = base();
        missing.regions.remove("wall");
        let error = expand_instance(&proto, &inst, &missing, &catalog(), &recolour()).unwrap_err();
        assert!(text(&error).contains("missing declared region"), "{}", text(&error));

        let mut extra = base();
        extra.regions.insert("garden".into(), ShapeRegion { pixels: pixels(&["......"; 4]) });
        let error = expand_instance(&proto, &inst, &extra, &catalog(), &recolour()).unwrap_err();
        assert!(text(&error).contains("undeclared region"), "{}", text(&error));
    }

    #[test]
    fn non_pixel_draw_commands_are_refused() {
        let proto = prototype();
        let inst = instance(&proto);
        let mut shapes = base();
        shapes.draw = vec![DrawCmd::Rect { rect: [0, 0, 2, 2], c: 1, pattern: None, mirror: None }];
        let error = expand_instance(&proto, &inst, &shapes, &catalog(), &recolour()).unwrap_err();
        assert!(text(&error).contains("must use 'pixels'"), "{}", text(&error));
    }

    #[test]
    fn missing_recolour_table_is_an_error() {
        let proto = prototype();
        let mut inst = instance(&proto);
        inst.surfaces.insert("roof".into(), "roof_tile".into());
        let error = expand_instance(&proto, &inst, &base(), &catalog(), &BTreeMap::new()).unwrap_err();
        assert!(text(&error).contains("no recolour table"), "{}", text(&error));
    }
}
