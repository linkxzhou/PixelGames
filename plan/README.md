# PixelGames 计划总览

> 更新：2026-10-22。本文是 `plan/` 的**索引与状态总表**。历史版本（v1 Web 编辑器方向、v2 "只做规划不写代码"）已归档到 git 历史（`65f7e2a..8964bc9`），不再在此赘述。

## 当前阶段

**CLI 与 Harness 均已实现并可用**：

- `crates/`：`pxl` 确定性像素素材 CLI（core / gen / ops / qa / format / export / cli），黄金测试跨平台一致。
- `harness/`：`pxl-harness` Agent 编排（design + assets 双阶段、权限交集、预算账本、事件审计、候选目录、Goal 验收闭环、MCP 接入、并行工具执行）。
- `review/`：v1–v4 素材质量对比归档（v3 为 100 张配方对照）。
- 全工作区 `cargo test` 176 项全绿（2026-10-22 实测）。

## 文档索引与状态

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [roadmap.md](roadmap.md) | 路线图 | 执行中 | M0/M1 完成，M2 部分（MCP 未做），M3/M4 未开始 |
| [architecture.md](architecture.md) | 设计 | 已实现 | crates 布局与第三方许可约束；`pxl-wasm` 仍为 deferred |
| [crates-module-split-plan.md](crates-module-split-plan.md) | 计划 | 已完成 | 6 个长文件全部拆分，crates 内所有 .rs < 600 行，全工作区测试通过 |
| [cli-design.md](cli-design.md) | 设计 | 已实现 | 全部子命令落地；`pxl mcp`（M2）未实现 |
| [asset-pipeline.md](asset-pipeline.md) | 设计 | 已实现 | 数据模型、op DSL、recipe、parts 均落地 |
| [miniagent-integration.md](miniagent-integration.md) | 设计+记录 | 方案 A 已落地 | bash + skill 已通；方案 B `pxl mcp` 未做 |
| [pixelgames-harness-design-plan.md](pixelgames-harness-design-plan.md) | 计划 | H0–H2 已实现 | H3 部分（预算持久化已做，跨进程 resume 另立项）；H4/H5 未开始 |
| [harness-asset-feedback-repair-plan.md](harness-asset-feedback-repair-plan.md) | 计划 | P0-A~D 已实施 | 提交 `23d05c3`；P1-A 部分（static 动画已落地，effect policy 未做）；P1-B 部分受限 schema 工具已开 |
| [harness-v3-agent-capability-plan.md](harness-v3-agent-capability-plan.md) | 计划 | 已全部实施 | V3-A/B/C/D 完成（tools 模块化 / MCP / Goal / 并行+压缩+`agent` CLI），待提交 |
| [pxl-quality-improvement-plan.md](pxl-quality-improvement-plan.md) | 计划 | Q2 完成 | Q2.1–Q2.6 全部落地（文内有明细）；Q0/Q1/Q3–Q6 未实施 |
| [pxl-quality-analysis.md](pxl-quality-analysis.md) | 分析 | 已完成 | R1–R10 根因分析，作为改进计划的输入，归档 |
| [reference-art-analysis.md](reference-art-analysis.md) | 分析 | 已完成 | 参考素材盘点与可行性分档，归档 |
| [game-dev-pipeline-harness-plan.md](game-dev-pipeline-harness-plan.md) | 调研 | 已完成 | 背景调研稿；已由 harness-design-plan 收敛实施，仅作参考 |

状态图例：**已实现/已完成**（代码或产物可核实）｜**部分**（列出已做/未做）｜**未开始**｜**归档**（历史输入，不再更新）。

## 下一步候选（按现有计划优先级）

1. 提交工作区中的 V3 实现（harness agent 层 + `plan/harness-v3-agent-capability-plan.md`）。
2. `harness-asset-feedback-repair-plan.md` 的 P1 收尾：effect/one-shot QA policy、能力元数据补全（P1-B 剩余）。
3. `pxl mcp`（miniagent-integration 方案 B / cli-design M2）——V3-B 的 MCP 客户端已就绪，只差 server 端。
4. 质量线：Q0 公平基线协议与 Q1 样板重做（quality-improvement-plan）。
5. H3 剩余（缓存失效传播、试运行评估）与 H4 Godot spike（harness-design-plan）。

## 已知边界

- `thirdparty/` 只读参考：miniagent（无 Cargo.toml，结构移植来源）、Pixelorama、piskel、Sprite-Generator（无许可，不复制代码）。
- 未验证项沿各文档标注【未验证】，不在此重复。
