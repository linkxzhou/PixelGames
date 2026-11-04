---
name: pxl-asset-gen-animate
description: Build idle/walk/attack cycles with timing and tags using recipe anims or ase frame/tag tools. Use when a sprite must move.
---

# Animate

## Library preview (separate authored frame assets)

For a pack/gallery, first read [../../references/library.md](../../references/library.md#animation-showcase-contract).
Register `sequence`, zero-based contiguous `frame`, positive `duration_ms` and optional `direction` per asset.
The library kit embeds a real frame player in `index.html`: play/pause, step, speed and integer pixel zoom.
Reuse existing frame PNGs; do not replace real sprite motion with CSS movement or infer cycles from IDs.
Verify original timing, wraparound, source cropping and reduced-motion pause. Static recipe/atlas `idle`
durations are not playback timing. Gallery playback does not itself produce an animated GIF/sheet.

## Prefer recipe macros when possible

`build` expands `anims` + macros (`$PXL --json list macros`, `$PXL schema recipe`; a generated library's `sources/*/*.recipe.json` is a working example). Keep keys few; in-betweens cheap.

```bash
$PXL build my.recipe.json -o out/ --overwrite
$PXL render out/my.pxl.json --anim walk --dir E -o walk.gif --scale 4
```

## Ase timeline / tags

```bash
$PXL ase frame add file.aseprite --at 1 --duration 100 -o f.aseprite
$PXL ase frame set-duration f.aseprite 0 120 -o f.aseprite
$PXL ase tag add f.aseprite walk --from 0 --to 3 --direction forward -o f.aseprite
$PXL ase list-tags f.aseprite
```

`$PXL ase frame --help` / `$PXL ase tag --help` list the exact edit flags.

## Timing notes

- Avoid identical frame durations for every cell of a walk (metronome feel).
- Contact poses longer than passing poses.
- Loop test with GIF/`--anim` before export sheet.

See [../../references/animation.md](../../references/animation.md).
