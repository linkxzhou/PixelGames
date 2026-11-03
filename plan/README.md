# PixelGames 计划总览

> 更新：2026-11-02。本文是 `plan/` 的**索引与状态总表**。**v5.0** 的 miniquad wasm + JS 游戏运行时与文本音频 A0–A4 工程已落地；**v6.0 通用像素美术与生成流程**的 P0–P5 工程已实现，真实视觉评审与真实模型 A/B 仍待开展；**v7.0 NES 素材模型**调研与落地计划（N0）已落成，N1–N4 工程未开始；**v8.0 Aseprite→Rust CLI** 计划已落成且工程已推进（M0–M8 主线已完成；M7 含 trim-by-grid/play-subtags/最小顺序 argv；M9 脚本部分完成），见 [STATUS.md](v8.0/STATUS.md)。v1.0–v4.0 文档保留为实施记录与历史方案。

## 当前实施线：v5.0、v6.0、v7.0 与 v8.0（计划）

- [web-js-engine-plan.md](v5.0/web-js-engine-plan.md)：S0–S6 与音频 A0–A4 工程已实施。JS 默认模式与纯 Rust 模式并存；step-500 支持静态构建、WAV 播放和双层 playtest；A5 未开始。
- 验证证据：battle-city 的 3 SFX + 1 BGM 双渲染一致；无头玩法/音频事件哈希一致；真实 Chromium 键盘操作与 BGM 三轮播放无 console error；skill clean-PATH 与音频迁出验证通过。人工听审仍为 NOT_ASSESSED，其他游戏迁移与 native 退役属于 S7。
- [音乐合成调研报告.md](v5.0/音乐合成调研报告.md)（2026-10-30）：程序化/代码即音乐调研与实施记录（无大模型）。ZzFX 命名参数 + BeepBox JSON 子集通过 `step-300 audio` 离线输出 WAV，贯通规格、打包和 JS 播放。配套 [sources.md](v5.0/sources.md)、[v5.0 索引](v5.0/README.md)。
- [pxl-skill-binaries-plan.md](v5.0/pxl-skill-binaries-plan.md)：macOS arm64 内置二进制与 §11 音频工具链已实施；固定适配器、依赖和许可证随 skill 交付，缺依赖负例通过；其他平台未评估。



## v6.0 RSI 与美术表现力（通用美术工程已实现）

> 索引：[v6.0/README.md](v6.0/README.md)。不改五大步结构；与 v5.0 音频、v4.0 门禁原则并存。

- [通用像素美术与生成流程实施计划.md](v6.0/通用像素美术与生成流程实施计划.md)：P0–P5、通用 showcase、L1/L2 光照与受控知识回流已实现；100→500 新链路、48 个独立构建样本、Rust/JS 测试与 WASM 构建通过。未修改 `games/`，不含 L3；真实两名独立评审、真实模型配对 A/B 和保留集人工结论为 NOT_ASSESSED。
- [rsi-producer-evolution-plan.md](v6.0/rsi-producer-evolution-plan.md)：制作人质量闭环 — V6-A 游戏性验收、**V6-B 视觉验收回流**、V6-C 跨游戏知识库、V6-D RSI 提炼、V6-E 音频贯通、V6-F 策划案分解。
- [美术光照与引擎材质改进计划.md](v6.0/美术光照与引擎材质改进计划.md)（2026-10-30）：美术总监视角。证据来自 `games/*/review/external-vs-runtime.png` 与 `review_image.py`（如 dead-cells 单明度带 >85%、stardew 精确色 2924）。内容定义「好看」（限色板、hue-shift 斜坡、像素对齐 2D 光照、材质角色、Juice）；**V6-B 是回流 harness**。配套 [sources-art.md](v6.0/sources-art.md)。


## v7.0 NES 素材模型（调研落成 · N0）

> 索引：[v7.0/README.md](v7.0/README.md)。不改五大步；与 v6.0 美术限色 / V6-B 回流对接。

- [NES素材实现调研与落地计划.md](v7.0/NES素材实现调研与落地计划.md)（2026-11-01）：NES/FC 硬件拼砖模型（CHR + Nametable + Attribute + Palette + OAM）与 PixelGames 资产契约启示；里程碑 N0–N4。配套 [sources.md](v7.0/sources.md)。
- **一句话**：先定 4 色砖与 palette，再拼关卡；NES 约束作硬限色默认档。非目标：完整模拟器、抄 ROM CHR、真 MMC3 时序。
- 与美术交叉：可作为 [v6.0 美术计划](v6.0/美术光照与引擎材质改进计划.md) A 轨硬限色 preset；N4 挂 V6-B 门禁。


## v8.0 Aseprite→Rust CLI（工程进行中 · M0–M2 完成）

> 索引：[v8.0/README.md](v8.0/README.md)。不改五大步结构；不修改 `games/`；`steps/README` 仅增 ase 指针；实现在 `libs/`；对照 `thirdparty/aseprite` 只读分层与公开 [CLI 文档](https://www.aseprite.org/docs/cli/)。

- **一句话**：用 Rust 在 `step-300` 旁建设无头、与 Aseprite 文档/批处理语义对齐的能力；**禁止**复制 EULA 覆盖的源码/资源；复用已有 `pxl-core`/`pxl-ops`/`pxl-export`/`pxl-cli`，缺口见各分册对照表。
- 入口：[00-总览与架构.md](v8.0/00-总览与架构.md)、[08-CLI命令面与批处理.md](v8.0/08-CLI命令面与批处理.md)、[09-与steps集成与迁移.md](v8.0/09-与steps集成与迁移.md)；证据 [sources.md](v8.0/sources.md)。
- 状态（2026-11-02）：M0–M8 **已完成**（M7 诚实限制：顺序 argv/`--play-subtags` 为子集）；M9 **部分完成**（Rhai/`ase script`；shell/MCP 后续）。证据见 [v8.0/STATUS.md](v8.0/STATUS.md)。

## v4.0 质量与稳定性基础

**step-100 可执行设计、step-200 可信规格、step-300 稳定构建**：

- [v4.0 总览](v4.0/README.md)：目标、不变决策、M0–M6 与本次验证边界。
- [当前状态审查](v4.0/current-state-audit.md)：契约漂移、假 PASS、strict 漏口、examples 覆盖与稳定性证据。
- [实施计划](v4.0/implementation-plan.md)：契约、门禁、依赖闭包、动画/多尺寸、真实模型实验的具体任务。
- [稳定性与验收](v4.0/stability-and-acceptance.md)：五类稳定性、测试矩阵、核心样本、重复实验和人工视觉标准。

## v3.0 实施基础

多尺寸档位（32/64/128/256）、SVG → 像素矩阵和五步流水线主体已经落地；其历史状态见下方 v3.0 索引。v4.0 复核发现部分旧文档中的能力、定点实现与验收描述已和当前代码不完全一致，应以当前代码和 v4.0 审查证据为准。

## 版本索引

### v8.0：Aseprite→Rust CLI（工程部分落地）

主题：clean-room 对齐 Aseprite 无头文档模型 / 颜色 / 像素 op / 滤镜 / 渲染导出 / IO / 时间轴 / CLI，并挂入 step-300。详见 [v8.0/README.md](v8.0/README.md)。

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [README.md](v8.0/README.md) | 索引 | 已更新 | 里程碑状态表 |
| [STATUS.md](v8.0/STATUS.md) | 状态 | 已完成 | 分册×状态×证据 |
| [00–10 分册](v8.0/README.md) | 计划+标注 | 部分完成 | 文首状态已同步 |
| [sources.md](v8.0/sources.md) | 证据 | 已完成 | 本地路径 + 公开 CLI/规格 URL |

### v7.0：NES / FC 素材模型（调研落成）

主题：硬件拼砖素材如何做，以及如何变成 PixelGames 可校验资产档。详见 [v7.0/README.md](v7.0/README.md)。

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [NES素材实现调研与落地计划.md](v7.0/NES素材实现调研与落地计划.md) | 调研+落地计划 | N0 已落成；N1–N4 未开始 | 硬件模型、对照 v6/todo 队列、验收与非目标 |
| [sources.md](v7.0/sources.md) | 链接清单 | 已完成 | NESDev / metatile / makechr / NAW 等 |

### v1.0：CLI 与坦克演示阶段（已完成，归档）


对应路线图 M0/M1。详见 [v1.0/README.md](v1.0/README.md)。

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [roadmap.md](v1.0/roadmap.md) | 路线图 | v1 阶段完成 | M0/M1 落地；M2+ 在 v2.0 展开 |
| [architecture.md](v1.0/architecture.md) | 设计 | 已实现 | crates 布局与第三方许可约束；`pxl-wasm` 仍 deferred |
| [asset-pipeline.md](v1.0/asset-pipeline.md) | 设计 | 已实现 | 数据模型、op DSL、recipe、parts 均落地 |
| [cli-design.md](v1.0/cli-design.md) | 设计 | 已实现 | 全部子命令落地；`pxl mcp`（M2）未实现 |
| [miniagent-integration.md](v1.0/miniagent-integration.md) | 设计+记录 | 方案 A 已落地 | bash + skill 已通；方案 B `pxl mcp` 未做 |
| [game-dev-pipeline-harness-plan.md](v1.0/game-dev-pipeline-harness-plan.md) | 调研 | 已完成 | 背景调研稿，归档 |
| [reference-art-analysis.md](v1.0/reference-art-analysis.md) | 分析 | 已完成 | 参考素材盘点与可行性分档，归档 |
| [pxl-quality-analysis.md](v1.0/pxl-quality-analysis.md) | 分析 | 已完成 | R1–R10 根因分析，归档 |

### v2.0：Harness 与素材质量阶段（部分归档）

对应 harness 设计/能力底座、素材质量反馈闭环与 ASCII 布局输入。详见 [v2.0/README.md](v2.0/README.md)。

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [pixelgames-harness-design-plan.md](v2.0/pixelgames-harness-design-plan.md) | 计划 | H0–H2 已实现 | H3 部分（预算持久化已做，跨进程 resume 另立项）；H4/H5 未开始 |
| [harness-asset-feedback-repair-plan.md](v2.0/harness-asset-feedback-repair-plan.md) | 计划 | P0-A~D 已实施 | 提交 `23d05c3`；P1-A 部分（static 动画已落地，effect policy 未做）；P1-B 部分受限 schema 工具已开 |
| [harness-v3-agent-capability-plan.md](v2.0/harness-v3-agent-capability-plan.md) | 计划 | 已全部实施 | V3-A/B/C/D 完成（tools 模块化 / MCP / Goal / 并行+压缩+`agent` CLI） |
| [harness-v4-battle-city-asset-quality-plan.md](v2.0/harness-v4-battle-city-asset-quality-plan.md) | 计划 | 待实施 | 交付证据、需求契约、视觉审阅、受控修复、资源成本（V4-A~E） |
| [crates-module-split-plan.md](v2.0/crates-module-split-plan.md) | 计划 | 已完成 | 6 个长文件全部拆分，crates 内所有 .rs < 600 行 |
| [pxl-quality-improvement-plan.md](v2.0/pxl-quality-improvement-plan.md) | 计划 | Q2 完成 | Q2.1–Q2.6 全部落地；Q0/Q1/Q3–Q6 未实施 |
| [pxl-ascii-layout-plan.md](v2.0/pxl-ascii-layout-plan.md) | 计划 | 已实现 | ASCII 布局 → `*.ascii.json` → `PxlDoc` → PNG/GIF；A1–A5 已落地，P2 项未做 |

### v3.0：多尺寸档位与 SVG 输入阶段（进行中）

对应多尺寸档位（32/64/128/256）、ASCII rows/rle/tiles 编码、`pxl-svg` 栅格化与 `*.sprite.json` v3 统一入口。

| 文档 | 类型 | 状态 | 说明 |
|---|---|---|---|
| [pxl-native-resolution-quality-plan.md](v3.0/pxl-native-resolution-quality-plan.md) | 计划 | 已实现 | 原生尺寸 ≠ 展示倍率；`pxl enhance`/`pixelize` 落地 |
| [pxl-pixelart-pipeline-v2-plan.md](v3.0/pxl-pixelart-pipeline-v2-plan.md) | 计划 | 已实现 | MMPX/RotSprite/量化抖动/AA/finish 管线落地 |
| [pxl-multi-size-ascii-svg-plan.md](v3.0/pxl-multi-size-ascii-svg-plan.md) | 计划 | M0–M5 已实现 | 档位底座 / ASCII 32–256 / `pxl-svg` / `sprite build`；M5 示例回归完成（`examples/v3` 12 个 Metal Slug 风格样本，2026-10-26） |
| [pxl-thirdparty-algorithm-benchmark-plan.md](v3.0/pxl-thirdparty-algorithm-benchmark-plan.md) | 测绘+计划 | 待实施 | 四项目 110 项算法测绘与 `crates` 逐项对比；6 项真实缺口（选区/填充、旋转后处理、抖动矩阵、调色板排序、GIF 帧差、图集切分）、8 类明确不做；P0–P3 排期与验收 |
| [pxl-step-pipeline-plan.md](v3.0/pxl-step-pipeline-plan.md) | 架构重组 | **M0–M6 全部实施**（2026-10-27） | 顶层 5 个大 step（100 design / 200 art-spec / 300 asset-build=crates 整体 / 400 package / 500 engine），小步骤全部是子命令不拆目录；LLM 只在 100/200，step-200 禁止构建；含四份开源管线解剖与 30 条引用指引。M0–M6 完成：`pxl` 退役为 `step-300`、harness 移出 workspace、全链 e2e 1.9s、53 套件 556 测试全绿、battle-city 真实数据 coverage 13/13 + engine check 5/5。历史遗留：harness 物理目录待用户 `rm -rf harness`；step-500 build/playtest 已由 v5.0 实装 |

状态图例：**已实现/已完成**（代码或产物可核实）｜**部分**（列出已做/未做）｜**未开始**｜**归档**（历史输入，不再更新）。

## 下一步候选（按现有计划优先级）

0. **v8.0**：M0–M8 主线已完成；M9 脚本部分完成（`--shell`/MCP 后续）。见 [STATUS.md](v8.0/STATUS.md)。
1. **v7.0 NES 约束资产**：推进 N1（4 色砖 + palette 校验/导出）与 N3（pac-man / galaga 或 battle-city 试点）；N2 metatile 预览与 N4↔V6-B 门禁随后。见 [v7.0 索引](v7.0/README.md)。
2. **v6.0 剩余工作**：通用美术流程需补真实两名独立视觉评审、真实模型配对 A/B 与保留集人工结论；RSI 制作人闭环（[rsi-producer-evolution-plan.md](v6.0/rsi-producer-evolution-plan.md)）和美术光照/材质主计划（[美术光照与引擎材质改进计划.md](v6.0/美术光照与引擎材质改进计划.md)）仍按各自范围实施。
3. ~~v3.0 M5 收尾~~ 已完成（2026-10-26）：`examples/v3` 12 个 Metal Slug 风格样本全部通过（QA 零警告、视觉核验、性能预算达标）。遗留：删 `ascii refine`/`shapes refine`（存量素材迁移完再删）、可选 `review/v5` 对照。
4. 提交工作区中的 v3.0 实现（多尺寸档位 + `pxl-svg` + `pxl-art` 管线 + `plan/v3.0` + `examples/v3`）。
5. **算法缺口 P0**（`pxl-thirdparty-algorithm-benchmark-plan.md`）：选区与扫描线填充（解锁全部局部编辑）→ 旋转后处理三件套 → 抖动矩阵 2/8/16 + 蓝噪声 → 调色板排序。
6. `harness-asset-feedback-repair-plan.md` 的 P1 收尾：effect/one-shot QA policy、能力元数据补全（P1-B 剩余）。
7. `pxl mcp`（miniagent-integration 方案 B / cli-design M2）——V3-B 的 MCP 客户端已就绪，只差 server 端。
8. H3 剩余（缓存失效传播、试运行评估）与 H4 Godot spike（harness-design-plan）。

## 已知边界

- `thirdparty/` 只读参考：miniagent（无 Cargo.toml，结构移植来源）、Pixelorama、piskel、Sprite-Generator（无许可，不复制代码）；**Aseprite** 树见 `thirdparty/aseprite/`（主体 EULA，v8.0 clean-room 对齐，不复制源码/资源进产品）。
- 未验证项沿各文档标注【未验证】/`NOT_ASSESSED`，不在此重复。