---
name: pxl-asset-gen-shade
description: Add hue-shifted light and shadow so sprites gain form without flat-luminance mud. Use after flat masses read correctly.
---

# Shade

Flat “same hue, darker” reads as slider art. Shift hue with value.

## Rules

- Fix **one** light direction (default upper-left) for the whole sprite and its animation.
- Shadows → cooler / slightly less saturated; lights → warmer / slightly more saturated.
- Prefer palette ramp indices over inventing new hex mid-shade.

```bash
$PXL palette ramp base.palette.json --levels 4 --hue-shift 40 -o shade.palette.json
$PXL palette apply flat.pxl.json shade.palette.json -o shaded.pxl.json
```

## Procedure

1. Confirm flat block-in reads (render + silhouette).
2. Apply ramps / remap masses to shade steps (ASCII legend or shapes colours).
3. Soft materials: wider value bands; hard materials: tighter bands + specular speck (1–2 px) only where earned.
4. Re-render; if volume fights silhouette, undo shade and fix form first.

## Ase filters (optional)

`$PXL ase filter file.aseprite brightness-contrast -o out.aseprite` (and hue-saturation) — use lightly; prefer authored ramp indices for indexed work.
