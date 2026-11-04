# Asset library (index.html deliverable)

Final delivery for any multi-asset request is a **self-contained folder** browsable via `index.html`
(same shape as `examples/stardew`). Built by `assets/library-kit/build_library.py`; checked by `assets/library-kit/verify_library.py`.

## Contents

- Folder layout
- `tools/project.py` contract
- `lib` API (art_kit)
- Gotchas
- Porting an existing generator

## Folder layout

```
<out>/
  index.html            # search, category nav, thumbnails cropped to source_rect, lightbox, atlases, boards
  manifest.json         # library/title/seed/tool_version/partial/count/categories/brief/scope/deferred/hero/atlases/previews/assets[]
  validation.json       # `validate` per asset + verify_library export_integrity / tile edges / determinism
  palette.json          # {v, id, colors, ramps, special}; index 0 transparent
  assets/<id>/<id>.png  # transparent asset PNG (render --asset)  + .pxl.json doc + .sheet.png/.json + build.json
  sources/<id>/<id>.ascii.json | <id>.recipe.json   # the only authored inputs the binary consumes
  atlases/<category>.png|.json                      # `sheet --layout grid --pad 1`
  previews/<category>_NN.png|.index.json|.ascii.json, scene.png   # ascii render --scale 3 overview boards
  tools/project.py (+ sibling modules) + vendored art_kit.py build_library.py gallery.py animation_gallery.py verify_library.py
```

Manifest asset entry: `id name category section source_rect canvas png doc source recipe sha256` + any extra
kwargs passed to `lib.add` (e.g. `tile`, `tags`, `frame`, `direction`).

Rebuild standalone later: `PXL_BIN=<skill>/bin/pxl-asset-gen-<platform> python3 <out>/tools/build_library.py`
(no `--project` needed; the vendored copy cannot locate the skill `bin/` by itself).

## `tools/project.py` contract

```python
from art_kit import Grid
CONFIG = dict(
    id='mygame', title='…', eyebrow='…', subtitle='…', note='…', license_note='…',
    seed=1104, scope='…', deferred=['…'], brief={'view': '…', 'light': 'upper-left', 'tile_size': 16},
    palette=dict(id='…', glyphs='.#bt…', colors=['#00000000', …], ramps={'wood': [2, 3, 4]}, special={…}),
    max_colors=16, max_colors_large=24,          # per-asset glyph budget (<64 / >=64 canvas)
    board_cell={'buildings': 64}, board={'bg': 'b', 'frame': '#'}, preview_scale=3,
    theme={'bg': '#1c2926', 'accent': '#e4bc57'}, lang='zh-CN',
)
def populate(lib): ...            # required: call lib.add(...) for every asset
def scene(lib): return Grid(256)  # optional hero composite built only from lib.place(...)
```

Only `id`, `palette.colors` and `populate` are required. `glyphs[0]` must be `'.'`; one unique glyph per colour.

## `lib` API (art_kit)

| Call | Purpose |
| --- | --- |
| `Grid(size=32, fill='.')` | square canvas; **size ∈ {32, 64, 128, 256}** (`ascii` rule) |
| `.mark .rect .frame .line .oval .stamp(x, y, text, remap) .mirror_x .replace .clone .crop` | glyph placement only |
| `lib.add(id, name, category, grid, section=None, region=None, **meta)` | register; `region` = `source_rect` `[x, y, w, h]` |
| `lib.g('wood', 2)` | glyph of ramp level (keeps code palette-agnostic) |
| `lib.place(target, id, x, y)` | stamp an asset's `source_rect` into a scene/board |

Conventions: ids `snake_case`; 16×16 art goes top-left in a 32 canvas with `region=[0, 0, 16, 16]`;
frames/directions are separate ids (`farmer_south_0`, `farmer_south_1`);
seamless tiles pass `tile=True` (verifier compares opposite edges inside `source_rect`).

### Animation showcase contract

Register frames with `lib.add(..., sequence='farmer_walk_south', frame=0, duration_ms=180, direction='south')`.
Use a distinct `sequence` per action/direction, contiguous integer `frame` indices from 0,
positive integer `duration_ms`, and identical `canvas`, `source_rect`, and `direction` within a sequence.
Author genuinely different poses/pixels. Register frames in order so `--limit` smoke builds contain a prefix.
Do not infer animation from numeric ID suffixes: crop stages and colour variants are not automatically cycles.

`animation_gallery.py` groups explicit sequences with 2+ frames and embeds a file-safe player in
`index.html#animations`. It loops in frame order using each manifest duration, crops `source_rect`,
preloads PNGs and provides play/pause, frame selection/step, speed and integer zoom.
Single-frame sequences stay static; invalid metadata fails gallery generation instead of silently guessing.
The player suspends in background tabs and defaults to paused under `prefers-reduced-motion`.
No fetch/CDN/server is required; include the module in vendored `tools/` for standalone rebuilds.

Individual frame recipes remain static `idle` assets. Their atlas `duration=1000` is not animation timing:
use manifest `duration_ms` instead. GIF/animated sheet exports require a separate animated recipe.
Preserve `animation_playback: NOT_ASSESSED` until playback is actually verified, even when asset validation passes.
For existing packs, regenerate the gallery from the manifest without rebuilding unchanged PNGs;
Stardew supports `python3 examples/stardew/tools/build.py --gallery-only`.

## Gotchas

- `ascii.bad_size`: canvas 16 is rejected — use 32 + `region`.
- Glyph assertion errors come from typos in stamps; `'.'` in stamps is always skipped (transparent).
- Colour budget assertion: split ramps or raise `max_colors` deliberately, not silently.
- `--limit N` marks `manifest.partial=true`; always finish with a full build before delivery.
- `--clean` wipes `assets/ sources/ atlases/ previews/` — use when ids were renamed/removed.
- `visual_review` stays `NOT_ASSESSED` until you actually look at `previews/*.png` / `index.html` screenshots.

## Porting an existing generator

Modules written against a global `add(...)` (like `examples/stardew/tools/*.py`) work through a shim:

```python
# tools/art_core.py
from art_kit import Grid  # noqa: F401
PENDING = []
def add(asset_id, name, category, grid, section, region=None, **meta):
    PENDING.append((asset_id, name, category, grid, section, region, meta)); return grid
# tools/project.py -> populate(lib): run modules, then lib.add(*row[:6], **row[6]) for each PENDING row
```

Verified: porting `examples/stardew` this way reproduces all 290 PNGs byte-identically (same SHA-256).
