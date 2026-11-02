# v2.0：Harness 与素材质量阶段（进行中）

> 归档/更新：2026-10-23。本目录收录 **v2.0 正在实施或近期完成** 的规划文档，覆盖 Harness Agent 编排、素材质量反馈闭环、crates 模块拆分与 ASCII 布局输入等新能力。

## 阶段定位

v2.0 在 v1.0 的 `pxl` CLI 骨架之上，叠加 **`pxl-harness` Agent 编排（H0–H2 已落地）**，并围绕 **素材质量的闭环与可信交付**（V3/V4）持续迭代。质量改进计划（Q2 已完成）与 ASCII 布局输入（A1–A5 已完成）是本阶段的两条独立工作线。

## 文档索引

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [pixelgames-harness-design-plan.md](pixelgames-harness-design-plan.md) | 计划 | H0–H2 已实现 | Harness 总设计；H3 部分（预算持久化已做，跨进程 resume 另立项）；H4/H5 未开始 |
| [harness-asset-feedback-repair-plan.md](harness-asset-feedback-repair-plan.md) | 计划 | P0-A~D 已实施 | 素材反馈与调度闭环；P1-A/B 部分落地 |
| [harness-v3-agent-capability-plan.md](harness-v3-agent-capability-plan.md) | 计划 | 已全部实施 | V3-A/B/C/D：tools 模块化 / MCP / Goal / 并行+压缩+`agent` CLI |
| [harness-v4-battle-city-asset-quality-plan.md](harness-v4-battle-city-asset-quality-plan.md) | 计划 | 待实施 | 交付证据、需求契约、视觉审阅、受控修复、资源成本（V4-A~E） |
| [crates-module-split-plan.md](crates-module-split-plan.md) | 计划 | 已完成 | 6 个长文件全部拆分，crates 内所有 .rs < 600 行 |
| [pxl-quality-improvement-plan.md](pxl-quality-improvement-plan.md) | 计划 | Q2 完成 | Q2.1–Q2.6 全部落地；Q0/Q1/Q3–Q6 未实施 |
| [pxl-ascii-layout-plan.md](pxl-ascii-layout-plan.md) | 计划 | 已实现 | ASCII 布局 → `*.ascii.json` → `PxlDoc` → PNG/GIF；A1–A5 已落地，P2 项未做 |

状态图例：**已实现/已完成**（代码或产物可核实）｜**部分**（列出已做/未做）｜**未开始**。

## 与 v1.0 的关系

- v1.0 确立了 `pxl` 的确定性模型、文件格式与 CLI 骨架（见 `../v1.0`），v2.0 不破坏这些约定。
- 本阶段大量设计引用 v1.0 的能力与约束（如 `cli-design.md` 的 `pxl mcp`、`miniagent-integration.md` 的方案 B），阅读时请按需回溯 `../v1.0`。