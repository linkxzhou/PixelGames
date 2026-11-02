# pxl-core (crates/pxl-core)

Foundation crate: `#![no_std]` + `alloc`, no filesystem, threads, clock, or platform RNG. Every other `pxl-*` crate builds on it.

## Key API (re-exported at crate root)

- `IndexFrame { w, h, pixels }`: row-major palette indices, index 0 is transparent. `new`, `canvas`, `filled`, `get`/`set` (out-of-bounds safe), `bbox`, `count_opaque`, `components`, `stats`, `diff_count`, `hash_bytes`.
- `Palette { id, colors, ramps, cycles, special }` with `Special { transparent, outline, flash }`.
- Text matrix: `encode_matrix(&frame) -> Vec<String>` and `decode_matrix(&rows)`. Chars: `.` = 0, `1-9`, then `a-z` for 10-35 (palette limit 36 for text).
- PRNG: `SplitMix64`, `Pcg32::from_seed(seed)`, `gen_range(n)`, `op_seed(seed, op_index, frame_name)`; `sha256_hex(bytes)`.
- `PxlError` with `ErrorCode`, `.with_hint(..)`; constructors `usage`, `schema`, `semantic`, `qa`, `io`.
- Limits: `MAX_DIM = 128`, `MAX_FRAME_PIXELS = 128*128`, `MAX_ANIM_FRAMES = 16`; `PXL_VERSION`, `GEN_VERSION`.
- `Mirror::parse("x"|"y"|"xy"|"none")`.

## Rules when editing

- Keep it IO-free and `no_std`; use `alloc::` imports, not `std::`.
- Integer math only; any randomness goes through the seeded PRNGs so output stays reproducible.
- New errors carry a `hint` the CLI can print.

## Example: random 4x4 frame and its hash

```rust
use pxl_core::{encode_matrix, sha256_hex, IndexFrame, Pcg32};

fn main() -> pxl_core::PxlResult<()> {
    let mut frame = IndexFrame::new(4, 4)?;
    let mut rng = Pcg32::from_seed(7);
    for y in 0..4 {
        for x in 0..4 {
            frame.set(x, y, 1 + rng.gen_range(3) as u8);
        }
    }
    for row in encode_matrix(&frame)? {
        println!("{row}");
    }
    println!("{}", sha256_hex(&frame.hash_bytes()));
    Ok(())
}
```

Output with seed 7 (verified): rows `3132`, `3122`, `3321`, `2231`, and hash `6b196581e1a9e842c8116f9e4d2a952b750add896e619613dc52ce1c2339da64`. Re-running yields the same bytes.

Run tests: `cargo test -p pxl-core`.
