//! ASCII text grid to [`AsciiSpec`]: parse the blocks, fill the colours the
//! text does not carry, and validate. No filesystem access; the shared legend
//! and palette come through a [`Resolver`].

use std::collections::{BTreeMap, BTreeSet};

use pxl_core::{sha256_hex, Palette, PxlError, PxlResult};
use pxl_format::{
    parse_ascii_text, Anchor, AsciiBlockRef, AsciiFrame, AsciiLayer, AsciiSource, AsciiSpec,
    AsciiTextOptions, BlockKind, Charset, LegendValue, PaletteMode,
};

use crate::resolver::{load_legend_file, load_palette, Resolver};

/// Asset kinds an ASCII layout may declare. `parts` is a rig, not a layout.
const ASCII_KINDS: &[&str] = &["sprite", "tiles", "effect", "ui", "overlay"];

const PLACEHOLDER: &str = "#ff00ffff";

#[derive(Debug, Clone)]
pub struct AsciiImportOptions {
    pub charset: Option<Charset>,
    pub size: u16,
    pub cell: Option<String>,
    pub anchor: Anchor,
    pub palette: Option<String>,
    /// Name the resolver looks the shared legend up by.
    pub legend: Option<String>,
    /// How the spec stores `legend_ref`. A CLI rewrites it relative to the spec
    /// file; callers without a directory pass the same text as `legend`.
    pub legend_ref: Option<String>,
    pub blocks: BlockKind,
    pub pad: bool,
    pub directives: bool,
    pub kind: Option<String>,
    /// The caller passed `size` itself, so a `:: size` line that disagrees is
    /// an error rather than a silent override.
    pub size_explicit: bool,
}

impl Default for AsciiImportOptions {
    fn default() -> Self {
        Self {
            charset: None,
            size: 32,
            cell: None,
            anchor: Anchor::default(),
            palette: None,
            legend: None,
            legend_ref: None,
            blocks: BlockKind::Frames,
            pad: false,
            directives: true,
            kind: None,
            size_explicit: false,
        }
    }
}

pub fn check_canvas_size(size: u16) -> PxlResult<()> {
    if !pxl_core::SIZE_TIERS.contains(&size) {
        return Err(PxlError::schema(format!(
            "ascii.bad_size: canvas size {size} is not 32, 64, 128 or 256"
        ))
        .with_path("/size")
        .with_hint("ascii layouts use a 32, 64, 128 or 256 pixel square canvas"));
    }
    Ok(())
}

/// Parse `source` (the text grid) into a spec. `input` names the text for
/// messages and for `source.file`.
pub fn build_spec(
    opts: &AsciiImportOptions,
    source: &str,
    input: &str,
    res: &dyn Resolver,
) -> PxlResult<(AsciiSpec, Vec<String>)> {
    check_canvas_size(opts.size)?;
    let legend_file = load_legend_file(opts.legend.as_deref(), res)?;
    let charset = opts
        .charset
        .or_else(|| legend_file.as_ref().and_then(|l| l.charset))
        .unwrap_or_default();
    let mut text_opts = AsciiTextOptions::new(charset, opts.blocks);
    text_opts.pad = opts.pad;
    text_opts.directives = opts.directives;
    text_opts.file = Some(input.to_string());
    let text = parse_ascii_text(source, &text_opts)?;
    // A `:: size 128` line in the file beats the default; an explicit option
    // from the caller beats the file.
    let mut size = opts.size;
    if let Some(n) = text.size {
        if opts.size_explicit {
            if opts.size != n {
                return Err(PxlError::semantic(format!(
                    "ascii.size_conflict: the file says size {n}, the command says {}",
                    opts.size
                ))
                .with_path("/size")
                .with_hint("drop --size to follow the file, or make them agree"));
            }
        } else {
            size = n;
        }
    }
    check_canvas_size(size)?;

    let palette_ref = opts
        .palette
        .clone()
        .or_else(|| legend_file.as_ref().and_then(|l| l.palette.clone()));
    let palette = match &palette_ref {
        Some(name) => Some(load_palette(name, res)?),
        None => None,
    };
    let shared = legend_file
        .as_ref()
        .map(|l| l.legend.clone())
        .unwrap_or_default();
    let used = used_chars_in_blocks(&text.blocks);
    let legend = auto_legend(charset, palette.as_ref(), &shared, &used)?;
    let cell = parse_cell(opts.cell.as_deref(), size, text.width, text.height, input)?;

    let mut frames = BTreeMap::new();
    let mut layers = Vec::new();
    for block in &text.blocks {
        match block.kind {
            BlockKind::Frames => {
                frames.insert(
                    block.name.clone(),
                    AsciiFrame::Rows {
                        rows: block.rows.clone(),
                    },
                );
            }
            BlockKind::Layers => layers.push(AsciiLayer {
                name: block.name.clone(),
                rows: block.rows.clone(),
                x: 0,
                y: 0,
                visible: true,
            }),
        }
    }
    if let Some(kind) = &opts.kind {
        check_kind(kind)?;
    }
    let spec = AsciiSpec {
        encoding: text.encoding.unwrap_or_default(),
        v: 1,
        charset,
        size,
        cell,
        anchor: opts.anchor,
        offset: [0, 0],
        palette: palette_ref,
        palette_mode: PaletteMode::Strict,
        legend_ref: opts.legend_ref.clone(),
        legend,
        keep: None,
        layers,
        frames,
        anims: Vec::new(),
        post: Vec::new(),
        mirror: None,
        kind: opts.kind.clone(),
        source: Some(AsciiSource {
            file: input.to_string(),
            sha256: sha256_hex(source.as_bytes()),
            blocks: text
                .blocks
                .iter()
                .map(|b| AsciiBlockRef {
                    name: b.name.clone(),
                    line: b.line,
                })
                .collect(),
        }),
    };
    spec.validate()?;
    Ok((spec, text.warnings))
}

fn auto_legend(
    charset: Charset,
    palette: Option<&Palette>,
    shared: &BTreeMap<String, LegendValue>,
    used: &BTreeSet<char>,
) -> PxlResult<BTreeMap<String, LegendValue>> {
    let transparent = charset.default_transparent();
    let mut legend = BTreeMap::new();
    match (charset, palette) {
        (Charset::Digits, Some(_)) => {}
        (Charset::Digits, None) => {
            for c in used {
                if transparent.contains(c) || shared.contains_key(&c.to_string()) {
                    continue;
                }
                legend.insert(
                    c.to_string(),
                    LegendValue::Entry(pxl_format::LegendEntry {
                        color: Some(PLACEHOLDER.into()),
                        auto: true,
                        ..Default::default()
                    }),
                );
            }
        }
        (Charset::Printable, Some(palette)) => {
            let mut taken = taken_indices(shared, palette);
            let mut next = 2u8;
            for c in used {
                if transparent.contains(c) || shared.contains_key(&c.to_string()) {
                    continue;
                }
                while (next as usize) < palette.len() && taken.contains(&next) {
                    next += 1;
                }
                if (next as usize) >= palette.len() {
                    return Err(PxlError::semantic(format!(
                        "ascii.palette_full: charset printable needs a colour for '{c}' but {} is full",
                        palette.id
                    ))
                    .with_hint("map more symbols in the legend file, or use fewer symbols"));
                }
                taken.insert(next);
                legend.insert(
                    c.to_string(),
                    LegendValue::Entry(pxl_format::LegendEntry {
                        index: Some(next),
                        auto: true,
                        ..Default::default()
                    }),
                );
                next += 1;
            }
        }
        (Charset::Printable, None) => {
            for c in used {
                if transparent.contains(c) || shared.contains_key(&c.to_string()) {
                    continue;
                }
                legend.insert(
                    c.to_string(),
                    LegendValue::Entry(pxl_format::LegendEntry {
                        color: Some(PLACEHOLDER.into()),
                        auto: true,
                        ..Default::default()
                    }),
                );
            }
        }
    }
    Ok(legend)
}

fn taken_indices(shared: &BTreeMap<String, LegendValue>, palette: &Palette) -> BTreeSet<u8> {
    let mut taken = BTreeSet::new();
    taken.insert(palette.special.transparent);
    taken.insert(palette.special.outline);
    taken.insert(palette.special.flash);
    for value in shared.values() {
        if let Some(index) = value_index(value, palette) {
            taken.insert(index);
        }
    }
    taken
}

fn value_index(value: &LegendValue, palette: &Palette) -> Option<u8> {
    match value {
        LegendValue::Index(index) => Some(*index),
        LegendValue::Text(text) => {
            if text.starts_with('#') {
                let color = pxl_core::parse_hex(text).ok()?;
                palette
                    .colors
                    .iter()
                    .position(|c| *c == color)
                    .map(|i| i as u8)
            } else {
                role_index(pxl_format::Role::parse(text).ok()?, palette)
            }
        }
        LegendValue::Entry(entry) => {
            if let Some(index) = entry.index {
                return Some(index);
            }
            if let Some(role) = &entry.role {
                return role_index(pxl_format::Role::parse(role).ok()?, palette);
            }
            if let Some(text) = &entry.color {
                let color = pxl_core::parse_hex(text).ok()?;
                return palette
                    .colors
                    .iter()
                    .position(|c| *c == color)
                    .map(|i| i as u8);
            }
            None
        }
    }
}

fn role_index(role: pxl_format::Role, palette: &Palette) -> Option<u8> {
    Some(match role {
        pxl_format::Role::Transparent => palette.special.transparent,
        pxl_format::Role::Outline => palette.special.outline,
        pxl_format::Role::Flash => palette.special.flash,
    })
}

fn parse_cell(
    text: Option<&str>,
    size: u16,
    cols: usize,
    rows: usize,
    input: &str,
) -> PxlResult<[u16; 2]> {
    let Some(text) = text else {
        return Ok([1, 1]);
    };
    if text == "fit" {
        let span = cols.max(rows).max(1) as u16;
        let fit = size / span;
        if fit == 0 {
            return Err(PxlError::schema(format!(
                "ascii.size_overflow: {input} is {cols}x{rows} cells, which does not fit a {size}x{size} canvas"
            ))
            .with_path("/cell")
            .with_hint("use size 64, or fewer columns"));
        }
        return Ok([fit, fit]);
    }
    let (w, h) = match text.split_once('x') {
        Some((w, h)) => (w, h),
        None => (text, text),
    };
    let parse = |value: &str| -> PxlResult<u16> {
        value.parse::<u16>().map_err(|_| {
            PxlError::schema(format!("ascii.bad_cell: '{value}' is not a pixel count"))
                .with_path("/cell")
                .with_hint("cell is N, NxM, or fit")
        })
    };
    let (w, h) = (parse(w)?, parse(h)?);
    if w == 0 || h == 0 {
        return Err(
            PxlError::schema("ascii.bad_cell: a cell needs at least 1x1 pixels").with_path("/cell"),
        );
    }
    Ok([w, h])
}

fn check_kind(kind: &str) -> PxlResult<()> {
    if ASCII_KINDS.contains(&kind) {
        return Ok(());
    }
    Err(PxlError::schema(format!("unknown ascii kind '{kind}'"))
        .with_path("/kind")
        .with_hint("kind is sprite, tiles, effect, ui, or overlay"))
}

fn used_chars_in_blocks(blocks: &[pxl_format::AsciiBlock]) -> BTreeSet<char> {
    let mut used = BTreeSet::new();
    for block in blocks {
        for row in &block.rows {
            for c in row.chars() {
                used.insert(c);
            }
        }
    }
    used
}
