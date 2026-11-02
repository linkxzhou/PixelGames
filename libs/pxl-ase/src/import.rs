//! Aseprite → steps import bridge (M8).

use std::path::{Path, PathBuf};

use pxl_ase_doc::{ColorMode, Sprite};
use pxl_core::Rgba;
use pxl_format::{Recipe, RecipeBase, RecipeSheet, RecipeSprite};
use serde_json::{json, Value};

use crate::render::flatten_frame_rgba;
use crate::{
    export_frame_png, export_sheet_png_json_layout_opts, sprite_palette, AseErr, Result,
    SheetExportOpts,
};

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub out_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub recipe_path: PathBuf,
    pub palette_path: PathBuf,
    pub frame_paths: Vec<PathBuf>,
    pub sheet_png: Option<PathBuf>,
    pub sheet_json: Option<PathBuf>,
    pub warnings: Vec<String>,
}

/// Import an aseprite document into a schema-valid recipe + frames layout (§5.1).
pub fn import_ase_to_dir(
    sprite: &Sprite,
    source_path: &Path,
    out_dir: &Path,
    asset_name: &str,
) -> Result<ImportReport> {
    std::fs::create_dir_all(out_dir).map_err(|e| AseErr(e.to_string()))?;
    let frames_dir = out_dir.join("frames");
    std::fs::create_dir_all(&frames_dir).map_err(|e| AseErr(e.to_string()))?;

    let mut warnings = Vec::new();
    let source_sha = sha256_file(source_path).unwrap_or_else(|| {
        warnings.push("source_sha256 unavailable (read failed)".into());
        String::new()
    });

    // palette.json (PaletteFile shape)
    let pal_colors: Vec<String> = if sprite.color_mode == ColorMode::Indexed && !sprite.palette.is_empty()
    {
        sprite.palette.iter().map(|c| c.to_hex()).collect()
    } else {
        let mut samples = Vec::new();
        for f in 0..sprite.frame_count() {
            samples.extend_from_slice(&flatten_frame_rgba(sprite, f as u16)?);
        }
        let colors = crate::color::quantize_rgba(&samples, 32);
        if colors.is_empty() {
            vec![Rgba::TRANSPARENT.to_hex()]
        } else {
            colors.iter().map(|c| c.to_hex()).collect()
        }
    };
    let palette_path = out_dir.join("palette.json");
    let palette_json = json!({
        "v": 1,
        "id": asset_name,
        "colors": pal_colors,
        "ramps": {},
        "cycles": {},
        "special": {}
    });
    write_json(&palette_path, &palette_json)?;

    // frames
    let mut frame_paths = Vec::new();
    for f in 0..sprite.frame_count() {
        let name = format!("frame_{f:04}.png");
        let path = frames_dir.join(&name);
        let png = export_frame_png(sprite, f as u16, 1)?;
        std::fs::write(&path, png).map_err(|e| AseErr(e.to_string()))?;
        frame_paths.push(path);
    }

    // optional sheet
    let frames: Vec<u16> = (0..sprite.frame_count() as u16).collect();
    let (png, js) = export_sheet_png_json_layout_opts(
        sprite,
        &frames,
        "sheet.png",
        "horizontal",
        &SheetExportOpts::default(),
    )?;
    let sheet_png = out_dir.join("sheet.png");
    let sheet_json = out_dir.join("sheet.json");
    std::fs::write(&sheet_png, png).map_err(|e| AseErr(e.to_string()))?;
    std::fs::write(&sheet_json, js).map_err(|e| AseErr(e.to_string()))?;

    // Schema-valid Recipe (typed) — no unknown fields. Registration path:
    // palette + empty sprites + sheet pointing at imported sheet file.
    // Frames are consumed as pre-rendered assets (not re-gen); gen_version matches.
    let recipe = Recipe {
        v: 1,
        asset: asset_name.to_string(),
        gen_version: pxl_core::GEN_VERSION,
        seed: 0,
        palette: "palette.json".into(),
        sprites: vec![RecipeSprite {
            id: asset_name.to_string(),
            base: RecipeBase {
                gen: "ascii".into(),
                spec: Some("imported.ascii.json".into()),
                template: None,
                seed: Some(0),
                ramp: None,
                mirror: None,
                frame: Some("base".into()),
                palette: None,
            },
            post: vec![],
            directions: vec![],
            anims: vec![],
            kind: Some("sprite".into()),
            tile_ramp: None,
            anchors: Default::default(),
            protected_regions: Default::default(),
        }],
        tiles: vec![],
        sheet: RecipeSheet {
            sources: vec![asset_name.to_string()],
            layout: "rows-by-anim".into(),
            pad: 0,
            file: "sheet.sheet.png".into(),
        },
        preview: vec![],
        validate: None,
    };
    // Round-trip through serde to guarantee no extra fields on disk.
    let recipe_value = serde_json::to_value(&recipe).map_err(|e| AseErr(e.to_string()))?;
    let recipe_check: Recipe =
        serde_json::from_value(recipe_value.clone()).map_err(|e| AseErr(e.to_string()))?;
    let _ = recipe_check;
    let recipe_path = out_dir.join("asset.recipe.json");
    write_json(&recipe_path, &recipe_value)?;

    // Minimal ascii registration so the recipe references a real file.
    // Digits charset: map index 0..=9; higher indices collapse with a warning.
    let ascii_path = out_dir.join("imported.ascii.json");
    let ascii = indexed_frame0_to_ascii(sprite, asset_name, &mut warnings)?;
    write_json(&ascii_path, &ascii)?;

    let tags: Vec<Value> = sprite
        .tags
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "from": t.from,
                "to": t.to,
            })
        })
        .collect();
    let manifest = json!({
        "schema": "ase-import/v1",
        "source_path": source_path.display().to_string(),
        "source_sha256": source_sha,
        "pxl_version": pxl_core::PXL_VERSION,
        "gen_version": pxl_core::GEN_VERSION,
        "asset": asset_name,
        "width": sprite.width,
        "height": sprite.height,
        "color_mode": match sprite.color_mode {
            ColorMode::Rgba => "rgba",
            ColorMode::Grayscale => "grayscale",
            ColorMode::Indexed => "indexed",
        },
        "frame_count": sprite.frame_count(),
        "tags": tags,
        "warnings": warnings,
        "capabilities": ["ase_import_v1"],
        "outputs": {
            "palette": "palette.json",
            "recipe": "asset.recipe.json",
            "ascii": "imported.ascii.json",
            "frames_dir": "frames",
            "sheet_png": "sheet.png",
            "sheet_json": "sheet.json",
        }
    });
    let manifest_path = out_dir.join("import-manifest.json");
    write_json(&manifest_path, &manifest)?;

    // Copy/link note: do not rewrite games/; source stays caller-owned.
    Ok(ImportReport {
        out_dir: out_dir.to_path_buf(),
        manifest_path,
        recipe_path,
        palette_path,
        frame_paths,
        sheet_png: Some(sheet_png),
        sheet_json: Some(sheet_json),
        warnings: manifest["warnings"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut s = serde_json::to_string_pretty(value).map_err(|e| AseErr(e.to_string()))?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    std::fs::write(path, s).map_err(|e| AseErr(e.to_string()))
}

fn sha256_file(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    // Lightweight FNV-1a 64 as stand-in when sha2 not in deps; label clearly.
    // Prefer real sha256 if available via std hashing is not crypto — document as fnv64.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in buf {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(format!("fnv1a64:{hash:016x}"))
}

fn indexed_frame0_to_ascii(sprite: &Sprite, asset: &str, warnings: &mut Vec<String>) -> Result<Value> {
    let rgba = flatten_frame_rgba(sprite, 0)?;
    let pal = if sprite.color_mode == ColorMode::Indexed && !sprite.palette.is_empty() {
        sprite_palette(sprite)
    } else {
        let colors = crate::color::quantize_rgba(&rgba, 16);
        pxl_core::Palette {
            id: "ase-import".into(),
            colors,
            ramps: Default::default(),
            cycles: Default::default(),
            special: Default::default(),
        }
    };
    let indices = pxl_art::quantize::map_to_palette(
        sprite.width as usize,
        sprite.height as usize,
        &rgba,
        &pal,
        0,
    );
    let mut rows = Vec::new();
    let w = sprite.width as usize;
    let h = sprite.height as usize;
    let mut used_high = false;
    for y in 0..h {
        let mut row = String::with_capacity(w);
        for x in 0..w {
            let i = indices[y * w + x];
            if i > 9 {
                used_high = true;
                row.push(char::from_digit((i % 10) as u32, 10).unwrap_or('0'));
            } else {
                row.push(char::from_digit(i as u32, 10).unwrap_or('0'));
            }
        }
        rows.push(row);
    }
    if used_high {
        warnings.push(
            "ascii import collapsed palette indices >9 into digit mod 10; treat as preview registration"
                .into(),
        );
    }
    Ok(json!({
        "v": 1,
        "charset": "digits",
        "size": sprite.width.max(sprite.height),
        "cell": [1, 1],
        "anchor": "center",
        "offset": [0, 0],
        "palette": "palette.json",
        "palette_mode": "strict",
        "legend": {},
        "layers": [],
        "frames": {
            "base": { "rows": rows }
        },
        "anims": [],
        "post": [],
        "source": {
            "file": format!("ase-import:{asset}"),
            "sha256": "",
            "blocks": [{ "name": "base", "line": 1 }]
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pxl_ase_doc::{Cel, ColorMode, ImageBuffer, IndexedImage, Sprite};
    use pxl_core::Rgba;

    #[test]
    fn import_writes_schema_valid_recipe() {
        let mut s = Sprite::new(2, 2, ColorMode::Indexed);
        s.palette = vec![Rgba::TRANSPARENT, Rgba::rgb(255, 0, 0)];
        s.frame_durations_ms = vec![100];
        let layer = s.add_image_layer("a");
        let img = IndexedImage::from_indices(2, 2, vec![0, 1, 1, 0]);
        let id = s.insert_image(ImageBuffer::Indexed(img));
        s.set_cel(layer, 0, Cel::image(id, 0, 0, 255));
        let dir = std::env::temp_dir().join(format!("pxl-ase-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let report = import_ase_to_dir(&s, Path::new("mem.aseprite"), &dir, "pilot").unwrap();
        let text = std::fs::read_to_string(&report.recipe_path).unwrap();
        let recipe: Recipe = pxl_format::parse_json(&text, "recipe").unwrap();
        assert_eq!(recipe.asset, "pilot");
        assert_eq!(recipe.gen_version, pxl_core::GEN_VERSION);
        let man: Value =
            serde_json::from_str(&std::fs::read_to_string(&report.manifest_path).unwrap()).unwrap();
        assert_eq!(man["schema"], "ase-import/v1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
