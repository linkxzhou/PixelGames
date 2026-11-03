---
name: pxl-asset-gen-fix
description: Iterate on existing pixel assets without destroying what already works. Use for touch-ups, style corrections, and "make it more X".
---

# Fix / iterate

## Procedure

1. **Understand.** `inspect` / `ase inspect` / render preview. State what must stay.
2. **Minimal change.** Prefer `op`, palette swap, single ASCII region edit, or recipe `patch_pixels` over full regenerate.
3. **One knob per pass** (colour, then outline, then timing) so regressions are attributable.
4. **Diff visually** — old vs new PNG screenshots.
5. **`validate` + cold read** before claiming fixed.

```bash
$PXL palette swap in.pxl.json --map map.json -o out.pxl.json
$PXL op in.pxl.json ... -o out.pxl.json
$PXL ase filter in.aseprite replace-color -o out.aseprite   # see filter --help
```

If a regenerate is required, keep the same `--seed` when comparing fair variants.
