# pxl-qa (crates/pxl-qa)

Checks pixels only; asset kind, names, and directions never select rules.

## API

- `validate(&ValidateInput, "default"|"strict") -> Report`. Unknown ruleset is a usage error (`E_USAGE`).
- `validate_with_config(&ValidateInput, &QaConfig) -> Report`; report rules are labelled `custom`.
- `Report { rules, checks }`, `report.ok()` is true when no `Severity::Error` check failed. `Check { id, pass, value, limit, severity, samples }`.
- `ValidateInput { palette, frames, anims: Vec<AnimSeq>, facings, tiles, tile_ramp, tile_ramps, asset_kinds, frame_assets }`.

## Behaviour

- Integrity failures (palette alpha, budgets, outline gaps with default limit 0, holes) block. Style heuristics warn unless `strict` is on (CLI `--strict` or `validate --rules strict`).
- `QaConfig` percentages are basis points (10000 = 100%); `None` disables a limit. Defaults include `max_colors: 32`, `max_components: 8`, `max_outline_gaps: 0`, `max_animation_diff_bps: 3500`, `allow_terminal_empty: true`, seam limits for tiles. Unknown JSON keys are rejected (`deny_unknown_fields`).
- Invalid config (ratio > 10000, zero budget) returns a usage error.

CLI use: `step-300 validate in.pxl.json --json`, or `step-300 validate in.pxl.json --qa-config qa.json --json` with e.g. `{"max_colors":8,"allow_duplicate_frames":true}`. `--rules` and `--qa-config` are mutually exclusive.

## Example: default pass, then a strict custom config

```rust
use std::collections::BTreeMap;

use pxl_gen::{builtin_palette, generate_bollinger, load_builtin_mask};
use pxl_qa::{validate, validate_with_config, QaConfig, ValidateInput};

fn main() -> pxl_core::PxlResult<()> {
    let palette = builtin_palette("p16")?;
    let frame = generate_bollinger(&load_builtin_mask("slime_8x8")?, 7, &palette, Some("slime"))?;
    let input = ValidateInput {
        palette,
        frames: vec![("base".into(), frame)],
        anims: vec![],
        facings: BTreeMap::new(),
        tiles: vec![],
        tile_ramp: String::new(),
        tile_ramps: BTreeMap::new(),
        asset_kinds: BTreeMap::new(),
        frame_assets: BTreeMap::new(),
    };
    println!("default ok={}", validate(&input, "default")?.ok());

    let config = QaConfig { max_colors: Some(4), strict: true, ..QaConfig::default() };
    let report = validate_with_config(&input, &config)?;
    for c in report.checks.iter().filter(|c| !c.pass) {
        println!("{} {:?} value={} limit={}", c.id, c.severity, c.value, c.limit);
    }
    Ok(())
}
```

Output (verified): `default ok=true`, then a strict failure `frame.base.outline_gaps Error value=6 ...`. Fix by outlining the sprite (`step-300 op ... --op '{"op":"outline","mode":"outer"}'`) and re-validating.

Run tests: `cargo test -p pxl-qa`.
