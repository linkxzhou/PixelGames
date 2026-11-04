---
name: pxl-asset-gen-new
description: Scaffold a new canvas — ASCII/shapes/recipe/pxl.json or prepare an aseprite workflow — with size and palette locked. Use when starting fresh.
---

# New canvas

## Pick a surface

| Surface | When | Start command |
| --- | --- | --- |
| ASCII grid | Hand-authored silhouette / UI | write `*.ascii.txt` → `ascii import` / `ascii render` |
| shapes.json | Geometric props / tanks / panels | `gen shapes --spec …` or recipe `base.gen=shapes` |
| bollinger template | Organic blob / critter | `gen bollinger --template … --palette …` |
| recipe.json | Multi-dir + anim + sheet in one build | `build recipe.json -o outdir` |
| `.aseprite` IO | Already have ase / need tags·layers·sheet | `ase inspect` / `ase convert` / edit cmds |

## Procedure

1. Create an output directory (not under `games/`).
2. Lock `--palette` / palette file from brief.
3. Scaffold:
   - Blank-ish parts canvas: `$PXL gen parts -o blank.pxl.json --palette p16` (omit `--dir` for empty-ish start when supported).
   - Or write ASCII block-in (one glyph = one mass).
   - Or copy a nearest existing `*.recipe.json` (e.g. `sources/<id>/` of a built library; `$PXL schema recipe` for fields) and rename ids (do not copy game runtimes).
4. `$PXL inspect <doc>` once to confirm size/palette.

## Ase path

Prefer `$PXL ase …` for `.aseprite` roundtrip (v8). Create pixels via gen/ascii/shapes/recipe first, export to ase when needed (`ase convert` / import bridge), then tag/layer with `ase tag|layer|frame` (rebuild `step-300` if those subcommands are missing from an old binary).
