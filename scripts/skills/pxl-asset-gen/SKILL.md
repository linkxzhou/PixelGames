---
name: pxl-asset-gen
description: Craft pixel-art materials with PixelGames step-300 (ASCII/shapes/recipe/ase) — brief → concept → palette → draw → shade → review → animate → export → fix/tileset. Use when generating or iterating sprites, tiles, sheets, GIFs, or .aseprite assets for this repo; not for the full brief→package→engine pipeline (use sibling skill `pxl` for that).
---

# pxl-asset-gen

Standalone **asset craft** skill for PixelGames. Sibling of [`../pxl/SKILL.md`](../pxl/SKILL.md): that skill owns the five-stage pipeline (step-100…500). **This skill owns drawing craft only** — plan, generate, look, fix, export pixels via `step-300`.

## Hard rules

1. **All pixels from `step-300`.** Never invent or edit pixels with Python/PIL/ImageMagick paint tools.
2. **Do not modify `games/`.** Work under `/tmp`, `examples/`, or a user-named output dir.
3. **Screenshots for visual delivery only** — no screen recordings.
4. **Trust `--help` over memory.** Rebuild/use a current `step-300` after ase CLI changes.
5. Keep skill files ≤500 lines (repo `AGENTS.md`).

## How to invoke `step-300`

```bash
# Prefer a freshly built binary on this machine:
PXL=./target/debug/step-300
# Or:
# cargo +1.95.0 run -p pxl-cli --bin step-300 --
# When the sibling `pxl` skill launcher is available, use that for step-300 instead.
$PXL --help
$PXL ase --help
```

Global flags worth knowing: `--json`, `--seed`, `--overwrite`, `--strict`.

## Studio loop (always)

1. **Decide intent** → pick subskills below (do not improvise order).
2. **Inspect** existing inputs (`inspect`, `ase inspect`, `validate`).
3. **Generate a small batch** (ASCII block-in, shapes, bollinger, recipe, or ase edit).
4. **Look** — `$PXL render … -o preview.png --scale 8` and/or `inspect` text matrix; open PNG as screenshot when delivering to the user.
5. **Fix** → re-validate → export only when review passes.

Default happy path for a new sprite:

`brief` → `concept` (if from-scratch character/prop) → `new`/`palette` → `draw` → `shade` → `review` → (`animate`) → `export`

Edits to existing art: `fix` (and only the craft steps that apply).

## Route table

| Intent | Read |
| --- | --- |
| Vague request / lock size·palette·view·light | [skills/brief/SKILL.md](skills/brief/SKILL.md) |
| Design before pixels / reference masses | [skills/concept/SKILL.md](skills/concept/SKILL.md) |
| Blank canvas / recipe scaffold / ase doc | [skills/new/SKILL.md](skills/new/SKILL.md) |
| Choose or extend palette / ramps | [skills/palette/SKILL.md](skills/palette/SKILL.md) |
| Block-in → refine pixels | [skills/draw/SKILL.md](skills/draw/SKILL.md) |
| Volume with hue-shifted light | [skills/shade/SKILL.md](skills/shade/SKILL.md) |
| Mechanical + by-eye QA before "done" | [skills/review/SKILL.md](skills/review/SKILL.md) |
| Tags, frame timing, cycles | [skills/animate/SKILL.md](skills/animate/SKILL.md) |
| PNG / GIF / sheet / ase export | [skills/export/SKILL.md](skills/export/SKILL.md) |
| Iterate without wrecking good parts | [skills/fix/SKILL.md](skills/fix/SKILL.md) |
| Seamless / noise tiles / tileset atlas | [skills/tileset/SKILL.md](skills/tileset/SKILL.md) |

## References (craft)

- [references/core-principles.md](references/core-principles.md)
- [references/palette-and-color.md](references/palette-and-color.md)
- [references/shading-and-light.md](references/shading-and-light.md)
- [references/silhouette-and-form.md](references/silhouette-and-form.md)
- [references/animation.md](references/animation.md)
- [references/review-checklist.md](references/review-checklist.md)
- [references/cli-cheatsheet.md](references/cli-cheatsheet.md) — real flags from this repo

## Out of scope

- Full design→package→engine pipeline → use `pxl`.
- Aseprite GUI / WebSocket / third-party MCP Lua bridges.
- Tile-management plugins beyond what `step-300 ase export-tileset` / `tile gen` already do.
