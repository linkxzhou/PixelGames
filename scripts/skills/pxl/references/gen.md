# pxl-gen (crates/pxl-gen)

Generators only: input spec + palette + seed in, `IndexFrame` out. No IO.

## Public API

- `generate_bollinger(&RuntimeMask, seed, &Palette, ramp: Option<&str>) -> IndexFrame`: body sample, mirror, edges, indexed colour (follows the MIT zfedoran pixel-sprite-generator pipeline).
- `generate_shapes(&ShapesFile, &Palette)`: draw list (`rect`, `pixels`, `outline`, patterns, `mirror`).
- `generate_noise_tile(kind, size, seed, &Palette, ramp)`: kinds `dirt`, `grass`, `sand`, `ore`; edges wrap so tiles are seamless. `variant_seed(base, i)` derives per-variant seeds.
- `compile_ascii` / `AsciiCompiled` / `AsciiWarning`: ASCII grid to frames.
- Built-ins: `builtin_palette(id)`, `builtin_template(id)`, `load_builtin_mask(id)`, `palette_ids()`, `template_ids()`.
  - Palettes: `p16`, `tank`.
  - Templates: `slime_8x8`, `ship_8x12`, `robot_6x10`, `bug_8x8`, `gem_6x6`, `mushroom_8x8`, `tree_8x12`, `rock_6x6`, `dragon_side_8x10`, `dragon_top_8x8`, `panel_4x8`. Assets live in `assets/templates` and `assets/palettes`.

## Rules when editing

- Integer arithmetic only; randomness from `pxl_core::Pcg32` seeded by the caller.
- Changing generator output changes golden hashes (`crates/pxl-cli/tests/golden/hashes.json`); bump `GEN_VERSION` thinking and regenerate only on purpose.
- Algorithms are reimplemented from descriptions; do not copy the unlicensed MaartenGr port.

## Example: slime sprite and dirt tile

```rust
use pxl_gen::{builtin_palette, generate_bollinger, generate_noise_tile, load_builtin_mask};

fn main() -> pxl_core::PxlResult<()> {
    let palette = builtin_palette("p16")?;
    let mask = load_builtin_mask("slime_8x8")?;
    let slime = generate_bollinger(&mask, 7, &palette, Some("slime"))?;
    println!("{}x{} opaque={}", slime.w, slime.h, slime.count_opaque());

    let tank = builtin_palette("tank")?;
    let tile = generate_noise_tile("dirt", 16, 3, &tank, Some("ground"))?;
    println!("tile {}x{}", tile.w, tile.h);
    Ok(())
}
```

Output (verified): `8x8 opaque=52` and `tile 16x16`. CLI equivalent: `step-300 --seed 7 gen bollinger --template slime_8x8 --palette p16 --ramp slime -o slime.pxl.json`.

Run tests: `cargo test -p pxl-gen`.
