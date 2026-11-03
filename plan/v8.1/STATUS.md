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
| GD-1 frame CRUD | P1 | 已完成 | `edit::frame_*` + `ase frame …`（非官方）；tag 边界单测 |
| GD-2 tag CRUD | P1 | 已完成 | `ase tag add/remove/rename/set` |
| GD-3 layer CRUD | P1 | 已完成 | `ase layer add/remove/rename/set` |
| GD-4 cel ops | P1 | 已完成 | `ase cel move/unlink/clear/set` |
| GD-5 flatten | P1 | 已完成 | `ase flatten` 写回单层 |
| GD-6 sprite attrs | P2 | 已完成 | `ase sprite set` transparent/pixel-ratio/grid |
| GD-7 帧级 palette | P2 | 不做 | 模型仍为文档级 `sprite.palette` |
| GD-8 palette sort | P2 | 已完成 | `ase palette sort --by hue\|sat\|lum` |
| GE-1 convolution | P1 | 已完成 | `ase filter convolve` + Kernel JSON |
| GE-2 median | P1 | 已完成 | `ase filter median`；半径 cap=3 |
| GE-3 color curve | P2 | 已完成（最小） | `ase filter curve` + `--curve-json` |
| GE-4 target/tiled 文档 | P2 | 已完成 | `plan/v8.1/ge4-filter-boundary.md`（clamp，非 wrap） |
| GE-5 tileset 最小 | P2 | 已完成 | 读 0x2023 + cel type3；`ase export-tileset`；tilemap 层渲染仍跳过 |

验证：`cargo +1.95.0 test -p pxl-ase-doc -p pxl-ase-io -p pxl-ase --lib`；`check -p pxl-cli`。
