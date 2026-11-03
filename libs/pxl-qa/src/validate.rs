use crate::common::{palette_checks, valid_frame};
use crate::{Check, QaConfig, Report, Severity, ValidateInput};
use pxl_core::{IndexFrame, PxlError, PxlResult};
use std::collections::{BTreeMap, BTreeSet};

pub fn validate(input: &ValidateInput, rules: &str) -> PxlResult<Report> {
    let config =
        match rules {
            "default" => QaConfig::default(),
            "strict" => QaConfig {
                strict: true,
                ..QaConfig::default()
            },
            _ => return Err(
                PxlError::usage(format!("unknown ruleset '{rules}'")).with_hint(
                    "use default or strict, or validate_with_config for explicit pixel constraints",
                ),
            ),
        };
    let mut report = validate_with_config(input, &config)?;
    report.rules = rules.to_string();
    Ok(report)
}

/// Integrity errors are always blocking. Style heuristics are warnings unless
/// strict is enabled. Asset kinds and animation names never select rules.
pub fn validate_with_config(input: &ValidateInput, config: &QaConfig) -> PxlResult<Report> {
    config.check()?;
    let entries = input
        .frames
        .len()
        .saturating_add(input.tiles.len())
        .saturating_add(input.facings.len())
        .saturating_add(input.anims.len())
        .saturating_add(
            input
                .anims
                .iter()
                .map(|a| a.frames.len().max(a.frame_names.len()))
                .fold(0usize, usize::saturating_add),
        );
    if entries > config.max_entries
        || input
            .palette
            .ramps
            .len()
            .saturating_add(input.palette.cycles.len())
            > config.max_entries
    {
        return Ok(Report {
            rules: "custom".into(),
            checks: vec![Check::new(
                "input.entry_budget",
                false,
                entries,
                "frame, reference and palette-group count within configured entry budget",
                Severity::Error,
            )],
        });
    }
    let mut checks = palette_checks(&input.palette, config);
    let all = input
        .frames
        .iter()
        .map(|(_, f)| f)
        .chain(input.tiles.iter().map(|(_, f)| f))
        .chain(input.facings.values())
        .chain(input.anims.iter().flat_map(|a| &a.frames));
    let mut total = 0usize;
    for f in all {
        total = total.saturating_add(f.pixels.len());
        if total > config.max_total_pixels {
            checks.push(Check::new(
                "input.pixel_budget",
                false,
                total,
                "total pixel budget exceeded",
                Severity::Error,
            ));
            return Ok(Report {
                rules: "custom".into(),
                checks,
            });
        }
    }
    checks.push(Check::new(
        "input.nonempty",
        !input.frames.is_empty()
            || !input.anims.is_empty()
            || !input.tiles.is_empty()
            || !input.facings.is_empty(),
        input.frames.len(),
        "at least one frame",
        Severity::Error,
    ));
    let mut named: BTreeMap<&str, &IndexFrame> = BTreeMap::new();
    let mut conflicts = Vec::new();
    let mut valid = true;
    for (name, frame) in &input.frames {
        if let Some(previous) = named.insert(name, frame) {
            conflicts.push((name.as_str(), previous));
            checks.push(Check::new(
                format!("frame.{name}.unique"),
                false,
                name,
                "unique frame name",
                Severity::Error,
            ));
        }
    }
    let mut labels = BTreeSet::new();
    for anim in &input.anims {
        let label = anim_label(anim);
        if !labels.insert(label.clone()) {
            checks.push(Check::new(
                format!("anim.{label}.unique"),
                false,
                &label,
                "unique animation display name and direction",
                Severity::Error,
            ));
        }
        checks.push(Check::new(
            format!("anim.{label}.references"),
            anim.frame_names.len() == anim.frames.len(),
            anim.frame_names.len(),
            "one frame identity per animation frame",
            Severity::Error,
        ));
        for (name, frame) in anim.frame_names.iter().zip(&anim.frames) {
            if let Some(previous) = named.insert(name, frame) {
                if previous != frame {
                    conflicts.push((name.as_str(), previous));
                    checks.push(Check::new(
                        format!("frame.{name}.identity"),
                        false,
                        name,
                        "same frame identity must have identical dimensions and pixels",
                        Severity::Error,
                    ));
                }
            }
        }
    }
    let mut tile_names = BTreeSet::new();
    for (name, frame) in &input.tiles {
        if !tile_names.insert(name) {
            checks.push(Check::new(
                format!("tile.{name}.unique"),
                false,
                name,
                "unique tile frame name",
                Severity::Error,
            ));
        }
        if named
            .get(name.as_str())
            .is_some_and(|previous| *previous != frame)
        {
            checks.push(Check::new(
                format!("tile.{name}.identity"),
                false,
                name,
                "tile and named frame pixels must agree",
                Severity::Error,
            ));
        }
    }
    let frames = input
        .frames
        .iter()
        .map(|(n, f)| (format!("frame.{n}"), f))
        .chain(input.tiles.iter().map(|(n, f)| (format!("tile.{n}"), f)))
        .chain(
            input
                .facings
                .iter()
                .map(|(n, f)| (format!("facing.{n}"), f)),
        )
        .chain(input.anims.iter().flat_map(|a| {
            a.frames
                .iter()
                .enumerate()
                .map(move |(i, f)| (format!("anim.{}.frame_{i}", anim_label(a)), f))
        }));
    for (id, f) in frames {
        let ok = valid_frame(f, config);
        valid &= ok;
        checks.push(Check::new(
            format!("{id}.buffer"),
            ok,
            format!("{}x{} / {}", f.w, f.h, f.pixels.len()),
            "positive dimensions, exact buffer length, within pixel budget",
            Severity::Error,
        ));
    }
    if !valid {
        return Ok(Report {
            rules: "custom".into(),
            checks,
        });
    }
    // Global identity tracking prevents a shared empty frame borrowing an
    // exemption from one sequence while being used in another nonterminal slot.
    let mut terminal: BTreeMap<&str, bool> = BTreeMap::new();
    let mut allowed_empty: BTreeMap<&str, bool> = BTreeMap::new();
    for a in &input.anims {
        for (i, n) in a.frame_names.iter().enumerate() {
            let end = a
                .allow_terminal_empty
                .unwrap_or(config.allow_terminal_empty)
                && !a.looped
                && !a.static_mode
                && i > 0
                && i + 1 == a.frames.len();
            terminal.entry(n).and_modify(|v| *v &= end).or_insert(end);
            let allowed = a.allow_empty_frames.unwrap_or(config.allow_empty_frames);
            allowed_empty
                .entry(n)
                .and_modify(|v| *v &= allowed)
                .or_insert(allowed);
        }
    }
    for (name, f) in named {
        let empty_ok = config.allow_empty_frames
            || allowed_empty.get(name).copied().unwrap_or(false)
            || terminal.get(name).copied().unwrap_or(false);
        checks.extend(crate::pixels::inspect(
            f,
            &input.palette,
            name,
            empty_ok,
            config,
        ));
        checks.extend(crate::resolution::inspect(f, name, config));
    }
    for (occurrence, (name, f)) in conflicts.into_iter().enumerate() {
        checks.extend(crate::pixels::inspect(
            f,
            &input.palette,
            &format!("{name}.conflict_{occurrence}"),
            config.allow_empty_frames,
            config,
        ));
    }
    for (name, f) in &input.facings {
        checks.extend(crate::pixels::inspect(
            f,
            &input.palette,
            &format!("facing:{name}"),
            config.allow_empty_frames,
            config,
        ));
    }
    for (name, f) in &input.tiles {
        checks.extend(crate::pixels::inspect(
            f,
            &input.palette,
            &format!("tile:{name}"),
            config.allow_empty_frames,
            config,
        ));
    }
    for a in &input.anims {
        checks.extend(crate::animation::inspect(
            a,
            &anim_label(a),
            &input.palette,
            config,
        ));
        // Even malformed identity lists must not hide unlisted frame pixels.
        if a.frame_names.len() != a.frames.len() {
            for (i, f) in a.frames.iter().enumerate() {
                checks.extend(crate::pixels::inspect(
                    f,
                    &input.palette,
                    &format!("{}:{i}", anim_label(a)),
                    config.allow_empty_frames,
                    config,
                ));
            }
        }
    }
    checks.extend(crate::tile::inspect(input, config));
    checks.extend(crate::composition::inspect(input, config));
    Ok(Report {
        rules: "custom".into(),
        checks,
    })
}

fn anim_label(a: &crate::AnimSeq) -> String {
    match &a.dir {
        Some(d) => format!("{}.{}", a.name, d),
        None => a.name.clone(),
    }
}
