# v8.1 进度状态

> 日期：2026-11-03（Asia/Shanghai）  
> 诚实标准：与 v8.0 相同——未跑检查不得写 PASS；无证据不得写「已完成」。

| 任务 | 优先级 | 状态 | 证据 / 备注 |
|---|---|---|---|
| GC-1 矩阵补行 | P0 | 已完成 | `plan/v8.0/cli-flag-matrix.md`「v8.1 GC-1」节；对照 `app_options.h` + `libs/pxl-cli/src/cmd/ase/` |
| GA-1 User Data 0x2020 | P0 | 后续 | 本轮实施中 / 见提交 |
| GA-6 写回丢失警告 | P0 | 后续 | 依赖读侧记录；本轮实施中 |
| GB-1 cel z-index 渲染排序 | P0 | 后续 | NOTE.5；本轮实施中 |
| GA-2 Color Profile 透传 | P1 | 未开始 | |
| GA-3 Cel Extra | P1 | 未开始 | |
| GA-4 header/layer 位 + pixel ratio | P1 | 未开始 | |
| GA-5 External Files | P2 | 未开始 | |
| GB-2 blend 分期 | P1 | 未开始 | |
| GB-3 组层合成 | P1 | 未开始 | |
| GB-4 Playback 导出语义 | P1 | 未开始 | |
| GB-5 `--shrink-to` | P2 | 未开始 | |
| GC-2..5 / GD / GE | P1–P2 | 未开始 | 见 `01-缺口分析与增补总表.md` |

状态词：`已完成` / `部分完成` / `后续` / `未开始` / `NOT_ASSESSED`（有代码未跑验证时用）。
