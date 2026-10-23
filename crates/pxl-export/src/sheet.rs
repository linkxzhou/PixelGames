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

/// `rows-by-anim` keeps caller row grouping. `grid` packs in a roughly square grid.
pub fn pack_sheet(rows: &[Vec<SheetCell>], layout: &str, pad: u32) -> PxlResult<PackedSheet> {
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
    let pad = pad as i32;
    let mut placements = Vec::new();
    let mut y = 0i32;
    let mut sheet_w = 1i32;
    for row in &groups {
        let mut x = 0i32;
        let mut row_h = 1i32;
        for cell in row {
            let cw = cell.frame.w as i32;
            let ch = cell.frame.h as i32;
            placements.push((x, y, cell));
            row_h = row_h.max(ch);
            x += cw + pad;
        }
        if x > 0 {
            sheet_w = sheet_w.max(x - pad);
        }
        y += row_h + pad;
    }
    let sheet_h = if y > 0 { y - pad } else { 1 };
    let mut image = IndexFrame::canvas(sheet_w.max(1) as u16, sheet_h.max(1) as u16)?;
    let mut out_places = Vec::new();
    for (x, y, cell) in placements {
        for py in 0..cell.frame.h as i32 {
            for px in 0..cell.frame.w as i32 {
                image.set(x + px, y + py, cell.frame.get(px, py));
            }
        }
        out_places.push(Placement {
            name: cell.name.clone(),
            x: x as u32,
            y: y as u32,
            w: cell.frame.w as u32,
            h: cell.frame.h as u32,
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

pub fn sheet_json(packed: &PackedSheet, image_name: &str, pxl: Value) -> PxlResult<String> {
    let mut frames = Map::new();
    let mut tags: Vec<FrameTag> = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for (i, p) in packed.placements.iter().enumerate() {
        let entry = AseFrame {
            frame: AseRect {
                x: p.x,
                y: p.y,
                w: p.w,
                h: p.h,
            },
            rotated: false,
            trimmed: false,
            sprite_source_size: AseRect {
                x: 0,
                y: 0,
                w: p.w,
                h: p.h,
            },
            source_size: Size { w: p.w, h: p.h },
            duration: p.duration_ms,
        };
        frames.insert(
            p.name.clone(),
            serde_json::to_value(entry).map_err(|e| PxlError::io(e.to_string()))?,
        );
        match (&current, &p.tag) {
            (None, Some(tag)) => current = Some((tag.clone(), i as u32)),
            (Some((name, _from)), Some(tag)) if name == tag => {}
            (Some((name, from)), _) => {
                tags.push(FrameTag {
                    name: name.clone(),
                    from: *from,
                    to: i as u32 - 1,
                    direction: "forward".into(),
                });
                current = p.tag.clone().map(|t| (t, i as u32));
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
    let doc = serde_json::json!({
        "frames": frames,
        "meta": meta,
    });
    let mut s = serde_json::to_string_pretty(&doc).map_err(|e| PxlError::io(e.to_string()))?;
    s.push('\n');
    Ok(s)
}
