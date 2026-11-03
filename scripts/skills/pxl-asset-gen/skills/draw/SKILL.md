---
name: pxl-asset-gen-draw
description: Block in silhouette then refine pixels via ASCII/shapes/gen/ops — cheapest mistakes first. Use for the main body of drawing work.
---

# Draw

Order beats fancy technique. Detail before form is expensive wrongness.

## Procedure

### 1. Look

`$PXL inspect` / `ase inspect` any existing file. Read layer/frame/palette truth.

### 2. Block-in (silhouette)

- ASCII: large masses only (2–4 glyphs). `$PXL ascii render sketch.ascii.txt -o sketch.png --scale 8 --palette p16`
- Or shapes rects/pixels for hard props.
- Or bollinger template for organic base, then ops.

Check silhouette readability at small size (see silhouette reference).

### 3. Refine

```bash
$PXL ascii refine sketch.ascii.txt -o refined.ascii.txt   # 2× editable cells
# edit refined grid → render again
$PXL shapes refine prop.shapes.json -o prop_x2.shapes.json
$PXL op <doc> --ops '...' -o next.pxl.json   # trust op --help / list ops
```

Patch sparingly (`patch_pixels` in recipes) after the mass is right.

### 4. Outline last

Outer selective outline in a dark index; break outline where forms meet soft materials. Prefer recipe `post: [{op:outline,…}]` or `ase filter … outline` over painting every edge twice.

### 5. Look again

`$PXL render doc.pxl.json -o look.png --scale 8` → screenshot to user if delivering visuals. Then `shade` or `review`.

## Batch habit

Generate ≤3 variants (`--seed` / `--variants`) when exploring; pick one; discard the rest. Do not fork endless near-duplicates.
