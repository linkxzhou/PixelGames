use std::path::Path;

use pxl_core::{Palette, Rgba};
use pxl_export::{encode_gif_timed, encode_indexed_png, review_image, ReviewMode};
use pxl_format::PxlDoc;
use serde_json::{json, Value};

use super::sheet::{pack_loaded, Loaded};
use super::visual::VisualContext;
use crate::error::CmdResult;
use crate::util;

pub(super) fn write_visual_artifacts(
    out_dir: &Path,
    recipe_dir: &Path,
    loaded: &[Loaded],
    visual: Option<&VisualContext>,
    visual_reports: Vec<Value>,
) -> CmdResult<Vec<String>> {
    let mut outputs = Vec::new();
    let mut previews = Vec::new();
    let mut metadata = Vec::new();
    for asset in loaded {
        let id = &asset.id;
        let doc = &asset.doc;
        let base_name = if doc.frame("base").is_ok() {
            "base".to_string()
        } else {
            doc.frame_names()[0].clone()
        };
        let base = doc.frame(&base_name)?;
        let review_scale = (256 / base.w.max(base.h)).clamp(1, 4);
        let modes = [
            ("native".to_string(), ReviewMode::Native, 1, None),
            (format!("nearest-{review_scale}x"), ReviewMode::Native, review_scale, None),
            ("light".into(), ReviewMode::Native, review_scale, Some(Rgba::rgb(240, 240, 232))),
            ("dark".into(), ReviewMode::Native, review_scale, Some(Rgba::rgb(24, 24, 32))),
            ("silhouette".into(), ReviewMode::Silhouette, review_scale, Some(Rgba::rgb(24, 24, 32))),
            ("grayscale".into(), ReviewMode::Grayscale, review_scale, None),
            ("posterize3".into(), ReviewMode::Posterize3, review_scale, None),
        ];
        for (label, mode, scale, background) in modes {
            let (frame, palette) = review_image(&base, &asset.palette, mode, scale, background)?;
            let bytes = encode_indexed_png(&frame, &palette, 1)?;
            let name = format!("{id}.{label}.png");
            util::write_out(out_dir.join(&name).to_str().unwrap(), &bytes, true, false)?;
            outputs.push(name.clone());
            previews.push(preview_entry(id, &base_name, &label, scale, background, doc, &name, &bytes)?);
        }
        for (anim_name, anim) in &doc.anims {
            let dirs = anim.dir_list();
            let names = anim.frames.sequence(dirs.first().map(String::as_str))?;
            let frames = names
                .iter()
                .map(|name| doc.frame(name))
                .collect::<Result<Vec<_>, _>>()?;
            let default = (1000 / anim.fps.max(1)).max(1);
            let delays = (0..frames.len())
                .map(|index| {
                    doc.meta
                        .durations
                        .get(anim_name)
                        .and_then(|durations| durations.get(index))
                        .copied()
                        .unwrap_or(default)
                        .min(u32::from(u16::MAX)) as u16
                })
                .collect::<Vec<_>>();
            let bytes = encode_gif_timed(&frames, &asset.palette, 2, &delays, anim.looped)?;
            let name = format!("{id}.{anim_name}.gif");
            util::write_out(out_dir.join(&name).to_str().unwrap(), &bytes, true, false)?;
            outputs.push(name.clone());
            previews.push(preview_entry(id, anim_name, "animation", 2, None, doc, &name, &bytes)?);
        }
        let plan = visual.and_then(|context| {
            context
                .plan
                .assets
                .iter()
                .find(|plan| plan.asset_id == *id)
        });
        let mut channels = Vec::new();
        if let (Some(context), Some(plan)) = (visual, plan) {
            for (channel, enabled) in [
                ("material", plan.channels.material_labels),
                ("emissive", plan.channels.emissive),
                ("shadow", plan.channels.shadow),
            ] {
                if !enabled {
                    continue;
                }
                let channel_doc = context.channel_doc(doc, recipe_dir, channel)?;
                let channel_name = format!("{id}.{channel}");
                util::save_doc(
                    &channel_doc,
                    out_dir.join(format!("{channel_name}.pxl.json")).to_str().unwrap(),
                    true,
                    false,
                )?;
                let channel_palette = channel_palette()?;
                let channel_loaded = Loaded {
                    id: id.clone(),
                    doc: channel_doc,
                    palette: channel_palette,
                    family: None,
                };
                let (png, json_text, _, _, _, _, _) = pack_loaded(
                    &[channel_loaded],
                    "rows-by-anim",
                    1,
                    &format!("{channel_name}.sheet.png"),
                )?;
                util::write_out(
                    out_dir.join(format!("{channel_name}.sheet.png")).to_str().unwrap(),
                    &png,
                    true,
                    false,
                )?;
                util::write_out(
                    out_dir.join(format!("{channel_name}.sheet.json")).to_str().unwrap(),
                    json_text.as_bytes(),
                    true,
                    false,
                )?;
                outputs.extend([
                    format!("{channel_name}.pxl.json"),
                    format!("{channel_name}.sheet.png"),
                    format!("{channel_name}.sheet.json"),
                ]);
                channels.push(json!({
                    "id": channel,
                    "document": format!("{channel_name}.pxl.json"),
                    "sheet": format!("{channel_name}.sheet.json"),
                    "image": format!("{channel_name}.sheet.png"),
                    "palette": "channel.palette.json",
                }));
            }
        }
        let anims = doc.anims.iter().map(|(name, anim)| {
            let duration = doc.meta.durations.get(name).map(|durations| {
                durations.iter().copied().sum::<u32>()
            }).unwrap_or_else(|| {
                let frames = anim.frames.sequence(anim.dir_list().first().map(String::as_str))
                    .map(|frames| frames.len())
                    .unwrap_or(0);
                (frames * 1000 / anim.fps.max(1) as usize) as u32
            });
            (name.clone(), json!({
                "fps": anim.fps,
                "loop": anim.looped,
                "duration_ms": duration,
                "frame_durations_ms": doc.meta.durations.get(name),
            }))
        }).collect::<serde_json::Map<_, _>>();
        metadata.push(json!({
            "asset_id": id,
            "role": plan.map(|plan| plan.role.clone()),
            "native_size": [doc.w, doc.h],
            "pivot": plan.and_then(|plan| plan.animations.first().map(|anim| anim.anchor)),
            "material_ids": plan.map(|plan| {
                let mut ids = plan.regions.iter().map(|region| region.material_id.clone()).collect::<Vec<_>>();
                ids.sort();
                ids.dedup();
                ids
            }),
            "channels": channels,
            "animations": anims,
            "source_digest": util::hash_of(pxl_format::to_pretty(doc)?.as_bytes()),
        }));
    }
    if loaded.iter().any(|asset| {
        visual.is_some_and(|context| {
            context.plan.assets.iter().any(|plan| {
                plan.asset_id == asset.id
                    && (plan.channels.material_labels || plan.channels.emissive || plan.channels.shadow)
            })
        })
    }) {
        let palette = channel_palette()?;
        util::write_out(
            out_dir.join("channel.palette.json").to_str().unwrap(),
            &super::visual::palette_bytes(&palette)?,
            true,
            false,
        )?;
        outputs.push("channel.palette.json".into());
    }
    let report = json!({
        "contract": "visual-report/v1",
        "verdict": "PASS",
        "visual_review": "not_run",
        "assets": visual_reports,
        "applicability": {"scene_semantics": "not_assessed", "human_aesthetics": "not_assessed"},
    });
    write_json(out_dir, "visual-report.json", &report)?;
    outputs.push("visual-report.json".into());
    let metadata = json!({
        "contract": "visual-metadata/v1",
        "material_encoding": material_encoding(),
        "assets": metadata,
    });
    write_json(out_dir, "visual-metadata.json", &metadata)?;
    outputs.push("visual-metadata.json".into());
    let preview_manifest = json!({
        "contract": "preview-manifest/v1",
        "production_assets": false,
        "previews": previews,
    });
    write_json(out_dir, "preview-manifest.json", &preview_manifest)?;
    outputs.push("preview-manifest.json".into());
    Ok(outputs)
}

fn preview_entry(
    asset_id: &str,
    frame: &str,
    mode: &str,
    scale: u16,
    background: Option<Rgba>,
    doc: &PxlDoc,
    name: &str,
    bytes: &[u8],
) -> CmdResult<Value> {
    Ok(json!({
        "asset_id": asset_id,
        "frame": frame,
        "mode": mode,
        "scale": scale,
        "background": background.map(|color| color.to_hex()),
        "path": name,
        "source_digest": util::hash_of(pxl_format::to_pretty(doc)?.as_bytes()),
        "output_digest": util::hash_of(bytes),
    }))
}

fn channel_palette() -> CmdResult<Palette> {
    let mut colors = vec![Rgba::TRANSPARENT];
    for index in 1..=15u8 {
        let value = index * 16;
        colors.push(Rgba::rgb(value, value, value));
    }
    let palette = Palette {
        id: "visual_channels".into(),
        colors,
        ramps: Default::default(),
        cycles: Default::default(),
        special: pxl_core::Special {
            transparent: 0,
            outline: 1,
            flash: 1,
        },
    };
    palette.validate()?;
    Ok(palette)
}

fn material_encoding() -> Value {
    json!({
        "organic": 1, "outline": 2, "metal": 3, "wood": 4, "stone": 5,
        "water": 6, "foliage": 7, "cloth": 8, "skin": 9, "hair": 10,
        "glass": 11, "emissive": 12, "flat": 13,
    })
}

fn write_json(out_dir: &Path, name: &str, value: &Value) -> CmdResult<()> {
    let mut text = pxl_format::to_pretty(value)?;
    text.push('\n');
    util::write_out(out_dir.join(name).to_str().unwrap(), text.as_bytes(), true, false)
        .map(|_| ())
}
