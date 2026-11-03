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

/// Extra pack constraints beyond layout name + padding triad.
#[derive(Clone, Debug, Default)]
pub struct SheetPackOpts {
    pub pad: SheetPadding,
    /// Fixed columns (cells per row). Overrides auto grid column count.
    pub columns: Option<u32>,
    /// Fixed row count → columns = ceil(n / rows). Ignored if `columns` set.
    pub rows: Option<u32>,
    /// Wrap cells when the next placement would exceed this atlas width.
    pub sheet_width: Option<u32>,
    /// Fail if packed height exceeds this (after power-of-two expand).
    pub sheet_height: Option<u32>,
    /// Expand final atlas sides to the next power of two (≤2048).
    pub power_of_two: bool,
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
/// `vertical` is one cell per row. `horizontal` is a single row of all cells.
pub fn pack_sheet(rows: &[Vec<SheetCell>], layout: &str, pad: u32) -> PxlResult<PackedSheet> {
    pack_sheet_opts(
        rows,
        layout,
        SheetPackOpts {
            pad: SheetPadding::shape_only(pad),
            ..SheetPackOpts::default()
        },
    )
}

pub fn pack_sheet_padded(
    rows: &[Vec<SheetCell>],
    layout: &str,
    pad: SheetPadding,
) -> PxlResult<PackedSheet> {
    pack_sheet_opts(
        rows,
        layout,
        SheetPackOpts {
            pad,
            ..SheetPackOpts::default()
        },
    )
}

pub fn pack_sheet_opts(
    rows: &[Vec<SheetCell>],
    layout: &str,
    opts: SheetPackOpts,
) -> PxlResult<PackedSheet> {
    let flat: Vec<&SheetCell> = rows.iter().flatten().collect();
    if flat.is_empty() {
        return Err(PxlError::semantic("sheet has no frames"));
    }
    let layout = layout.to_ascii_lowercase();
    let shape = i64::from(opts.pad.shape);
    let border = i64::from(opts.pad.border);
    let inner = i64::from(opts.pad.inner);

    // Width-constrained shelf pack ignores row grouping.
    if let Some(max_w) = opts.sheet_width.filter(|w| *w > 0) {
        return pack_shelf(&flat, opts, max_w, shape, border, inner);
    }

    let groups: Vec<Vec<&SheetCell>> = if let Some(cols) = opts.columns.filter(|c| *c > 0) {
        flat.chunks(cols as usize).map(|c| c.to_vec()).collect()
    } else if let Some(nrows) = opts.rows.filter(|r| *r > 0) {
        let cols = ((flat.len() as u32 + nrows - 1) / nrows).max(1) as usize;
        flat.chunks(cols).map(|c| c.to_vec()).collect()
    } else {
        match layout.as_str() {
            "rows-by-anim" | "rows" | "horizontal" | "h" => rows
                .iter()
                .map(|r| r.iter().collect())
                .filter(|r: &Vec<_>| !r.is_empty())
                .collect(),
            "vertical" | "v" | "columns" => flat.iter().map(|c| vec![*c]).collect(),
            "grid" | "packed" => {
                let n = flat.len();
                let mut cols = 1usize;
                while cols * cols < n {
                    cols += 1;
                }
                flat.chunks(cols).map(|c| c.to_vec()).collect()
            }
            other => {
                return Err(PxlError::usage(format!("unknown sheet layout '{other}'"))
                    .with_hint("layout is horizontal|vertical|rows|grid|packed"));
            }
        }
    };

    let mut placements = Vec::new();
    let mut y = border;
    let mut sheet_w = 1i64;
    for row in &groups {
        let mut x = border;
        let mut row_h = 1i64;
        for cell in row {
            let cw = cell.frame.w as i64 + inner * 2;
            let ch = cell.frame.h as i64 + inner * 2;
            placements.push((x, y, *cell));
            row_h = row_h.max(ch);
            x += cw + shape;
        }
        if x > border {
            sheet_w = sheet_w.max(x - shape + border);
        }
        y += row_h + shape;
    }
    let sheet_h = if y > border { y - shape + border } else { 1 };
    finish_sheet(placements, sheet_w.max(1), sheet_h.max(1), &opts)
}

fn pack_shelf(
    flat: &[&SheetCell],
    opts: SheetPackOpts,
    max_w: u32,
    shape: i64,
    border: i64,
    inner: i64,
) -> PxlResult<PackedSheet> {
    let max_w = max_w as i64;
    let mut placements = Vec::new();
    let mut x = border;
    let mut y = border;
    let mut row_h = 1i64;
    let mut sheet_w = border;
    for cell in flat {
        let cw = cell.frame.w as i64 + inner * 2;
        let ch = cell.frame.h as i64 + inner * 2;
        if cw + border * 2 > max_w {
            return Err(PxlError::semantic(format!(
                "cell {}x{} does not fit sheet-width {max_w}",
                cell.frame.w, cell.frame.h
            )));
        }
        if x > border && x + cw + border > max_w {
            sheet_w = sheet_w.max(x - shape + border);
            x = border;
            y += row_h + shape;
            row_h = 1;
        }
        placements.push((x, y, *cell));
        row_h = row_h.max(ch);
        x += cw + shape;
    }
    sheet_w = sheet_w.max(if x > border { x - shape + border } else { border + 1 });
    sheet_w = sheet_w.max(max_w.min(sheet_w.max(1)));
    // Prefer exact sheet_width when set: pad canvas to requested width.
    let sheet_w = max_w.max(sheet_w);
    let sheet_h = if y > border || row_h > 0 {
        y + row_h + border
    } else {
        1
    };
    finish_sheet(placements, sheet_w, sheet_h.max(1), &opts)
}

fn finish_sheet(
    placements: Vec<(i64, i64, &SheetCell)>,
    mut sheet_w: i64,
    mut sheet_h: i64,
    opts: &SheetPackOpts,
) -> PxlResult<PackedSheet> {
    if opts.power_of_two {
        sheet_w = next_pow2_u32(sheet_w as u32).min(2048) as i64;
        sheet_h = next_pow2_u32(sheet_h as u32).min(2048) as i64;
    }
    if let Some(max_h) = opts.sheet_height.filter(|h| *h > 0) {
        if sheet_h > max_h as i64 {
            return Err(PxlError::semantic(format!(
                "sheet height {sheet_h} exceeds --sheet-height {max_h}"
            )));
        }
    }
    if sheet_w > 2048 || sheet_h > 2048 {
        return Err(PxlError::semantic(format!(
            "sheet would be {sheet_w}x{sheet_h}, over the 2048 limit"
        ))
        .with_hint("use fewer frames, smaller padding, or sheet-width wrapping"));
    }
    let mut image = IndexFrame::canvas(sheet_w as u16, sheet_h as u16)?;
    let mut out_places = Vec::new();
    for (x, y, cell) in placements {
        let (x, y) = (x as i32, y as i32);
        let ox = x + opts.pad.inner as i32;
        let oy = y + opts.pad.inner as i32;
        for py in 0..cell.frame.h as i32 {
            for px in 0..cell.frame.w as i32 {
                image.set(ox + px, oy + py, cell.frame.get(px, py));
            }
        }
        out_places.push(Placement {
            name: cell.name.clone(),
            x: x as u32,
            y: y as u32,
            w: (cell.frame.w as u32) + opts.pad.inner * 2,
            h: (cell.frame.h as u32) + opts.pad.inner * 2,
            duration_ms: cell.duration_ms,
            tag: cell.tag.clone(),
        });
    }
    Ok(PackedSheet {
        image,
        placements: out_places,
    })
}

fn next_pow2_u32(v: u32) -> u32 {
    let v = v.max(1);
    if v >= 2048 {
        return 2048;
    }
    v.next_power_of_two()
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
#[path = "sheet_tests.rs"]
mod tests;
