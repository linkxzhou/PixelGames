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
| GA-5 External Files | P1/P2 | 已完成 | `external_files_raw` 透传；`external_files_passthrough_roundtrip` |
| GB-2 blend 全集 | P1 | 已完成 | 19 种 `BlendMode` 均有路径（`render/blend/`）；单测 smoke + HSL |
| GB-3 组层合成 | P1 | 已完成 | header bit2 / Group：子层先合成再按组 blend；`group_composites_children_then_blends` |
| GB-4 Playback | P1 | 已完成 | `playback.md` + expand_tag_frames |
| GB-5 `--shrink-to` | P2 | 未开始 | |
| GC-2 split/slice/… | P1 | 部分→补齐 padding | split/slice/ignore/trim-sprite 已有；**padding 三元组现已真实分拆** |
| GC-3 `--format json-array` | P1 | 已完成 | `sheet_json_format`；默认 `json-hash` 不变；`ase sheet --format` |
| GC-4.. / GD / GE | P1–P2 | 未开始 | |

验证：`cargo +1.95.0 test -p pxl-export -p pxl-ase-io -p pxl-ase-doc -p pxl-ase --lib`；`check -p pxl-cli`。
