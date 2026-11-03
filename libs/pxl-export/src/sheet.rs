use pxl_core::{IndexFrame, PxlError, PxlResult};
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Clone, Debug)]
pub struct SheetCell {
    pub name: String,
    pub frame: IndexFrame,
    pub duration_ms: u32,
    pub tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
    pub tag: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PackedSheet {
    pub image: IndexFrame,
    pub placements: Vec<Placement>,
}

/// Official Aseprite sheet padding triad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheetPadding {
    /// Empty margin around the whole atlas.
    pub border: u32,
    /// Gap between packed cells.
    pub shape: u32,
    /// Extra transparent margin inside each cell rect (content inset).
    pub inner: u32,
}

impl SheetPadding {
    pub fn shape_only(shape: u32) -> Self {
        Self {
            border: 0,
            shape,
            inner: 0,
        }
    }
}

/// The whole-atlas memory budget of one packed sheet (P08). `rgba8_bytes`
/// is the level-1 texture estimate — width × height × 4 bytes at 1x — never
/// the PNG size on disk. A sheet is always one page today: multi-page
/// packing is deferred until a real sample proves it necessary, and any
/// sheet that cannot fit under `page_limit` fails closed in `pack_sheet`.
#[derive(Clone, Debug, Serialize)]
pub struct SheetBudget {
    pub width: u32,
    pub height: u32,
    pub pages: u32,
    pub page_limit: u32,
    pub frames: u32,
    /// width × height × pages × 4, the RGBA8 texture estimate in bytes.
    pub rgba8_bytes: u64,
    /// Same estimate in mebibytes, rounded to two decimals.
    pub rgba8_mib: f64,
}

impl SheetBudget {
    pub fn of(packed: &PackedSheet) -> Self {
        let pixels = u64::from(packed.image.w) * u64::from(packed.image.h);
        let rgba8_bytes = pixels * 4;
        Self {
            width: packed.image.w as u32,
            height: packed.image.h as u32,
            pages: 1,
            page_limit: 1,
            frames: packed.placements.len() as u32,
            rgba8_bytes,
            rgba8_mib: (rgba8_bytes as f64) / (1024.0 * 1024.0),
        }
    }
}


/// `rows-by-anim` keeps caller row grouping. `grid` packs in a roughly square grid.
pub fn pack_sheet(rows: &[Vec<SheetCell>], layout: &str, pad: u32) -> PxlResult<PackedSheet> {
    pack_sheet_padded(rows, layout, SheetPadding::shape_only(pad))
}

pub fn pack_sheet_padded(
    rows: &[Vec<SheetCell>],
    layout: &str,
    pad: SheetPadding,
) -> PxlResult<PackedSheet> {
    let flat: Vec<&SheetCell> = rows.iter().flatten().collect();
    if flat.is_empty() {
        return Err(PxlError::semantic("sheet has no frames"));
    }
    let groups: Vec<Vec<&SheetCell>> = match layout {
        "rows-by-anim" | "rows" => rows
            .iter()
            .map(|r| r.iter().collect())
            .filter(|r: &Vec<_>| !r.is_empty())
            .collect(),
        "grid" => {
            let n = flat.len();
            let mut cols = 1usize;
            while cols * cols < n {
                cols += 1;
            }
            flat.chunks(cols).map(|c| c.to_vec()).collect()
        }
        other => {
            return Err(PxlError::usage(format!("unknown sheet layout '{other}'"))
                .with_hint("layout is rows-by-anim or grid"));
        }
    };
    let shape = i64::from(pad.shape);
    let border = i64::from(pad.border);
    let inner = i64::from(pad.inner);
    let mut placements = Vec::new();
    let mut y = border;
    let mut sheet_w = 1i64;
    for row in &groups {
        let mut x = border;
        let mut row_h = 1i64;
        for cell in row {
            let cw = cell.frame.w as i64 + inner * 2;
            let ch = cell.frame.h as i64 + inner * 2;
            placements.push((x, y, cell));
            row_h = row_h.max(ch);
            x += cw + shape;
        }
        if x > border {
            sheet_w = sheet_w.max(x - shape + border);
        }
        y += row_h + shape;
    }
    let sheet_h = if y > border { y - shape + border } else { 1 };
    let (sheet_w, sheet_h) = (sheet_w.max(1), sheet_h.max(1));
    if sheet_w > 2048 || sheet_h > 2048 {
        return Err(PxlError::semantic(format!(
            "sheet would be {sheet_w}x{sheet_h}, over the 2048 limit"
        ))
        .with_hint("use fewer frames, a smaller --pad, or the grid layout"));
    }
    let mut image = IndexFrame::canvas(sheet_w as u16, sheet_h as u16)?;
    let mut out_places = Vec::new();
    for (x, y, cell) in placements {
        let (x, y) = (x as i32, y as i32);
        let ox = x + pad.inner as i32;
        let oy = y + pad.inner as i32;
        for py in 0..cell.frame.h as i32 {
            for px in 0..cell.frame.w as i32 {
                image.set(ox + px, oy + py, cell.frame.get(px, py));
            }
        }
        out_places.push(Placement {
            name: cell.name.clone(),
            x: x as u32,
            y: y as u32,
            w: (cell.frame.w as u32) + pad.inner * 2,
            h: (cell.frame.h as u32) + pad.inner * 2,
            duration_ms: cell.duration_ms,
            tag: cell.tag.clone(),
        });
    }
    Ok(PackedSheet {
        image,
        placements: out_places,
    })
}

#[derive(Serialize)]
struct AseRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct AseFrame {
    frame: AseRect,
    rotated: bool,
    trimmed: bool,
    #[serde(rename = "spriteSourceSize")]
    sprite_source_size: AseRect,
    #[serde(rename = "sourceSize")]
    source_size: Size,
    duration: u32,
}

#[derive(Serialize)]
struct Size {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct FrameTag {
    name: String,
    from: u32,
    to: u32,
    direction: String,
}

/// The budget block `sheet_json` embeds under `meta.pxl.budget` and step-400
/// re-serializes into coverage.json. One canonical shape, two consumers.
pub fn sheet_budget_json(packed: &PackedSheet) -> Value {
    serde_json::to_value(SheetBudget::of(packed))
        .map_err(|e| PxlError::io(e.to_string()))
        .unwrap_or_else(|_| Value::Null)
}

pub fn sheet_json(packed: &PackedSheet, image_name: &str, pxl: Value) -> PxlResult<String> {
    sheet_json_format(packed, image_name, pxl, "json-hash")
}

/// `format`: `json-hash` (frames object, default) or `json-array` (frames array).
pub fn sheet_json_format(
    packed: &PackedSheet,
    image_name: &str,
    pxl: Value,
    format: &str,
) -> PxlResult<String> {
    let as_array = match format {
        "json-array" | "array" => true,
        "json-hash" | "hash" | "" => false,
        other => {
            return Err(PxlError::usage(format!(
                "unknown sheet data format '{other}' (json-hash|json-array)"
            )));
        }
    };
    let mut frames_map = Map::new();
    let mut frames_arr: Vec<Value> = Vec::new();
    let mut tags: Vec<FrameTag> = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for (i, pl) in packed.placements.iter().enumerate() {
        let entry = AseFrame {
            frame: AseRect {
                x: pl.x,
                y: pl.y,
                w: pl.w,
                h: pl.h,
            },
            rotated: false,
            trimmed: false,
            sprite_source_size: AseRect {
                x: 0,
                y: 0,
                w: pl.w,
                h: pl.h,
            },
            source_size: Size { w: pl.w, h: pl.h },
            duration: pl.duration_ms,
        };
        let val = serde_json::to_value(entry).map_err(|e| PxlError::io(e.to_string()))?;
        if as_array {
            let mut obj = Map::new();
            obj.insert("filename".into(), Value::String(pl.name.clone()));
            if let Value::Object(m) = val {
                for (k, v) in m {
                    obj.insert(k, v);
                }
            }
            frames_arr.push(Value::Object(obj));
        } else {
            frames_map.insert(pl.name.clone(), val);
        }
        match (&current, &pl.tag) {
            (None, Some(tag)) => current = Some((tag.clone(), i as u32)),
            (Some((name, _from)), Some(tag)) if name == tag => {}
            (Some((name, from)), _) => {
                tags.push(FrameTag {
                    name: name.clone(),
                    from: *from,
                    to: i as u32 - 1,
                    direction: "forward".into(),
                });
                current = pl.tag.clone().map(|tg| (tg, i as u32));
            }
            (None, None) => {}
        }
    }
    if let Some((name, from)) = current {
        tags.push(FrameTag {
            name,
            from,
            to: packed.placements.len() as u32 - 1,
            direction: "forward".into(),
        });
    }
    let meta = serde_json::json!({
        "app": "pxl",
        "version": pxl_core::PXL_VERSION,
        "image": image_name,
        "format": "indexed8",
        "size": { "w": packed.image.w, "h": packed.image.h },
        "scale": "1",
        "frameTags": tags,
        "pxl": pxl,
    });
    let frames_value = if as_array {
        Value::Array(frames_arr)
    } else {
        Value::Object(frames_map)
    };
    let doc = serde_json::json!({
        "frames": frames_value,
        "meta": meta,
    });
    let mut s = serde_json::to_string_pretty(&doc).map_err(|e| PxlError::io(e.to_string()))?;
    s.push_str("\n");
    Ok(s)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn cell(name: &str, w: u16) -> SheetCell {
        SheetCell {
            name: name.into(),
            frame: IndexFrame::new(w, 4).unwrap(),
            duration_ms: 100,
            tag: None,
        }
    }

    #[test]
    fn pad_does_not_wrap_and_oversize_sheets_are_rejected() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let ok = pack_sheet(&rows, "rows-by-anim", 1).unwrap();
        assert_eq!((ok.image.w, ok.image.h), (9, 4));
        let err = pack_sheet(&rows, "rows-by-anim", u32::MAX).unwrap_err();
        assert!(err.message.contains("2048"), "{err:?}");
        let wide: Vec<SheetCell> = (0..40).map(|i| cell(&format!("f{i}"), 64)).collect();
        assert!(pack_sheet(&[wide], "rows-by-anim", 1).is_err());
    }

    #[test]
    fn budget_reports_rgba8_estimate_not_png_size() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let packed = pack_sheet(&rows, "rows-by-anim", 1).unwrap();
        let triad = pack_sheet_padded(
            &rows,
            "rows-by-anim",
            SheetPadding {
                border: 2,
                shape: 3,
                inner: 1,
            },
        )
        .unwrap();
        assert!(triad.image.w > packed.image.w);
        assert_eq!(triad.placements[0].x, 2);
        assert_eq!(triad.placements[0].w, rows[0][0].frame.w as u32 + 2);

        let budget = SheetBudget::of(&packed);
        // 9x4 sheet: the RGBA8 estimate is w*h*4, not the indexed PNG size.
        assert_eq!(budget.rgba8_bytes, 9 * 4 * 4);
        assert_eq!(budget.frames, 2);
        assert_eq!(budget.pages, 1);
        assert_eq!(budget.page_limit, 1);
        // A full 2048x2048 single page tops out at exactly 16 MiB. Frames
        // cannot reach that size (MAX_DIM is 256), so build the sheet
        // directly and check the arithmetic.
        let full = PackedSheet {
            image: IndexFrame::canvas(2048, 2048).unwrap(),
            placements: Vec::new(),
        };
        assert_eq!(SheetBudget::of(&full).rgba8_bytes, 2048 * 2048 * 4);
        assert_eq!(SheetBudget::of(&full).rgba8_mib, 16.0);
    }


    #[test]
    fn json_array_frames_is_array_hash_is_object() {
        let rows = vec![vec![cell("a", 4), cell("b", 4)]];
        let packed = pack_sheet(&rows, "rows-by-anim", 0).unwrap();
        let hash = sheet_json_format(&packed, "s.png", serde_json::json!({}), "json-hash").unwrap();
        let arr = sheet_json_format(&packed, "s.png", serde_json::json!({}), "json-array").unwrap();
        let hv: serde_json::Value = serde_json::from_str(&hash).unwrap();
        let av: serde_json::Value = serde_json::from_str(&arr).unwrap();
        assert!(hv["frames"].is_object());
        assert!(av["frames"].is_array());
        assert_eq!(av["frames"].as_array().unwrap().len(), 2);
    }
}
