# v8.1 进度状态

> 日期：2026-11-03（Asia/Shanghai）  
> 诚实标准：与 v8.0 相同——未跑检查不得写 PASS；无证据不得写「已完成」。

| 任务 | 优先级 | 状态 | 证据 / 备注 |
|---|---|---|---|
| GC-1 矩阵补行 | P0 | 已完成 | `plan/v8.0/cli-flag-matrix.md`「v8.1 GC-1」节；对照 `app_options.h` + `libs/pxl-cli/src/cmd/ase/`；commit `d223d75` |
| GA-1 User Data 0x2020 | P0 | 已完成 | `pxl-ase-doc::UserData`；读/写挂 layer/cel/tag/slice/sprite；tag 颜色 roundtrip 测试 `tag_color_user_data_roundtrip`；properties map 为 raw 透传 |
| GA-6 写回丢失警告 | P0 | 已完成 | 读侧记入 `Sprite.read_loss_notes`（Cel Extra / Color Profile / External Files / Tileset / palette 名 / pixel ratio）；`write_aseprite_bytes` → stderr `pxl-ase-io write warning:` |
| GB-1 cel z-index 渲染排序 | P0 | 已完成 | `render::flatten_frame_rgba` 按 NOTE.5（`layerIndex+zIndex`，同分比 `zIndex`）；测试 `z_index_changes_paint_order` |
| GA-2 Color Profile 透传 | P1 | 未开始 | 现仅跳过 + 写回警告 |
| GA-3 Cel Extra | P1 | 未开始 | 同上 |
| GA-4 header/layer 位 + pixel ratio | P1 | 未开始 | pixel ratio 有 loss note，未建模 |
| GA-5 External Files | P2 | 未开始 | |
| GB-2 blend 分期 | P1 | 未开始 | |
| GB-3 组层合成 | P1 | 未开始 | |
| GB-4 Playback 导出语义 | P1 | 未开始 | |
| GB-5 `--shrink-to` | P2 | 未开始 | |
| GC-2..5 / GD / GE | P1–P2 | 未开始 | 见 `01-缺口分析与增补总表.md` |

验证（本机）：`cargo +1.95.0 test -p pxl-ase-io -p pxl-ase-doc -p pxl-ase` 全绿；`cargo +1.95.0 check -p pxl-cli` 见同日提交说明。

状态词：`已完成` / `部分完成` / `后续` / `未开始` / `NOT_ASSESSED`。
