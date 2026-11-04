---
name: pxl-asset-gen-tileset
description: Build seamless noise tiles or export ase tileset atlases with `$PXL tile` / `$PXL ase` tools. Use for terrain, repeating props, autotile inputs.
---

# Tileset

## Grid first

Tile size belongs to the game (16×16 / 32×32 common). Lock it in the brief.

## Seamless noise tiles

```bash
$PXL tile gen --kind dirt --size 16 --palette tank --ramp ground --variants 4 -o dirt.pxl.json
$PXL gen noise-tile --kind grass --size 16 --palette tank --ramp ground -o grass.pxl.json
$PXL render dirt.pxl.json -o dirt.png --asset
```

Kinds include `dirt`, `grass`, `sand`, `ore` (trust `list` / `--help`). Edges wrap for seamless repeat — verify by rendering a 2×2 montage via sheet or repeated blit in preview.

## Ase tileset atlas

```bash
$PXL ase export-tileset file.aseprite -o tiles.png --index 0
```

No tile-management plugins. Do not invent external tileset editors.

## Autotile

`$PXL tile autotile --help` — use only when the help surface matches the user's grid bit-mask needs; otherwise hand-author corner variants as separate frames/files.
