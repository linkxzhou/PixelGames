---
name: pxl-asset-gen-palette
description: Choose, show, ramp, or apply a palette for PixelGames assets. Use when colour count, ramps, or off-palette fixes matter.
---

# Palette

## Choose (priority)

1. User-named palette or project file.
2. Match an existing asset in the same set (`inspect` / open palette.json).
3. Era shorthand → keep small: ~12–16 colours as 3–4 hues × value steps.
4. Default built-in: `p16` (tank set for battle-city-like art).

```bash
$PXL list palettes
$PXL palette show p16
$PXL palette ramp p16 --levels 4 --hue-shift 40 -o ramped.palette.json
$PXL palette apply in.pxl.json ramped.palette.json -o out.pxl.json
```

## Ramps

Prefer warm highlights / cool shadows (see [../../references/shading-and-light.md](../../references/shading-and-light.md)). `palette ramp` builds shadow/light steps with hue shift — use it instead of hand-darkening hex toward mud.

## Ase document palette

```bash
$PXL ase palette sort file.aseprite --by hue -o out.aseprite
$PXL ase palette set file.aseprite 3 '#c08040' -o out.aseprite
$PXL ase palette add file.aseprite '#4060a0' -o out.aseprite
```

(Frame palette: pass `--frame N` where supported.)

## Cap

Text documents in this stack keep palettes small (ramp `--max` default 36). Prefer fewer stronger ramps over many near-duplicates.
