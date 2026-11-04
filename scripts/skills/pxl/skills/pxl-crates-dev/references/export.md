# pxl-export (libs/pxl-export)

Encoders with pinned settings: indexed PNG uses PLTE + tRNS, fixed compression and filter, no timestamps. No `image` crate.

## API

- PNG: `encode_indexed_png(frame, palette, scale)`, `encode_rgba_png`, `decode_png_rgba`, `decode_indexed`.
- GIF: `encode_gif(frames, palette, scale, delay_cs)` (looping), `encode_gif_timed(frames, palette, scale, delays_cs, looped)`, `gif_delays_cs(fps, n)` (spreads rounding so total time matches `n/fps`).
- Preview vs asset: `render_preview_png(frame, palette, scale, grid)` draws a dark backdrop and optional grid (viewing aid); `render_asset_png(frame, palette, scale)` keeps transparency, no grid.
- Raster: `scale_nearest`, `scale_rgba`, `composite(_checked)`, `scaled_dims` (checked variants guard output size).
- Sheets: `pack_sheet(rows: &[Vec<SheetCell>], layout, pad)` with layouts `rows-by-anim` (keeps caller rows) or `grid`; hard cap 2048x2048. `sheet_json(&packed, image_name, pxl_meta)` emits Aseprite `frames` + `meta.frameTags`.

## Rules when editing

- Never add timestamps, wall-clock durations, or nondeterministic ordering. Changing encoder settings changes golden hashes; regenerate only on purpose (`UPDATE_GOLDEN=1 cargo test -p pxl-cli --test golden`).
- All sheet inputs must share one palette (enforced by the CLI before packing).

## Example: pack two sprites into a sheet and make a GIF

```rust
use pxl_export::{encode_gif, encode_indexed_png, pack_sheet, sheet_json, SheetCell};
use pxl_gen::{builtin_palette, generate_bollinger, load_builtin_mask};

fn main() -> pxl_core::PxlResult<()> {
    let palette = builtin_palette("p16")?;
    let slime = generate_bollinger(&load_builtin_mask("slime_8x8")?, 7, &palette, Some("slime"))?;
    let gem = generate_bollinger(&load_builtin_mask("gem_6x6")?, 2, &palette, Some("heat"))?;
    let cell = |name: &str, frame| SheetCell {
        name: name.into(),
        frame,
        duration_ms: 100,
        tag: Some(name.into()),
    };
    let packed = pack_sheet(
        &[vec![cell("slime", slime.clone())], vec![cell("gem", gem)]],
        "rows-by-anim",
        1,
    )?;
    let png = encode_indexed_png(&packed.image, &palette, 1)?;
    let json = sheet_json(&packed, "sheet.png", serde_json::json!({}))?;
    let gif = encode_gif(&[slime.clone(), slime], &palette, 8, 10)?;
    println!("png {} bytes, json {} bytes, gif {} bytes", png.len(), json.len(), gif.len());
    Ok(())
}
```

Output (verified): `png 269 bytes, json 1138 bytes, gif 813 bytes`; identical on every run. The packed image is 8x15 (8-wide slime row, 1px pad, 6-tall gem row). CLI equivalent: `step-300 sheet a.pxl.json b.pxl.json --layout rows-by-anim --pad 1 -o s.sheet.png`.

Run tests: `cargo test -p pxl-export`.
