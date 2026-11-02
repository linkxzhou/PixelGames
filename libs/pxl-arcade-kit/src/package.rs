//! `400-package` loading: the only filesystem read a game performs.
//!
//! Sheets are indexed PNGs; they are expanded to RGBA here so the renderer
//! uploads one texture per sheet and never thinks about palettes again.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Frame {
    /// Opaque bounds inside this frame rectangle, in texture pixels.
    pub fn bbox(&self, sheet: &Sheet) -> Option<Frame> {
        let mut min_x = self.w;
        let mut min_y = self.h;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut found = false;
        for y in 0..self.h {
            for x in 0..self.w {
                let sx = self.x + x;
                let sy = self.y + y;
                if sx >= sheet.w || sy >= sheet.h {
                    continue;
                }
                let alpha = sheet.rgba[((sy * sheet.w + sx) * 4 + 3) as usize];
                if alpha > 0 {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        found.then(|| Frame {
            x: min_x,
            y: min_y,
            w: max_x - min_x + 1,
            h: max_y - min_y + 1,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anim {
    pub name: String,
    pub fps: u32,
    pub looped: bool,
    pub frames: Vec<String>,
}

impl Anim {
    /// Frame key for a tick clock, honouring the loop flag.
    pub fn key(&self, elapsed_ms: u64) -> Option<&str> {
        if self.frames.is_empty() {
            return None;
        }
        let fps = self.fps.max(1) as u64;
        let step = elapsed_ms * fps / 1000;
        let index = if self.looped {
            (step % self.frames.len() as u64) as usize
        } else {
            (step as usize).min(self.frames.len() - 1)
        };
        self.frames.get(index).map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub id: String,
    pub path: PathBuf,
    pub json: PathBuf,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub frames: BTreeMap<String, Frame>,
    pub order: Vec<String>,
}

impl Sheet {
    pub fn frame(&self, key: &str) -> Option<Frame> {
        self.frames.get(key).copied()
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: String,
    pub sheet: String,
    pub kind: String,
    pub anims: BTreeMap<String, Anim>,
    /// Union of every frame's opaque bounds: a stable sprite box that stops
    /// animation frames from jittering when their silhouettes differ.
    pub content: Option<Frame>,
}

impl Asset {
    pub fn anim(&self, name: &str) -> Option<&Anim> {
        self.anims.get(name).or_else(|| self.anims.values().next())
    }
}

#[derive(Debug, Clone)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub grid_w: u32,
    pub grid_h: u32,
    pub grid: Vec<String>,
    pub spawn: [u32; 2],
    pub exit: [u32; 2],
    pub raw: Value,
}

impl Level {
    pub fn tile(&self, x: u32, y: u32) -> Option<char> {
        if x >= self.grid_w || y >= self.grid_h {
            return None;
        }
        self.grid
            .get(y as usize)
            .and_then(|row| row.chars().nth(x as usize))
    }

    pub fn columns(&self) -> Vec<Vec<char>> {
        self.grid.iter().map(|row| row.chars().collect()).collect()
    }
}

#[derive(Debug, Clone)]
pub struct Package {
    pub root: PathBuf,
    pub package_id: String,
    pub palette: Palette,
    pub sheets: BTreeMap<String, Sheet>,
    pub assets: BTreeMap<String, Asset>,
    pub levels: Vec<Level>,
}

impl Package {
    pub fn load(dir: &Path) -> Result<Package, String> {
        let dir = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let doc = read_json(&dir.join("asset-package.json"))?;

        let palette_rel = doc["palette"]["path"]
            .as_str()
            .ok_or_else(|| "asset-package.json names no palette".to_string())?;
        let palette = Palette::load(&resolve(&dir, palette_rel))?;

        let mut sheets = BTreeMap::new();
        for entry in doc["sheets"].as_array().cloned().unwrap_or_default() {
            let id = entry["id"].as_str().unwrap_or_default().to_string();
            let rel = entry["path"]
                .as_str()
                .ok_or_else(|| format!("sheet '{id}' has no path"))?;
            let sheet = Sheet::load(id.clone(), resolve(&dir, rel))?;
            sheets.insert(id, sheet);
        }

        let mut assets = BTreeMap::new();
        for entry in doc["assets"].as_array().cloned().unwrap_or_default() {
            let id = entry["id"].as_str().unwrap_or_default().to_string();
            let sheet_id = entry["sheet"].as_str().unwrap_or_default().to_string();
            let sheet = sheets
                .get(&sheet_id)
                .ok_or_else(|| format!("asset '{id}' names a missing sheet '{sheet_id}'"))?;
            let anims = anims_for(&id, sheet);
            let mut content: Option<Frame> = None;
            for anim in anims.values() {
                for key in &anim.frames {
                    if let Some(bbox) = sheet.frame(key).and_then(|frame| frame.bbox(sheet)) {
                        content = Some(union(content, bbox));
                    }
                }
            }
            if content.is_none() {
                for key in sheet.frames.keys().filter(|key| key.starts_with(&format!("{id}_"))) {
                    if let Some(bbox) = sheet.frame(key).and_then(|frame| frame.bbox(sheet)) {
                        content = Some(union(content, bbox));
                    }
                }
            }
            let kind = entry["kind"].as_str().unwrap_or("sprite").to_string();
            assets.insert(
                id.clone(),
                Asset {
                    id,
                    sheet: sheet_id,
                    kind,
                    anims,
                    content,
                },
            );
        }

        let mut levels = Vec::new();
        if let Some(rel) = doc["levels"]["path"].as_str() {
            let levels_dir = resolve(&dir, rel);
            let mut files: Vec<PathBuf> = fs::read_dir(&levels_dir)
                .map_err(|e| format!("levels {rel}: {e}"))?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect();
            files.sort();
            for file in files {
                levels.push(Level::from_json(&file)?);
            }
        }

        Ok(Package {
            root: dir,
            package_id: doc["package_id"].as_str().unwrap_or_default().to_string(),
            palette,
            sheets,
            assets,
            levels,
        })
    }

    pub fn asset(&self, id: &str) -> Result<&Asset, String> {
        self.assets
            .get(id)
            .ok_or_else(|| format!("package has no asset '{id}'"))
    }

    pub fn sheet_of(&self, asset: &Asset) -> Result<&Sheet, String> {
        self.sheets
            .get(&asset.sheet)
            .ok_or_else(|| format!("asset '{}' sheet '{}' missing", asset.id, asset.sheet))
    }

    pub fn level(&self, index: usize) -> &Level {
        &self.levels[index.min(self.levels.len().saturating_sub(1))]
    }
}

impl Sheet {
    pub fn load(id: String, json_path: PathBuf) -> Result<Sheet, String> {
        let doc = read_json(&json_path)?;
        let base = json_path
            .parent()
            .ok_or_else(|| format!("{} has no parent", json_path.display()))?
            .to_path_buf();
        let image = doc["meta"]["image"]
            .as_str()
            .ok_or_else(|| format!("{}.sheet.json: meta.image missing", id))?;
        let png = base.join(image);
        let (w, h, rgba) = decode_png(&png)?;
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
        Ok(Sheet {
            id,
            path: png,
            json: json_path,
            w,
            h,
            rgba,
            frames,
            order,
        })
    }
}

impl Level {
    fn from_json(path: &Path) -> Result<Level, String> {
        let raw = read_json(path)?;
        Ok(Level {
            id: raw["id"].as_str().unwrap_or_default().to_string(),
            name: raw["name"].as_str().unwrap_or_default().to_string(),
            grid_w: raw["grid_w"].as_u64().unwrap_or(0) as u32,
            grid_h: raw["grid_h"].as_u64().unwrap_or(0) as u32,
            grid: raw["grid"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| row.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            spawn: pair(&raw["spawn"]),
            exit: pair(&raw["exit"]),
            raw,
        })
    }
}

fn pair(value: &Value) -> [u32; 2] {
    let list = value.as_array();
    [
        list.and_then(|items| items.first())
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        list.and_then(|items| items.get(1))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    ]
}

fn union(current: Option<Frame>, next: Frame) -> Frame {
    match current {
        None => next,
        Some(frame) => {
            let x = frame.x.min(next.x);
            let y = frame.y.min(next.y);
            let right = (frame.x + frame.w).max(next.x + next.w);
            let bottom = (frame.y + frame.h).max(next.y + next.h);
            Frame {
                x,
                y,
                w: right - x,
                h: bottom - y,
            }
        }
    }
}

/// Frame lists per animation. The sheet document carries them under
/// `meta.pxl.anims[<name>].dirs["_"]`; a hand-edited sheet falls back to the
/// `<asset>_<anim>_<i>` naming the exporter uses.
fn anims_for(id: &str, sheet: &Sheet) -> BTreeMap<String, Anim> {
    let mut out = BTreeMap::new();
    if let Some(map) = sheet_doc_anims(sheet).as_ref().and_then(Value::as_object) {
        for (name, anim) in map {
            let frames = anim["dirs"]["_"]
                .as_array()
                .or_else(|| anim["frames"].as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if frames.is_empty() {
                continue;
            }
            out.insert(
                name.clone(),
                Anim {
                    name: name.clone(),
                    fps: anim["fps"].as_u64().unwrap_or(8) as u32,
                    looped: anim["loop"].as_bool().unwrap_or(true),
                    frames,
                },
            );
        }
    }
    if out.is_empty() {
        for key in &sheet.order {
            let Some(rest) = key.strip_prefix(&format!("{id}_")) else {
                continue;
            };
            let Some((name, _)) = rest.rsplit_once('_') else {
                continue;
            };
            let anim = out.entry(name.to_string()).or_insert_with(|| Anim {
                name: name.to_string(),
                fps: 8,
                looped: true,
                frames: Vec::new(),
            });
            anim.frames.push(key.clone());
        }
    }
    out
}

fn sheet_doc_anims(sheet: &Sheet) -> Option<Value> {
    // `Sheet` keeps only the decoded pixels; the document is re-read once so a
    // missing `meta.pxl` block costs nothing at runtime.
    read_json(&sheet.json)
        .ok()
        .map(|doc| doc["meta"]["pxl"]["anims"].clone())
        .filter(|value| value.is_object())
}

pub fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn resolve(root: &Path, rel: &str) -> PathBuf {
    let path = Path::new(rel);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Expand an indexed PNG (PLTE + tRNS) to RGBA; truecolour input is accepted
/// so a re-exported sheet still loads.
pub fn decode_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let palette = reader.info().palette.clone();
    let trns = reader.info().trns.clone().unwrap_or_default();
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let width = info.width;
    let height = info.height;
    let pixels = (width * height) as usize;
    let bytes = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Indexed => {
            let plte = palette.ok_or_else(|| format!("{}: indexed PNG without PLTE", path.display()))?;
            let mut out = Vec::with_capacity(pixels * 4);
            for index in bytes.iter().take(pixels) {
                let at = *index as usize * 3;
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
        other => return Err(format!("{}: unsupported colour type {other:?}", path.display())),
    };
    Ok((width, height, rgba))
}
