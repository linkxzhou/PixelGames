---
name: pxl-asset-gen-animate
description: Build idle/walk/attack cycles with timing and tags using recipe anims or ase frame/tag tools. Use when a sprite must move.
---

# Animate

## Prefer recipe macros when possible

`build` expands `anims` + macros (see examples under `examples/`). Keep keys few; in-betweens cheap.

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

Rebuild `step-300` if `ase frame|tag` are missing from an older binary (source on main includes them).

## Timing notes

- Avoid identical frame durations for every cell of a walk (metronome feel).
- Contact poses longer than passing poses.
- Loop test with GIF/`--anim` before export sheet.

See [../../references/animation.md](../../references/animation.md).
