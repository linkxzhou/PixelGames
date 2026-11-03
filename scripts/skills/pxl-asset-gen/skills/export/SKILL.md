---
name: pxl-asset-gen-export
description: Export game-ready PNG/GIF/spritesheet/aseprite via step-300 render, sheet, or ase convert/sheet. Use when craft passed review.
---

# Export

Ask what consumes the asset if unknown (web sheet, Godot, preview GIF, raw `.aseprite`).

## Common paths

```bash
# Preview / delivery screenshot source
$PXL render doc.pxl.json -o preview.png --scale 8
$PXL render doc.pxl.json -o anim.gif --anim walk --scale 4

# Transparent asset PNG
$PXL render doc.pxl.json -o asset.png --asset --scale 1

# Indexed sheet from pxl docs
$PXL sheet a.pxl.json b.pxl.json --layout rows-by-anim -o sheet.png

# Ase flatten export
$PXL ase convert file.aseprite -o out.png --scale 1
$PXL ase convert file.aseprite -o out.gif --tag walk
$PXL ase sheet file.aseprite -o sheet.png --data sheet.json --layout packed --padding 1
```

## Before export

Run `review`. Do not ship failed validate.

## Delivery

Show the user **still screenshots** (scaled PNG) or the exported files — not recordings.
