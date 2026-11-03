# v8.1 进度状态

> 日期：2026-11-03（Asia/Shanghai）  
> 诚实标准：与 v8.0 相同——未跑检查不得写 PASS；无证据不得写「已完成」。

| 任务 | 优先级 | 状态 | 证据 / 备注 |
|---|---|---|---|
| GC-1 矩阵补行 | P0 | 已完成 | `cli-flag-matrix.md` GC-1 节；`d223d75` |
| GA-1 User Data 0x2020 | P0 | 已完成 | tag 颜色 roundtrip |
| GA-6 写回丢失警告 | P0 | 已完成 | `read_loss_notes` → stderr |
| GB-1 cel z-index 渲染排序 | P0 | 已完成 | NOTE.5；`z_index_changes_paint_order` |
| GA-2 Color Profile 透传 | P1 | 已完成 | `Sprite.color_profile_raw`；`color_profile_passthrough_roundtrip` |
| GA-3 Cel Extra | P1 | 已完成 | `CelExtra` 挂 cel；写回；渲染忽略；`cel_extra_roundtrip` |
| GA-4 header/layer 位 + pixel ratio | P1 | 已完成 | `header_flags` / `PixelRatio` / `LayerFlags` 全位 / layer UUID（header bit4） |
| GA-5 External Files | P2 | 未开始 | 仍 skip + loss note |
| GB-2 blend 分期 | P1 | 部分完成 | NORMAL+MULTIPLY/SCREEN/OVERLAY/ADDITION/SUBTRACT（`render/blend.rs` 单测）；其余模式仍回落 NORMAL |
| GB-3 组层合成 | P1 | 未开始 | |
| GB-4 Playback 导出语义 | P1 | 已完成 | `plan/v8.1/playback.md`；`expand_tag_frames` ping-pong+repeat；frames 单测 |
| GB-5 `--shrink-to` | P2 | 未开始 | |
| GC-2 split/slice/ignore/trim-sprite/padding | P1 | 部分完成 | `--slice/--ignore-layer/--split-layers/--split-tags/--trim-sprite` 已实现；border/shape/inner 已接 flag 但 pack 仍单 pad（矩阵「部分」） |
| GC-3..5 / GD / GE | P1–P2 | 未开始 | |

验证：`cargo +1.95.0 test -p pxl-ase-io -p pxl-ase-doc -p pxl-ase --lib`；`cargo +1.95.0 check -p pxl-cli`。

状态词：`已完成` / `部分完成` / `后续` / `未开始` / `NOT_ASSESSED`。
