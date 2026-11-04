# pxl-ops (libs/pxl-ops)

Transforms `IndexFrame`s using the `Op` enum from `pxl-format`. Integer-only and deterministic.

## Public API

- `apply_op(frame, &Op, &ApplyCx, index)` and `apply_ops(frame, &[Op], &ApplyCx)`.
- `ApplyCx { palette, seed, frame_name, load_template, parts, pose }`; per-op seeds come from `op_seed(seed, index, frame_name)`.
- `expand_macro(spec, dir, frame, palette, parts) -> Vec<Vec<Op>>`: one op list per output frame. Each frame's ops apply to the source frame independently (they do not accumulate).
- `parse_macro_call`, `macro_catalog`, `macro_info`, `MACRO_NAMES`.
- `compose_rig` / `PartPose` for paper-doll parts.

Ops (`step-300 list ops`): shift, row_shift, shear, squash, flip, rotate90, rotate (rejected until M2), palette_swap, palette_cycle, shade, outline, dissolve, mask_erase, mask_fill, patch_pixels, texture, material, detail, noop, part_transform, compose.

Macros (`step-300 list macros --json` gives args/defaults/frames): tread_cycle, idle_rumble, idle_breathe, bob, walk_side, run_side, hurt_flash, death_dissolve, blink, spin_projectile, pulse_glow, doll.

## Rules when editing

- Add a new op in three places: the `Op` enum and `OP_NAMES` in `pxl-format/src/ops.rs`, the match in `apply/mod.rs`, and tests. Macros go in `macros/` plus `catalog.rs` (argument metadata and example).
- Keep results independent of iteration order and platform; no floats in pixel decisions.
- Flip, rotate90, shift follow Piskel semantics; outline brushes follow Pixelorama.

## Example: outline a sprite and expand `bob`

```rust
use std::{cell::RefCell, collections::BTreeMap};

use pxl_gen::{builtin_palette, generate_bollinger, load_builtin_mask};
use pxl_ops::{apply_ops, expand_macro, ApplyCx};

fn main() -> pxl_core::PxlResult<()> {
    let palette = builtin_palette("p16")?;
    let base = generate_bollinger(&load_builtin_mask("slime_8x8")?, 7, &palette, Some("slime"))?;
    let cx = ApplyCx {
        palette: &palette,
        seed: 1,
        frame_name: "base",
        load_template: None,
        parts: None,
        pose: RefCell::new(BTreeMap::new()),
    };
    let per_frame = expand_macro("bob(amp=1)", None, &base, &palette, None)?;
    for (i, ops) in per_frame.iter().enumerate() {
        let frame = apply_ops(&base, ops, &cx)?;
        println!("hop_{i}: {} ops, opaque={}", ops.len(), frame.count_opaque());
    }
    let outline = pxl_format::parse_ops_value(r#"{"op":"outline","mode":"outer"}"#)?;
    println!("outlined opaque={}", apply_ops(&base, &outline, &cx)?.count_opaque());
    Ok(())
}
```

Output (verified): four frames `hop_0..hop_3` with opaque counts 52, 46, 52, 48, then `outlined opaque=64`. CLI equivalent: `step-300 anim in.pxl.json --name hop --macro 'bob(amp=1)' -o out.pxl.json`.

Run tests: `cargo test -p pxl-ops`.
