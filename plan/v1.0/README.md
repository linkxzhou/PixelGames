# v1.0：CLI 与坦克演示阶段（归档）

> 归档：2026-10-23。本目录收录 **v1.0 已收敛/已完成** 的规划与设计文档，供历史回溯，一般不再更新。

## 阶段定位

v1.0 以 **确定性的 `pxl` 像素素材 CLI + 坦克演示 + miniagent skill 集成** 为核心，对应路线图 M0/M1。核心能力已落地并稳定，后续迭代（harness 能力底座、素材质量闭环、ASCII 布局等）已迁入 `../v2.0`。

## 文档索引

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [roadmap.md](roadmap.md) | 路线图 | v1 阶段完成 | M0/M1 落地；M2+ 的推进已在 v2.0 质量/能力计划中展开 |
| [architecture.md](architecture.md) | 设计 | 已实现 | crates 布局与第三方许可约束；`pxl-wasm` 仍 deferred |
| [asset-pipeline.md](asset-pipeline.md) | 设计 | 已实现 | 数据模型、op DSL、recipe、parts；含 1.6 ASCII 布局（落地记录） |
| [cli-design.md](cli-design.md) | 设计 | 已实现 | 全部子命令落地；`pxl mcp`（M2）未实现 |
| [miniagent-integration.md](miniagent-integration.md) | 设计+记录 | 方案 A 已落地 | bash + skill 已通；方案 B `pxl mcp` 未做 |
| [game-dev-pipeline-harness-plan.md](game-dev-pipeline-harness-plan.md) | 调研 | 已完成 | 背景调研稿；已由 harness-design-plan 收敛实施 |
| [reference-art-analysis.md](reference-art-analysis.md) | 分析 | 已完成 | 参考素材盘点与可行性分档 |
| [pxl-quality-analysis.md](pxl-quality-analysis.md) | 分析 | 已完成 | R1–R10 根因分析，作为 v2.0 质量改进计划的输入 |

状态图例：**已实现/已完成**（代码或产物可核实）｜**部分**｜**未开始**｜**归档**（历史输入，不再更新）。

## 与 v2.0 的边界

- v1.0 确立 `pxl` 的确定性模型、文件格式与 CLI 骨架，后续版本不破坏这些约定。
- v2.0 在 v1.0 之上叠加 harness Agent 编排、素材质量反馈闭环、ASCII 布局输入等新能力。
- 跨版本共享的更新点（如 `cli-design.md` 里新加的 `pxl ascii`）以各自文档内标注的落地日期为准。