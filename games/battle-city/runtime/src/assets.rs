//! Asset package loading (`400-package`).
//!
//! The package is the single input: it names the palette, the sheets with
//! their digests, the assets and the levels. Sheets are indexed (PLTE + tRNS)
//! PNGs decoded here into RGBA so the renderer only needs one texture upload.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::level::Level;
use crate::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub from: usize,
    pub to: usize,
}

/// One decoded sheet: RGBA pixels plus the Aseprite frame table.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub id: String,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub frames: BTreeMap<String, Frame>,
    /// Frame keys in sheet order, so tag `from`/`to` indices resolve.
    pub order: Vec<String>,
    pub tags: Vec<Tag>,
}

impl Sheet {
    pub fn frame(&self, key: &str) -> Option<Frame> {
        self.frames.get(key).copied()
    }

    pub fn tag(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|t| t.name == name)
    }

    /// Frames for one animation facing. Tries the `<anim>_<dir>` tag first,
    /// then the `<sprite>_<anim>_<dir>_<i>` frame keys.
    pub fn anim_frames(&self, sprite: &str, anim: &str, dir: &str) -> Vec<Frame> {
        let tag_name = format!("{anim}_{dir}");
        if let Some(tag) = self.tag(&tag_name) {
            let frames: Vec<Frame> = (tag.from..=tag.to)
                .filter_map(|i| self.order.get(i))
                .filter_map(|key| self.frame(key))
                .collect();
            if !frames.is_empty() {
                return frames;
            }
        }
        if let Some(tag) = self.tag(anim) {
            let frames: Vec<Frame> = (tag.from..=tag.to)
                .filter_map(|i| self.order.get(i))
                .filter_map(|key| self.frame(key))
                .collect();
            if !frames.is_empty() {
                return frames;
            }
        }
        let prefix = format!("{sprite}_{anim}_{dir}_");
        self.order
            .iter()
            .filter(|key| key.starts_with(&prefix))
            .filter_map(|key| self.frame(key))
            .collect()
    }

    /// First frame of an animation facing, or `None`.
    pub fn first_frame(&self, sprite: &str, anim: &str, dir: &str) -> Option<Frame> {
        self.anim_frames(sprite, anim, dir).first().copied()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnimInfo {
    #[serde(default = "one")]
    pub frames: u32,
    #[serde(default = "one_fps")]
    pub fps: u32,
    #[serde(default = "yes")]
    pub r#loop: bool,
}

fn one() -> u32 {
    1
}
fn one_fps() -> u32 {
    8
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub id: String,
    pub sheet: String,
    #[serde(default)]
    pub anims: BTreeMap<String, AnimInfo>,
}

/// Everything the runtime needs, loaded from one package directory.
#[derive(Debug, Clone)]
pub struct Package {
    pub root: PathBuf,
    pub package_id: String,
    pub palette: Palette,
    pub sheets: BTreeMap<String, Sheet>,
    pub assets: Vec<Asset>,
    pub levels: Vec<Level>,
}

impl Package {
    pub fn asset_ids(&self) -> BTreeSet<String> {
        self.assets.iter().map(|a| a.id.clone()).collect()
    }

    /// Sheet of an asset id, if the package delivers it.
    pub fn sheet_for(&self, asset_id: &str) -> Option<&Sheet> {
        let asset = self.assets.iter().find(|a| a.id == asset_id)?;
        self.sheets.get(&asset.sheet)
    }
}

/// Load a package from `dir` (the directory holding `asset-package.json`).
pub fn load_package(dir: &Path) -> Result<Package, String> {
    let text = fs::read_to_string(dir.join("asset-package.json"))
        .map_err(|e| format!("{}/asset-package.json: {e}", dir.display()))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("asset-package.json: {e}"))?;

    let palette_rel = doc["palette"]["path"]
        .as_str()
        .ok_or("asset-package.json names no palette")?;
    let palette_text = fs::read_to_string(dir.join(palette_rel))
        .map_err(|e| format!("palette {palette_rel}: {e}"))?;
    let palette = Palette::from_json(&palette_text)?;

    let mut sheets = BTreeMap::new();
    for entry in doc["sheets"].as_array().cloned().unwrap_or_default() {
        let id = entry["id"].as_str().unwrap_or_default().to_string();
        let rel = entry["path"]
            .as_str()
            .ok_or_else(|| format!("sheet '{id}' has no path"))?;
        let sheet = load_sheet(dir.join(rel))?;
        sheets.insert(id, sheet);
    }

    let assets: Vec<Asset> = doc["assets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(|e| format!("asset entry: {e}")))
        .collect::<Result<_, _>>()?;

    let mut levels = Vec::new();
    if let Some(rel) = doc["levels"]["path"].as_str() {
        let known: BTreeSet<String> = assets.iter().map(|a| a.id.clone()).collect();
        let dir_path = dir.join(rel);
        let mut files: Vec<PathBuf> = fs::read_dir(&dir_path)
            .map_err(|e| format!("levels {rel}: {e}"))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            levels.push(Level::from_json(&text, &known)?);
        }
    }

    Ok(Package {
        root: dir.to_path_buf(),
        package_id: doc["package_id"].as_str().unwrap_or_default().to_string(),
        palette,
        sheets,
        assets,
        levels,
    })
}

/// Load one sheet json plus the PNG it points at.
pub fn load_sheet(json_path: PathBuf) -> Result<Sheet, String> {
    let text =
        fs::read_to_string(&json_path).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let base = json_path
        .parent()
        .ok_or_else(|| format!("{} has no parent", json_path.display()))?
        .to_path_buf();
    let image = doc["meta"]["image"]
        .as_str()
        .ok_or_else(|| format!("{}: meta.image missing", json_path.display()))?;
    let png_path = base.join(image);
    let (w, h, rgba) = load_indexed_rgba(&png_path)?;

    let mut frames = BTreeMap::new();
    let mut order = Vec::new();
    if let Some(map) = doc["frames"].as_object() {
        for (key, value) in map {
            let frame = &value["frame"];
            frames.insert(
                key.clone(),
                Frame {
                    x: frame["x"].as_u64().unwrap_or(0) as u32,
                    y: frame["y"].as_u64().unwrap_or(0) as u32,
                    w: frame["w"].as_u64().unwrap_or(0) as u32,
                    h: frame["h"].as_u64().unwrap_or(0) as u32,
                },
            );
            order.push(key.clone());
        }
    }
    let mut tags = Vec::new();
    for tag in doc["meta"]["frameTags"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        if let Some(name) = tag["name"].as_str() {
            tags.push(Tag {
                name: name.to_string(),
                from: tag["from"].as_u64().unwrap_or(0) as usize,
                to: tag["to"].as_u64().unwrap_or(0) as usize,
            });
        }
    }
    let id = base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Sheet {
        id,
        w,
        h,
        rgba,
        frames,
        order,
        tags,
    })
}

/// Decode an indexed PNG (PLTE + tRNS) into RGBA. Falls back to truecolour
/// input so a re-exported sheet still loads.
pub fn load_indexed_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // `Info` carries PLTE/tRNS; `next_frame` borrows the reader mutably, so the
    // colour table is copied out first.
    let plte = reader.info().palette.clone();
    let trns = reader.info().trns.clone().unwrap_or_default();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (info.width, info.height);
    let pixels = (w * h) as usize;
    let bytes = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Indexed => {
            let plte = plte
                .as_ref()
                .ok_or_else(|| format!("{}: indexed PNG without PLTE", path.display()))?;
            let mut out = Vec::with_capacity(pixels * 4);
            for index in bytes.iter().take(pixels) {
                let at = (*index as usize) * 3;
                if at + 2 >= plte.len() {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let alpha = trns.get(*index as usize).copied().unwrap_or(255);
                out.extend_from_slice(&[plte[at], plte[at + 1], plte[at + 2], alpha]);
            }
            out
        }
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(pixels * 4);
            for chunk in bytes.chunks_exact(3) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => {
            return Err(format!(
                "{}: unsupported colour type {:?}",
                path.display(),
                info.color_type
            ));
        }
    };
    Ok((w, h, rgba))
}

impl Sheet {
    /// Convenience: the RGBA of one frame, row-major.
    pub fn frame_pixels(&self, frame: Frame) -> Vec<u8> {
        let mut out = Vec::with_capacity((frame.w * frame.h * 4) as usize);
        for y in 0..frame.h {
            for x in 0..frame.w {
                let sx = frame.x + x;
                let sy = frame.y + y;
                if sx >= self.w || sy >= self.h {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let at = ((sy * self.w + sx) * 4) as usize;
                out.extend_from_slice(&self.rgba[at..at + 4]);
            }
        }
        out
    }
}
