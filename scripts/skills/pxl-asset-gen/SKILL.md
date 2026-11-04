---
name: pxl-asset-gen
description: Craft pixel-art materials with the bundled pxl-asset-gen binary (ASCII/shapes/recipe/ase) and deliver a browsable asset library (index.html + manifest + atlases + previews + interactive animation showcase, like examples/stardew). Covers brief → palette → draw → shade → review → animate → export → library. Use when generating or iterating sprites, tiles, sheets, GIFs, .aseprite assets, asset packs/galleries or frame-animation previews; not for the full brief→package→engine game pipeline (use sibling skill `pxl` for that).
version: 0.2.0
author: <NAME>
license: CC0
---

# pxl-asset-gen

Self-contained **pixel asset craft** skill: one prebuilt binary + craft guides + a library kit. Plan, generate, look, fix and export pixels with the binary, then publish one folder with `index.html`. No sources, compilers or build steps are needed.

## Hard rules

1. **All pixels from the binary.** Python may only place palette glyphs into ASCII grids/recipes; never paint or edit pixels with PIL/ImageMagick (PIL is allowed read-only in `verify_library.py`).
2. **Normal generation must not write into the skill folder** or existing reference packs (`games/`, `examples/stardew/`). Output goes to `/tmp/<pack>`, `examples/<new>/`, or a user-named dir. Explicit maintenance requests may modify the named skill/reference pack; preserve unrelated assets and change the generator as well as its generated page.
3. **Screenshots for visual delivery only** — no screen recordings.
4. **Trust `--help` over memory.** Check `$PXL <cmd> --help` before using a flag.
5. Keep every generated tools file ≤500 lines.

## Binary

One executable per platform in `bin/`; pick the one matching the host:

| Host | Binary |
| --- | --- |
| macOS Apple Silicon | `bin/pxl-asset-gen-darwin-arm64` |
| Linux x86_64 | `bin/pxl-asset-gen-linux` |

```bash
SKILL=<absolute path of this skill folder>
PXL=$SKILL/bin/pxl-asset-gen-darwin-arm64          # or $SKILL/bin/pxl-asset-gen-linux
$PXL --help; $PXL ascii render --help; $PXL ase --help
```

- If the host's binary is missing from `bin/`, stop and tell the user — do not try to build or fetch one.
- `bin/pxl-asset-gen-linux` must be a **static** build (`x86_64-unknown-linux-musl`); a glibc-linked one fails on older hosts. Maintainers verify with `scripts/build-pxl-asset-gen.sh --check` (never compiles).
- If it is not executable (permission denied), run `chmod +x "$PXL"`; on macOS quarantine, `xattr -d com.apple.quarantine "$PXL"`.
- Includes `ase` (headless Aseprite IO, frame/tag/layer edit, Rhai `ase script`).
- Global flags: `--json`, `--seed`, `--overwrite`, `--strict`. Exit 5 = QA fail.
- Library-kit scripts find the binary automatically (or take `--pxl $PXL` / `PXL_BIN`).

## Pick a workflow

| Request | Workflow |
| --- | --- |
| A pack / several assets / gallery / "like stardew" | **A. Asset library** (default; ends with `index.html`) |
| One sprite or tile | **B. Single sprite** |
| Change existing art (PNG / `.pxl.json` / `.aseprite`) | **C. Edit existing** |

Never skip a step's **Done when**; if it fails, loop back to the step named in the fix hint.

## Workflow A — Asset library (step by step)

Read [references/library.md](references/library.md) once before Step 4 (layout, `project.py` contract, `lib` API).

```bash
SKILL=<skill folder>; PXL=$SKILL/bin/pxl-asset-gen-darwin-arm64; KIT=$SKILL/assets/library-kit
OUT=/tmp/<pack>                 # or examples/<pack> / user-named dir — never inside $SKILL
```

**Step 0 — Check the binary.** `$PXL --version`.
Done when: it prints `pxl …`. Missing / not runnable → see "Binary" above; stop if the platform has no binary.

**Step 1 — Brief** ([skills/brief](skills/brief/SKILL.md)). Lock: tile/sprite size, view, light direction, outline style, palette size, destination engine, theme/mood. Ask only if the user gave nothing to infer from; otherwise state defaults.
Done when: a 5–8 line brief is written (it becomes `CONFIG['brief']` / `subtitle`).

**Step 2 — Asset list** ([skills/concept](skills/concept/SKILL.md)). Write a table: `id` (snake_case), name, category, canvas (32/64/128/256), `source_rect`, tile?, variants/frames. Group by category (terrain, items, props, characters, buildings…).
Done when: every requested asset has a row; counts per category are agreed/stated.

**Step 3 — Palette** ([skills/palette](skills/palette/SKILL.md), [references/palette-and-color.md](references/palette-and-color.md)). Index 0 `#00000000`; one dark outline; 3–4 step hue-shifted ramps per material; one unique glyph per colour (`glyphs[0]='.'`).
Done when: `CONFIG['palette']` has `colors`, `glyphs`, `ramps`; ≤16 colours per small asset is achievable.

**Step 4 — Scaffold.**
```bash
mkdir -p $OUT/tools && cp $SKILL/assets/project_template/project.py $OUT/tools/
```
Fill `CONFIG` (id, title, subtitle, seed, brief, palette, `board_cell`). Replace the template's demo `populate` with stubs per category module (`tools/terrain.py` … each exposing `populate(lib)`).
Done when: `python3 $KIT/build_library.py --project $OUT --limit 1` exits 0.

**Step 5 — Block-in** ([skills/draw](skills/draw/SKILL.md), [references/silhouette-and-form.md](references/silhouette-and-form.md)). Per asset: silhouette with outline + 1 flat mass colour using `Grid.stamp/rect/oval/line/mirror_x`; parametrise variants instead of copy-pasting.
```bash
python3 $KIT/build_library.py --project $OUT --limit 8      # smoke build, manifest.partial=true
```
Look at `$OUT/previews/*.png` (screenshot). Done when: each asset reads at 1× by silhouette alone.

**Step 6 — Shade & detail** ([skills/shade](skills/shade/SKILL.md), [references/shading-and-light.md](references/shading-and-light.md)). Apply ramps from the Step 1 light direction; selective outline; highlights 1–2 px only where earned. Tiles: `tile=True`, keep opposite edges matching.
Done when: smoke previews show consistent light and no new colours outside the palette (build asserts colour budget).

**Step 7 — Variants / animation** ([skills/animate](skills/animate/SKILL.md), [skills/tileset](skills/tileset/SKILL.md)). When motion is requested or frames already exist, register each authored frame as a separate asset with `sequence`, zero-based `frame`, positive integer `duration_ms`, and optional `direction`. Give each direction its own sequence. The gallery automatically plays sequences with 2+ frames; static variants without `sequence` stay static. Do not fake sprite animation with CSS transforms. For downloadable GIFs/animated sheets use Workflow B's recipe `anims` separately.
Done when: frame indices are unique and contiguous from zero; each sequence shares canvas, `source_rect` and direction; `index.html#animations` shows the expected sequences and original per-frame timing. See [references/library.md](references/library.md) for the metadata contract.

**Step 8 — Hero scene (optional).** Add `scene(lib)` composing only existing assets via `lib.place`.
Done when: `previews/scene.png` appears after the full build.

**Step 9 — Full build.**
```bash
python3 $KIT/build_library.py --project $OUT --clean
```
Done when: exit 0, JSON shows `"failed": []`, and `$OUT/manifest.json` has `"partial": false`. A failure lists the asset id → fix it in Step 5/6.

**Step 10 — Verify.**
```bash
python3 $KIT/verify_library.py --project $OUT --samples <id>,<id>
```
Done when: `"status": "PASS"` (links, SHA-256, palette membership, `source_rect` bounds, tile seams, clean-rebuild determinism). Pillow missing → pixel checks `NOT_ASSESSED`, say so.

**Step 11 — Visual review** ([skills/review](skills/review/SKILL.md), [references/review-checklist.md](references/review-checklist.md)).
```bash
python3 -m http.server 8000 --directory $OUT     # open http://127.0.0.1:8000/index.html
```
Screenshot `index.html` and 2+ preview boards; check readability at 1×, light consistency, style coherence across categories, search/lightbox working. For animation, verify wraparound and non-uniform frame timing, play/pause, next-frame/frame selection, speed, integer zoom, cropped `source_rect`, background-tab suspension and reduced-motion default pause. Test direct `file://` opening (no manifest fetch) and narrow screens. Keep `animation_playback: NOT_ASSESSED` until playback has actually been checked; asset validation alone is not animation verification.
Done when: checklist passes. Otherwise fix with [skills/fix](skills/fix/SKILL.md) → back to Step 9.

**Step 12 — Deliver.** Report: `$OUT` path, asset/category counts, `validation.json` status, what was visually reviewed (else `visual_review: NOT_ASSESSED`), deferred items, and how to rebuild (`PXL_BIN=$PXL python3 $OUT/tools/build_library.py`). Never claim PASS without evidence.

## Workflow B — Single sprite (step by step)

```bash
W=/tmp/<sprite>; mkdir -p $W/<id>       # palette.json in $W, sources in $W/<id>/
```

1. **Brief + concept** — size, view, light, palette ([skills/brief](skills/brief/SKILL.md), [skills/concept](skills/concept/SKILL.md)).
2. **Palette** — write `$W/palette.json` (`{v, id, colors, ramps}`) or use built-in `p16` (`$PXL --json palette show p16`).
3. **Scaffold** ([skills/new](skills/new/SKILL.md)) — `$W/<id>/<id>.ascii.json` (`$PXL schema ascii`) + `<id>.recipe.json` (`$PXL schema recipe`).
4. **Block-in** ([skills/draw](skills/draw/SKILL.md)) — edit ASCII rows; preview fast: `$PXL ascii render $W/<id>/<id>.ascii.json --scale 8 -o $W/block.png`.
5. **Shade** ([skills/shade](skills/shade/SKILL.md)) — remap masses to ramp glyphs; re-render.
6. **Build + validate** — `$PXL build $W/<id>/<id>.recipe.json -o $W/out` then `$PXL validate $W/out/<id>.pxl.json --json` (exit 5 = QA fail → Step 4/5).
7. **Look** — `$PXL inspect $W/out/<id>.pxl.json`; `$PXL render $W/out/<id>.pxl.json -o $W/preview.png --scale 8`; screenshot it ([skills/review](skills/review/SKILL.md)).
8. **Animate (optional)** ([skills/animate](skills/animate/SKILL.md)) — add `anims` to the recipe; `$PXL render … --anim <name> -o walk.gif --scale 4`.
9. **Export** ([skills/export](skills/export/SKILL.md)) — `$PXL render $W/out/<id>.pxl.json --asset -o $W/<id>.png`; sheets via `$PXL sheet`; `.aseprite` via `$PXL ase convert`.

## Workflow C — Edit existing art (step by step)

1. **Inspect** — `$PXL inspect <doc>` / `$PXL ase inspect <file.aseprite>`; PNG input → `$PXL pixelize --help` to get an indexed doc first.
2. **Baseline** — `$PXL validate <doc> --json` and a `--scale 8` render; keep both for comparison.
3. **Scope the change** with [skills/fix](skills/fix/SKILL.md): list what must stay untouched.
4. **Edit** the source (ASCII/recipe) or use `$PXL op` / `$PXL ase layer|cel|frame|tag …` — smallest change first.
5. **Re-validate + re-render**; compare before/after screenshots side by side.
6. **Export** as in Workflow B Step 9 (write to a new path; never overwrite the user's original without `--overwrite` and consent).

## Authoring tips (library)

- Split art into sibling modules ≤500 lines each; `project.py` only imports and calls them.
- 16×16 art: 32 canvas + `region=[0, 0, 16, 16]`; buildings 64; `tile=True` for seamless terrain.
- `lib.g('wood', 2)` keeps code palette-agnostic; `lib.place` reuses assets in scenes.
- `--limit N` for fast iterations, `--clean` after renaming/removing ids.
- `build_library.py` copies the kit into `$OUT/tools/`, so the folder is self-describing like `examples/stardew`. Python 3.8+; Pillow optional.

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
| Asset pack → `index.html` library | [references/library.md](references/library.md) |

## References (craft)

- [references/core-principles.md](references/core-principles.md)
- [references/palette-and-color.md](references/palette-and-color.md)
- [references/shading-and-light.md](references/shading-and-light.md)
- [references/silhouette-and-form.md](references/silhouette-and-form.md)
- [references/animation.md](references/animation.md)
- [references/review-checklist.md](references/review-checklist.md)
- [references/cli-cheatsheet.md](references/cli-cheatsheet.md) — real flags of the bundled binary
- [references/library.md](references/library.md) — library folder, `project.py`, `lib` API, porting

## Out of scope

- Full design→package→engine game pipeline → use `pxl`.
- Building or updating the binary (maintainers: repo `scripts/build-pxl-asset-gen.sh`).
- Aseprite GUI / WebSocket / third-party MCP Lua bridges.
- Tile-management plugins beyond what `$PXL ase export-tileset` / `tile gen` already do.
