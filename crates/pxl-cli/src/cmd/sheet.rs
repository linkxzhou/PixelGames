use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pxl_core::{IndexFrame, Palette, PxlError, GEN_VERSION, PXL_VERSION};
use pxl_export::{encode_indexed_png, pack_sheet, sheet_json, SheetCell};
use pxl_format::PxlDoc;
use serde_json::{json, Map, Value};

use super::args::SheetArgs;
use crate::error::CmdResult;
use crate::util::{self, Ctx};

pub(super) fn sheet_cmd(ctx: &Ctx, args: SheetArgs) -> CmdResult<()> {
    if args.inputs.is_empty() {
        return Err(PxlError::usage("sheet needs at least one .pxl.json")
            .with_hint("pxl sheet tank.pxl.json ground.pxl.json -o out/tank.sheet.png")
            .into());
    }
    let mut loaded = Vec::new();
    for input in &args.inputs {
        let (doc, path) = util::load_doc(input)?;
        util::check_lock(ctx, doc.meta.pxl_version.as_deref())?;
        let palette = util::doc_palette(&doc, &path)?;
        let id = doc
            .meta
            .asset
            .clone()
            .unwrap_or_else(|| util::stem_id(&path));
        loaded.push(Loaded { id, doc, palette });
    }
    let (png, json_text, hash, w, h, count) =
        pack_loaded(&loaded, &args.layout, args.pad, &args.out)?;
    let json_path = Path::new(&args.out).with_extension("json");
    // The png and its json are one artefact: both obey --overwrite, and the
    // check runs before either is written so a refusal never leaves a pair
    // made of a new json and an old png.
    if args.out != "-" && !ctx.overwrite {
        for p in [Path::new(&args.out), json_path.as_path()] {
            if p.exists() {
                return Err(PxlError::exists(&p.to_string_lossy()).into());
            }
        }
    }
    util::write_out(&args.out, &png, true, ctx.json)?;
    util::write_out(
        json_path.to_str().unwrap_or("sheet.json"),
        json_text.as_bytes(),
        true,
        false,
    )?;
    let mut body = util::ok_base("sheet");
    body.insert("out".into(), json!(args.out));
    body.insert("png".into(), json!(args.out));
    body.insert("png_w".into(), json!(w));
    body.insert("png_h".into(), json!(h));
    body.insert("frames_count".into(), json!(count));
    body.insert("hash".into(), json!(hash));
    util::emit_ok(ctx, &Value::Object(body));
    Ok(())
}

pub(super) struct Loaded {
    pub(super) id: String,
    pub(super) doc: PxlDoc,
    pub(super) palette: Palette,
}

pub(super) fn pack_loaded(
    loaded: &[Loaded],
    layout: &str,
    pad: u32,
    png_out: &str,
) -> CmdResult<(Vec<u8>, String, String, u32, u32, usize)> {
    let palette = &loaded[0].palette;
    for doc in loaded.iter().skip(1) {
        if doc.palette.colors.len() != palette.colors.len()
            || doc
                .palette
                .colors
                .iter()
                .zip(palette.colors.iter())
                .any(|(a, b)| a != b)
        {
            return Err(PxlError::semantic("sheet inputs use different palettes")
                .with_hint("quantize every document onto one palette before packing")
                .into());
        }
    }
    let mut rows = Vec::new();
    let mut pxl_anims = Map::new();
    let mut fps = Map::new();
    let mut facings = Map::new();
    let mut tiles = Map::new();
    let mut dirs = Vec::new();
    let mut used_tags = BTreeSet::new();
    let mut seen_anims = BTreeSet::new();
    let mut asset_frames: Vec<Vec<String>> = Vec::new();
    for doc in loaded {
        let frames = frame_map(&doc.doc)?;
        let mut doc_rows = rows_for(&doc.id, &doc.doc, &frames)?;
        {
            let mut seen = BTreeSet::new();
            asset_frames.push(
                doc_rows
                    .iter()
                    .flatten()
                    .filter(|c| seen.insert(c.name.clone()))
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>(),
            );
        }
        for row in &mut doc_rows {
            let Some(current) = row.first().and_then(|c| c.tag.clone()) else {
                continue;
            };
            let tag = if used_tags.contains(&current) {
                format!("{}_{current}", doc.id)
            } else {
                current
            };
            used_tags.insert(tag.clone());
            for cell in row.iter_mut() {
                cell.tag = Some(tag.clone());
            }
        }
        if doc.doc.meta.kind.as_deref() == Some("tiles") {
            // Every frame of a tile asset is a tile, animated or not: the
            // category follows the declared kind, never whether anims exist.
            let names: Vec<String> = if doc.doc.anims.is_empty() {
                doc_rows.iter().flatten().map(|c| c.name.clone()).collect()
            } else {
                let mut seen = BTreeSet::new();
                doc_rows
                    .iter()
                    .flatten()
                    .filter(|c| seen.insert(c.name.clone()))
                    .map(|c| c.name.clone())
                    .collect()
            };
            if !names.is_empty() {
                let ramp = doc
                    .doc
                    .meta
                    .tile_ramp
                    .clone()
                    .or_else(|| {
                        doc.palette
                            .ramps
                            .keys()
                            .find(|k| k.as_str() == "ground")
                            .cloned()
                    })
                    .or_else(|| doc.palette.ramps.keys().next().cloned())
                    .unwrap_or_else(|| "ground".into());
                tiles.insert(
                    doc.id.clone(),
                    json!({
                        "from": 0,
                        "to": 0,
                        "ramp": ramp,
                        "frames": names,
                    }),
                );
            }
        }
        for (name, anim) in &doc.doc.anims {
            // The first asset keeps the bare animation name. Later assets that
            // reuse it (recolours, variants) are keyed `<asset>_<name>`, matching
            // their renamed frame tags, so validation sees every asset's motion.
            let key = if seen_anims.insert(name.clone()) {
                name.clone()
            } else {
                let scoped = format!("{}_{name}", doc.id);
                if !seen_anims.insert(scoped.clone()) {
                    return Err(PxlError::semantic(format!(
                        "animation '{name}' appears twice in asset '{}'",
                        doc.id
                    ))
                    .into());
                }
                scoped
            };
            fps.insert(key.clone(), json!(anim.fps));
            let mut dir_map = Map::new();
            let identity = row_identity(&doc.doc)?;
            if identity.len() != doc_rows.len() {
                return Err(PxlError::semantic(format!(
                    "asset '{}' produced {} sheet rows for {} animation rows",
                    doc.id,
                    doc_rows.len(),
                    identity.len()
                ))
                .into());
            }
            for ((anim_name, dir), row) in identity.iter().zip(&doc_rows) {
                if anim_name != name {
                    continue;
                }
                let names: Vec<_> = row.iter().map(|c| c.name.clone()).collect();
                dir_map.insert(dir.clone().unwrap_or_else(|| "_".into()), json!(names));
            }
            if dir_map.is_empty() {
                return Err(PxlError::semantic(format!(
                    "animation '{name}' of asset '{}' has no sheet rows",
                    doc.id
                ))
                .into());
            }
            // `mode` is written so a standalone `pxl validate` sees the same
            // static/dynamic intent that build used.
            let mode = if anim.mode == pxl_format::AnimMode::Static {
                "static"
            } else {
                "dynamic"
            };
            pxl_anims.insert(
                key,
                json!({
                    "fps": anim.fps,
                    "loop": anim.looped,
                    "mode": mode,
                    "asset": doc.id,
                    "base": name,
                    "dirs": Value::Object(dir_map),
                }),
            );
        }
        if dirs.is_empty() {
            for d in FACING_NAMES {
                if frames.contains_key(*d) {
                    dirs.push((*d).to_string());
                }
            }
        }
        for d in FACING_NAMES {
            if facings.contains_key(*d) {
                continue;
            }
            let key = format!("idle_{d}_0");
            if frames.contains_key(&key) {
                facings.insert((*d).to_string(), json!(format!("{}_{}", doc.id, key)));
            } else if frames.contains_key(*d) {
                facings.insert((*d).to_string(), json!(format!("{}_{}", doc.id, *d)));
            }
        }
        rows.extend(doc_rows);
    }
    // Declared use and owned frames of every asset, so standalone validation
    // applies the same per-use rules that build did.
    let mut assets = Map::new();
    for (doc, names) in loaded.iter().zip(&asset_frames) {
        assets.insert(
            doc.id.clone(),
            json!({
                "kind": doc.doc.meta.kind.clone().unwrap_or_else(|| "sprite".into()),
                "frames": names,
            }),
        );
    }
    // Fill tile from/to after we know placement order.
    let packed = pack_sheet(&rows, layout, pad)?;
    let cursor = 0u32;
    for (id, value) in tiles.iter_mut() {
        let n = value
            .get("frames")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0) as u32;
        if let Some(obj) = value.as_object_mut() {
            // Find real indices by name.
            if let Some(frames) = obj.get("frames").and_then(|v| v.as_array()) {
                let names: Vec<String> = frames
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if let Some(first) = names.first() {
                    if let Some(pos) = packed.placements.iter().position(|p| p.name == *first) {
                        obj.insert("from".into(), json!(pos));
                        obj.insert("to".into(), json!(pos + names.len() - 1));
                    }
                }
            }
            let _ = (id, n, cursor);
        }
    }
    let image_name = Path::new(png_out)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet.png");
    let pivot = [
        packed.placements.first().map(|p| p.w / 2).unwrap_or(0),
        packed.placements.first().map(|p| p.h / 2).unwrap_or(0),
    ];
    let pxl = json!({
        "version": PXL_VERSION,
        "gen_version": GEN_VERSION,
        "palette": loaded[0].doc.palette,
        "colors": loaded[0].palette.colors.iter().map(|c| c.to_hex()).collect::<Vec<_>>(),
        "ramps": loaded[0].palette.ramps,
        "cycles": loaded[0].palette.cycles.iter().map(|(k, c)| (k.clone(), json!({"indices": c.indices, "period": c.period}))).collect::<Map<_, _>>(),
        "special": {
            "transparent": loaded[0].palette.special.transparent,
            "outline": loaded[0].palette.special.outline,
            "flash": loaded[0].palette.special.flash,
        },
        "pivot": pivot,
        "dirs": dirs,
        "fps": fps,
        "facings": facings,
        "anims": pxl_anims,
        "tiles": tiles,
        "assets": assets,
    });
    let text = sheet_json(&packed, image_name, pxl)?;
    let png = encode_indexed_png(&packed.image, palette, 1)?;
    let hash = util::hash_of(&png);
    Ok((
        png,
        text,
        hash,
        packed.image.w as u32,
        packed.image.h as u32,
        packed.placements.len(),
    ))
}

pub(super) fn frame_map(doc: &PxlDoc) -> CmdResult<BTreeMap<String, IndexFrame>> {
    let mut map = BTreeMap::new();
    for (name, frame) in util::frames_of(doc)? {
        map.insert(name, frame);
    }
    Ok(map)
}

/// `(animation, direction)` of every row `rows_for` returns, in the same order.
/// Row identity comes from the document, never from the display tag, so a
/// renamed or prefixed tag cannot detach an animation from its frames.
pub(super) fn row_identity(doc: &PxlDoc) -> CmdResult<Vec<(String, Option<String>)>> {
    let mut names: Vec<String> = doc.anims.keys().cloned().collect();
    names.sort_by(|a, b| anim_rank(a).cmp(&anim_rank(b)).then(a.cmp(b)));
    let mut out = Vec::new();
    for name in names {
        let dirs = doc.anims[&name].dir_list();
        if dirs.is_empty() {
            out.push((name, None));
        } else {
            for dir in dirs {
                out.push((name.clone(), Some(dir)));
            }
        }
    }
    Ok(out)
}

pub(super) fn rows_for(
    id: &str,
    doc: &PxlDoc,
    frames: &BTreeMap<String, IndexFrame>,
) -> CmdResult<Vec<Vec<SheetCell>>> {
    if doc.anims.is_empty() {
        let mut row = Vec::new();
        for (name, frame) in frames {
            row.push(SheetCell {
                name: format!("{id}_{name}"),
                frame: frame.clone(),
                duration_ms: 1000,
                tag: Some(id.to_string()),
            });
        }
        return Ok(vec![row]);
    }
    let mut names: Vec<String> = doc.anims.keys().cloned().collect();
    names.sort_by(|a, b| anim_rank(a).cmp(&anim_rank(b)).then(a.cmp(b)));
    let mut rows = Vec::new();
    for name in names {
        let anim = &doc.anims[&name];
        let duration = (1000 / anim.fps.max(1)).max(1);
        let dirs = anim.dir_list();
        if dirs.is_empty() {
            let seq = anim.frames.sequence(None)?;
            rows.push(cells_for(id, &name, &seq, frames, duration)?);
        } else {
            for dir in dirs {
                let seq = anim.frames.sequence(Some(&dir))?;
                let tag = format!("{name}_{dir}");
                rows.push(cells_for(id, &tag, &seq, frames, duration)?);
            }
        }
    }
    Ok(rows)
}

pub(super) fn cells_for(
    id: &str,
    tag: &str,
    names: &[String],
    frames: &BTreeMap<String, IndexFrame>,
    duration: u32,
) -> CmdResult<Vec<SheetCell>> {
    let mut row = Vec::new();
    for name in names {
        let frame = frames.get(name).ok_or_else(|| {
            PxlError::semantic(format!("animation references missing frame '{name}'"))
        })?;
        row.push(SheetCell {
            name: format!("{id}_{name}"),
            frame: frame.clone(),
            duration_ms: duration,
            tag: Some(tag.to_string()),
        });
    }
    Ok(row)
}

pub(super) const FACING_NAMES: &[&str] = &["N", "E", "S", "W", "down", "up", "left", "right"];

pub(super) fn anim_rank(name: &str) -> u8 {
    match name {
        "move" | "walk" => 0,
        "idle" => 1,
        "hoe" => 2,
        _ => 3,
    }
}
