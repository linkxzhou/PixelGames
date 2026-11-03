# v8.1 进度状态

> 日期：2026-11-03（Asia/Shanghai）  
> 诚实标准：与 v8.0 相同——未跑检查不得写 PASS；无证据不得写「已完成」。

| 任务 | 优先级 | 状态 | 证据 / 备注 |
|---|---|---|---|
| GC-1 矩阵补行 | P0 | 已完成 | `cli-flag-matrix.md` |
| GA-1 / GA-6 / GB-1 | P0 | 已完成 | 见既有提交 |
| GA-2 Color Profile | P1 | 已完成 | `color_profile_raw` roundtrip |
| GA-3 Cel Extra | P1 | 已完成 | `CelExtra` roundtrip |
| GA-4 header/layer/pixel ratio | P1 | 已完成 | flags / UUID / `PixelRatio` |
| GA-5 External Files | P1/P2 | 已完成 | `external_files_raw` 透传 |
| GB-2 blend 全集 | P1 | 已完成 | 19 种 `BlendMode` |
| GB-3 组层合成 | P1 | 已完成 | header bit2 / Group |
| GB-4 Playback | P1 | 已完成 | `playback.md` + expand_tag_frames |
| GB-5 `--shrink-to` | P2 | 已完成 | `ops::shrink_sprite_to` |
| GC-2..4 | P1–P2 | 已完成 | split/padding/format/sheet 高级旗标 |
| GD-1..6 / GD-8 | P1–P2 | 已完成 | frame/tag/layer/cel/flatten/sprite/palette-sort |
| GD-7 帧级 palette | P2 | 已完成 | `frame_palettes` 读写；`ase palette set/add/remove/clear-frame --frame`；`frame_palette_roundtrip` |
| GE-1..3 | P1–P2 | 已完成 | convolve/median/curve |
| GE-4 target/tiled | P2 | 已完成 | `FilterOpts` + `--channels`/`--tiled`；见 `ge4-filter-boundary.md` |
| GE-5 tileset + 渲染 | P2 | 已完成 | 读 0x2023/cel3；`export-tileset`；**tilemap 层参与 flatten**（`tilemap_paints_tiles_into_flatten`） |

验证：`cargo +1.95.0 test -p pxl-ase-doc -p pxl-ase-io -p pxl-ase --lib`；`check -p pxl-cli`。
