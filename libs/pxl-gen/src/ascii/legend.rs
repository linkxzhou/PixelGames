//! Legend resolution: characters to palette indices, patterns, and stamps,
//! including palette derivation and extension.

use std::collections::BTreeMap;

use pxl_core::{Cycle, Palette, PxlError, PxlResult, Rgba, Special};
use pxl_format::{
    AsciiSpec, CycleFile, LegendSource, LegendValue, PaletteFile, PaletteMode, Role, SpecialFile,
};

use super::frames::stamp_pixels;
use super::{CellPaint, PatternKind};

pub(super) struct LegendPlan {
    pub(super) paints: BTreeMap<char, CellPaint>,
    pub(super) palette: Palette,
    pub(super) palette_file: Option<PaletteFile>,
    pub(super) auto_chars: Vec<char>,
}

struct EntryInfo {
    source: LegendSource,
    pattern: Option<String>,
    alt: Option<u8>,
}

struct Working {
    colors: Vec<Rgba>,
    ramps: BTreeMap<String, Vec<u8>>,
    cycles: BTreeMap<String, Cycle>,
    special: Special,
    derived: bool,
    strict: bool,
    appended: bool,
}

pub(super) fn resolve_legend(
    spec: &AsciiSpec,
    given: Option<&Palette>,
    shared: &BTreeMap<String, LegendValue>,
) -> PxlResult<LegendPlan> {
    let mut merged: BTreeMap<String, LegendValue> = shared.clone();
    for (key, value) in &spec.legend {
        merged.insert(key.clone(), value.clone());
    }
    pxl_format::check_legend_keys(&merged, spec.charset, "/legend")?;

    let mut infos: BTreeMap<char, EntryInfo> = BTreeMap::new();
    let mut auto_chars = Vec::new();
    for (key, value) in &merged {
        let c = key.chars().next().ok_or_else(|| {
            PxlError::schema("legend key is empty").with_path(format!("/legend/{key}"))
        })?;
        let info = entry_info(value, &format!("/legend/{key}"))?;
        if let LegendValue::Entry(entry) = value {
            if entry.auto {
                auto_chars.push(c);
            }
        }
        infos.insert(c, info);
    }

    let (has_outline, outline_hint) = outline_prepass(&merged)?;
    let mut work = match given {
        Some(palette) => Working {
            colors: palette.colors.clone(),
            ramps: palette.ramps.clone(),
            cycles: palette.cycles.clone(),
            special: palette.special.clone(),
            derived: false,
            strict: spec.palette_mode == PaletteMode::Strict,
            appended: false,
        },
        None => {
            let mut colors = vec![Rgba::TRANSPARENT];
            let mut special = Special {
                transparent: 0,
                outline: 0,
                flash: 0,
            };
            if has_outline {
                colors.push(outline_hint.unwrap_or(Rgba::rgb(0, 0, 0)));
                special.outline = 1;
            }
            Working {
                colors,
                ramps: BTreeMap::new(),
                cycles: BTreeMap::new(),
                special,
                derived: true,
                strict: false,
                appended: false,
            }
        }
    };

    let mut paints = BTreeMap::new();
    for (c, info) in &infos {
        let path = format!("/legend/{c}");
        let mut ramp_neighbor = None;
        let base = match &info.source {
            LegendSource::Index(index) => {
                if work.derived && *index != 0 {
                    return Err(PxlError::semantic(format!(
                        "ascii.derive_requires_color: legend '{c}' uses index {index} without a palette"
                    ))
                    .with_path(&path)
                    .with_hint("pass --palette, or describe the colour as \"#rrggbb\""));
                }
                super::check_index(*index, work.colors.len(), *c, &path)?;
                *index
            }
            LegendSource::Color(rgba) => resolve_color(&mut work, *rgba, *c, &path)?,
            LegendSource::Role(role, hint) => match role {
                Role::Transparent => 0,
                Role::Outline => {
                    if work.special.outline == 0 {
                        let Some(hint) = hint else {
                            return Err(PxlError::schema(format!(
                                "ascii.outline_needs_color: legend '{c}' is outline but no colour is given"
                            ))
                            .with_path(&path));
                        };
                        let index = resolve_color(&mut work, *hint, *c, &path)?;
                        work.special.outline = index;
                    }
                    work.special.outline
                }
                Role::Flash => {
                    if work.special.flash == 0 {
                        let Some(hint) = hint else {
                            return Err(PxlError::schema(format!(
                                "ascii.flash_needs_color: legend '{c}' is flash but no colour is given"
                            ))
                            .with_path(&path)
                            .with_hint("write {\"role\": \"flash\", \"color\": \"#ffffff\"}"));
                        };
                        let index = resolve_color(&mut work, *hint, *c, &path)?;
                        work.special.flash = index;
                    }
                    work.special.flash
                }
            },
            LegendSource::Ramp(name, step) => {
                let ramp = work.ramps.get(name).ok_or_else(|| {
                    PxlError::semantic(format!(
                        "ascii.ramp_missing: legend '{c}' uses ramp '{name}', which the palette does not declare"
                    ))
                    .with_path(&path)
                    .with_hint("give a palette with ramps, or use an explicit index")
                })?;
                if *step < 0 || *step as usize >= ramp.len() {
                    return Err(PxlError::semantic(format!(
                        "ascii.ramp_step: legend '{c}' asks for step {step} of ramp '{name}' ({} entries)",
                        ramp.len()
                    ))
                    .with_path(&path));
                }
                let index = ramp[*step as usize];
                if (*step as usize) + 1 < ramp.len() {
                    ramp_neighbor = Some(ramp[*step as usize + 1]);
                }
                index
            }
            LegendSource::Stamp(rows) => {
                if work.derived {
                    return Err(PxlError::semantic(format!(
                        "ascii.stamp_requires_palette: legend '{c}' is a stamp but no palette is given"
                    ))
                    .with_path(&path)
                    .with_hint("stamps carry palette indices; pass --palette"));
                }
                let pixels = stamp_pixels(rows, spec.cell, work.colors.len(), &path)?;
                paints.insert(*c, CellPaint::Stamp(pixels));
                continue;
            }
        };
        let paint = build_paint(&work, base, info, ramp_neighbor, *c, &path)?;
        paints.insert(*c, paint);
    }

    finalize(work, given, paints, auto_chars)
}

fn finalize(
    mut work: Working,
    given: Option<&Palette>,
    paints: BTreeMap<char, CellPaint>,
    auto_chars: Vec<char>,
) -> PxlResult<LegendPlan> {
    if work.derived {
        if work.colors.len() < 2 {
            return Err(PxlError::schema(
                "ascii.empty_palette: the legend maps every character to transparent",
            )
            .with_hint("map at least one character to a colour"));
        }
        if work.special.outline == 0 {
            work.special.outline = 1;
        }
        if work.special.flash == 0 {
            work.special.flash = (work.colors.len() - 1) as u8;
        }
    }
    if work.colors.len() > 36 {
        return Err(PxlError::semantic(format!(
            "ascii.palette_overflow: the layout needs {} colours, the text matrix holds 36",
            work.colors.len()
        ))
        .with_hint("merge colours, or use fewer legend entries"));
    }
    let palette = Palette {
        id: given
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "ascii".into()),
        colors: work.colors,
        ramps: work.ramps,
        cycles: work.cycles,
        special: work.special,
    };
    palette.validate()?;
    palette.check_text_encodable()?;
    let palette_file = if work.derived || work.appended {
        Some(palette_file_of(&palette))
    } else {
        None
    };
    Ok(LegendPlan {
        paints,
        palette,
        palette_file,
        auto_chars,
    })
}

fn palette_file_of(palette: &Palette) -> PaletteFile {
    PaletteFile {
        v: 1,
        id: String::new(),
        colors: palette.colors.iter().map(|c| c.to_hex()).collect(),
        ramps: palette.ramps.clone(),
        cycles: palette
            .cycles
            .iter()
            .map(|(name, cycle)| {
                (
                    name.clone(),
                    CycleFile {
                        indices: cycle.indices.clone(),
                        period: cycle.period,
                    },
                )
            })
            .collect(),
        special: SpecialFile {
            transparent: palette.special.transparent,
            outline: palette.special.outline,
            flash: palette.special.flash,
        },
    }
}

fn entry_info(value: &LegendValue, path: &str) -> PxlResult<EntryInfo> {
    let (source, pattern, alt) = match value {
        LegendValue::Index(index) => (LegendSource::Index(*index), None, None),
        LegendValue::Text(text) => {
            if text.starts_with('#') {
                (LegendSource::Color(pxl_core::parse_hex(text)?), None, None)
            } else {
                (LegendSource::Role(Role::parse(text)?, None), None, None)
            }
        }
        LegendValue::Entry(entry) => (
            entry.source().map_err(|e| tag_path(e, path))?,
            entry.pattern.clone(),
            entry.alt,
        ),
    };
    if let LegendSource::Stamp(_) = source {
        if pattern.is_some() {
            return Err(PxlError::schema("a stamp cannot also set pattern").with_path(path));
        }
    }
    Ok(EntryInfo {
        source,
        pattern,
        alt,
    })
}

fn tag_path(error: PxlError, path: &str) -> PxlError {
    if error.path.is_none() {
        error.with_path(path)
    } else {
        error
    }
}

fn outline_prepass(merged: &BTreeMap<String, LegendValue>) -> PxlResult<(bool, Option<Rgba>)> {
    let mut has = false;
    let mut hint: Option<Rgba> = None;
    for (key, value) in merged {
        let LegendValue::Entry(entry) = value else {
            continue;
        };
        if entry.role.as_deref() != Some("outline") {
            continue;
        }
        has = true;
        if let Some(text) = &entry.color {
            let color = pxl_core::parse_hex(text)?;
            if hint.is_some_and(|prev| prev != color) {
                return Err(PxlError::semantic(format!(
                    "ascii.outline_conflict: legend '{key}' gives a second outline colour"
                ))
                .with_path(format!("/legend/{key}"))
                .with_hint("all outline roles must agree on one colour"));
            }
            hint = Some(color);
        }
    }
    Ok((has, hint))
}

fn resolve_color(work: &mut Working, color: Rgba, c: char, path: &str) -> PxlResult<u8> {
    if let Some(index) = work.colors.iter().position(|existing| *existing == color) {
        return Ok(index as u8);
    }
    if work.colors.len() >= 36 {
        return Err(PxlError::semantic(format!(
            "ascii.palette_overflow: legend '{c}' adds colour {} past the 36-colour limit",
            color.to_hex()
        ))
        .with_path(path)
        .with_hint("reuse an existing colour or trim the legend"));
    }
    if work.derived || !work.strict {
        work.colors.push(color);
        work.appended = true;
        return Ok((work.colors.len() - 1) as u8);
    }
    let (index, nearest) = nearest_color(&work.colors, color);
    Err(PxlError::semantic(format!(
        "ascii.color_not_in_palette: legend '{c}' uses {}, which the palette does not contain",
        color.to_hex()
    ))
    .with_path(path)
    .with_hint(format!(
        "nearest index {index} is {}; set {{\"index\": {index}}} or use palette_mode extend",
        nearest.to_hex()
    )))
}

fn nearest_color(colors: &[Rgba], target: Rgba) -> (usize, Rgba) {
    let mut best = (0usize, *colors.first().unwrap_or(&Rgba::TRANSPARENT));
    let mut best_distance = u32::MAX;
    for (index, color) in colors.iter().enumerate().skip(1) {
        let dr = color.r as i32 - target.r as i32;
        let dg = color.g as i32 - target.g as i32;
        let db = color.b as i32 - target.b as i32;
        let distance = (dr * dr + dg * dg + db * db) as u32;
        if distance < best_distance {
            best_distance = distance;
            best = (index, *color);
        }
    }
    best
}

fn build_paint(
    work: &Working,
    base: u8,
    info: &EntryInfo,
    ramp_neighbor: Option<u8>,
    c: char,
    path: &str,
) -> PxlResult<CellPaint> {
    let Some(pattern) = &info.pattern else {
        return Ok(CellPaint::Solid(base));
    };
    let Some((kind, period)) = parse_pattern(pattern).map_err(|e| e.with_path(path))? else {
        return Ok(CellPaint::Solid(base));
    };
    if base == 0 {
        return Err(PxlError::semantic(format!(
            "ascii.pattern_on_transparent: legend '{c}' patterns a transparent cell"
        ))
        .with_path(path));
    }
    let alt = match kind {
        PatternKind::Bevel => info.alt.or(ramp_neighbor).unwrap_or(base),
        _ => match info.alt {
            Some(alt) => alt,
            None => {
                return Err(PxlError::semantic(format!(
                    "ascii.pattern_alt_missing: legend '{c}' uses pattern '{pattern}' without alt"
                ))
                .with_path(path)
                .with_hint("patterns need a second colour, e.g. {\"index\": 7, \"pattern\": \"checker\", \"alt\": 8}"));
            }
        },
    };
    if alt as usize >= work.colors.len() {
        return Err(PxlError::semantic(format!(
            "ascii.palette_overflow: legend '{c}' alt index {alt} is outside the palette ({} colours)",
            work.colors.len()
        ))
        .with_path(path));
    }
    Ok(CellPaint::Pattern {
        base,
        alt,
        kind,
        period,
    })
}

fn parse_pattern(text: &str) -> PxlResult<Option<(PatternKind, i32)>> {
    if text == "solid" {
        return Ok(None);
    }
    if text == "bevel" {
        return Ok(Some((PatternKind::Bevel, 0)));
    }
    if text == "checker" {
        return Ok(Some((PatternKind::Checker, 2)));
    }
    let (prefix, rest) = match text.split_once(':') {
        Some((prefix, rest)) => (prefix, Some(rest)),
        None => (text, None),
    };
    let kind = match prefix {
        "stripe_x" => PatternKind::StripeX,
        "stripe_y" => PatternKind::StripeY,
        other => {
            return Err(PxlError::schema(format!("ascii.bad_pattern: '{other}'"))
                .with_hint("patterns are solid, checker, stripe_x[:N], stripe_y[:N], or bevel"))
        }
    };
    let period = match rest {
        None => 2,
        Some(text) => text.parse::<i32>().map_err(|_| {
            PxlError::schema(format!("ascii.bad_pattern: bad stripe period '{text}'"))
        })?,
    };
    if period < 2 {
        return Err(PxlError::schema(
            "ascii.bad_pattern: a stripe period must be at least 2",
        ));
    }
    Ok(Some((kind, period)))
}
