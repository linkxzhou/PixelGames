# PixelGames 游戏开发 Harness 工程方案（通用流水线 + Agent 驱动）

> 日期：2026-10-21（UTC+8）｜作者：Grok Bot（调研稿，未改动仓库）
> 目标：设计一套与品类无关、可由 miniagent 端到端驱动的像素游戏开发流水线（Harness），并映射到 PixelGames（pxl CLI / pxl-core/gen/ops/qa/format/export/cli、plan/、review/）。

## 0. 结论速览
1. 业界成熟流程几乎都收敛为：**概念/GDD → 原型/灰盒 → 核心循环 → 内容/资产管线 → 关卡 → 系统(存档/UI/音频/输入) → QA/试玩/自动化 → 性能 → 构建/发布 → 运营**。
2. 让 Agent 可靠驱动的关键不是"更聪明的生成"，而是**一切皆数据 + 确定性 + 可验证**：规格(spec)用 TOML/JSON 表达；生成带 seed；每阶段都有机器可判定的"门禁(gate)"——schema 校验、golden 哈希、headless 回放、截图 diff、VLM 评审。
3. 参考对象：game-ci / godot-ci（CI 构建测试）、Aseprite/LDtk/Tiled（资产与关卡数据化导出）、Gut（引擎内单测）、Gymnasium（step/reset 式可控环境 → 回放与 bot 试玩）、Voyager/ChatDev/godot-mcp（Agent 迭代循环与工具接口）、butler（发布）。
4. PixelGames 方案：`pxl` 作为"资产与规格编译器"，新增 `pxl-sim`(headless 确定性运行时)与 `pxl-harness`(阶段编排/门禁)，`review/` 作为每轮迭代的证据归档，miniagent 只通过 CLI + JSON 报告与系统交互。
5. 路线：M0 坦克 demo 打通闭环 → M1 Celeste-like 微平台跳跃(输入/物理/回放) → M2 Stardew-like 农场切片(系统/存档/时间/经济) → M3 其余品类模板化。

## 1. 调研：开源项目与资源（星数为 2026-10-21 GitHub API 实测）
| 类别 | 项目 | ★ | 对 Harness 的启示 |
|---|---|---|---|
| 引擎 | godotengine/godot | 118.8k | 场景/资源为文本格式(.tscn/.tres)，可 diff、可被 Agent 编辑；支持 `--headless` |
| 引擎 | bevyengine/bevy | 49.2k | Rust ECS，与 PixelGames 技术栈同源；App 可无窗口运行做测试 |
| 引擎 | phaserjs/phaser | 40.5k | Web 2D，WASM/浏览器发布参考 |
| 引擎 | raysan5/raylib | 35.3k | 极简 API，适合 Agent 生成代码 |
| 引擎 | kitao/pyxel | 18.4k | 复古像素约束(调色板/分辨率) → "约束即规格" |
| 引擎 | love2d/love | 8.9k | 轻量 Lua，jam 常用 |
| 引擎 | defold/defold | 6.4k | 内置构建管线 bob.jar，一键多平台 |
| 引擎 | nesbox/TIC-80 | 6.2k | fantasy console：单文件卡带=完整游戏包，利于版本化 |
| 引擎 | cocos/cocos-engine | 9.9k | 资源管线/热更新参考 |
| 模板 | TheBevyFlock/bevy_new_2d | 492 | Bevy 2D 起步模板：屏幕状态机、资产加载、CI、itch 发布 workflow |
| 模板/CI | bevyengine/bevy_github_ci_template | 262 | Bevy 官方 CI：fmt/clippy/test + 多平台 release |
| 模板 | bitbrain/godot-gamejam | 813 | Jam 模板：菜单/设置/存档/本地化/CI 开箱即用 |
| 示例 | godotengine/godot-demo-projects | 9.7k | 按品类拆分的最小可玩样例 → "品类模板库"思路 |
| CI | game-ci/unity-builder | 1.1k | 容器化引擎构建；矩阵多平台 |
| CI | game-ci/unity-test-runner | 267 | 编辑器/运行时测试 + 报告产物 |
| CI | abarichello/godot-ci | 1.1k | Docker 内 headless 导出 + butler 发布 |
| 测试 | bitwes/Gut | 2.8k | 引擎内单元/集成测试，支持 CLI 运行 |
| 资产 | aseprite/aseprite | 40.3k | 像素编辑器；CLI `--batch` 导出 sheet+JSON、Lua 脚本 |
| 资产 | NiklasEi/bevy_asset_loader | 710 | 声明式资产清单 + 加载状态 |
| 关卡 | deepnight/ldtk | 4.3k | 关卡数据 JSON + 公开 schema，auto-layer 规则 |
| 关卡 | mapeditor/tiled | 13.0k | TMX/JSON 地图、对象层、命令行导出 |
| 发布 | itchio/butler | 1.0k | 增量推送 itch.io，渠道/版本号 |
| RL/仿真 | Farama-Foundation/Gymnasium | 12.7k | reset/step/obs/reward 接口 → bot 试玩、回放、指标 |
| RL/仿真 | openai/gym | 37.2k | 同上（历史版） |
| 仿真 | facebookresearch/habitat-lab | 3.2k | 大规模 headless 评测思路 |
| AI Agent | MineDojo/Voyager | 7.3k | 自动课程 + 技能库 + 环境反馈/自我验证 循环 |
| AI Agent | OpenBMB/ChatDev | 34.6k | 多角色(策划/程序/测试)瀑布式软件/小游戏生成 |
| AI Agent | Coding-Solo/godot-mcp | 6.2k | MCP 让 LLM 启动编辑器、运行项目、抓调试输出 |
| AI Agent | ee0pdt/Godot-MCP | 621 | MCP 读写场景节点/脚本 |
| 资源 | ellisonleao/magictools | 17.5k | 游戏开发工具大全（资产/音频/引擎/发布） |
| 资源 | Kavex/GameDev-Resources | 7.1k | 流程/设计/工具资源合集 |
| 资源 | utilForever/game-developer-roadmap | 5.8k | 技能与流程路线图 |
| 资源 | leereilly/games | 25.0k | 开源游戏清单，可作品类参考实现 |
| 引擎文档 | godotengine/godot-docs | 5.8k | 导出/CI/本地化(CSV/PO)最佳实践 |
| 遥测 | getsentry/sentry | 45.6k | 崩溃/错误上报，live ops 基础 |
| 其他 | mrdoob/three.js | 116.7k | Web 渲染生态参考（非核心） |

未找到/未验证：GameGPT（常见引用仓库名不存在）、gdUnit4（本次 API 查询未返回）、TexturePacker（商业闭源，无仓库），已从 URL 清单剔除。

## 2. 通用阶段分析（输入/输出/工具/Agent 自动化与验证）
| # | 阶段 | 输入 | 输出/产物 | 典型工具 | Agent 自动化 | 验证门禁 |
|---|---|---|---|---|---|---|
| 1 | 概念/GDD | 一句话 pitch、参考游戏 | `game.toml`：类型、核心动词、循环、分辨率/调色板、范围表；一页 GDD.md | GDD 模板、magictools | LLM 从 pitch 填充结构化 GDD（字段固定） | JSON Schema 校验；范围预算(实体数/关卡数)；人类一次签字 |
| 2 | 原型/灰盒 | game.toml | 灰盒场景(色块)、可控角色、相机 | 引擎模板、godot-demo-projects | 从品类模板库实例化；只用占位图 | headless 启动不崩；输入脚本能移动角色(位置断言) |
| 3 | 核心循环 | 灰盒 + 动词表 | 规则/物理参数表 `tuning.toml`、胜负条件 | Gymnasium 式 step 接口 | 参数扫描(手感曲线)、bot 跑通 | 回放 golden：固定 seed+输入序列 → 状态哈希一致；可达性/可通关 bot |
| 4 | 内容/资产管线 | 风格规格(palette, 尺寸, 动画帧) | sprite sheet + atlas JSON、tileset、字体、音效 | Aseprite CLI、pxl、bevy_asset_loader | `pxl gen` 按 seed 生成，`pxl ops` 后处理 | pxl-qa：调色板合规、尺寸、透明边、帧数；golden 像素哈希；VLM 风格一致性打分 |
| 5 | 关卡设计 | tileset + 关卡规格(难度曲线) | 关卡 JSON(LDtk/Tiled 兼容) | LDtk、Tiled | 程序化生成 + Agent 修补 | 路径可达(A*/bot)、跳跃可行性、难度指标在区间、截图 diff |
| 6 | 系统 | GDD 系统清单 | 存档、UI/HUD、音频、输入映射、本地化表 | godot-gamejam 模板、PO/CSV | 系统按"模块+契约测试"拼装 | 存档 round-trip、UI 截图 golden、多语言长度溢出检测、按键重映射测试 |
| 7 | QA/试玩/自动化 | 构建产物 | 测试报告、回放文件、bug 列表 | Gut、game-ci test-runner、Gymnasium | 随机/搜索 bot 模糊测试；VLM 看截图找视觉 bug | 崩溃=0；软锁检测(状态长时不变)；回归 replay 全绿 |
| 8 | 性能 | 构建 + 压测场景 | 帧时间/内存/WASM 体积报告 | profiler、wasm-opt | 自动压测 N 实体 | 预算门禁：60fps@目标机、wasm < X MB、加载 < Y s |
| 9 | 构建/发布 | 通过门禁的 commit | web/桌面包、版本号、changelog | game-ci、godot-ci、butler | CI 矩阵构建、butler push | 冒烟测试发布包；签名/校验和 |
| 10 | 运营 | 线上版本 | 遥测、崩溃、反馈 | Sentry、自建事件 | 聚合反馈 → 生成新 plan/ 任务 | 错误率阈值、留存/通关漏斗 |

### 共性原则
- **Specs as data**：每阶段的输入输出都是可 schema 校验的文件，Agent 只改数据，不改"魔法"。
- **确定性**：固定时间步、PRNG seed 全局注入、禁止依赖墙钟；同输入 → 同输出（像素/状态哈希）。
- **分层验证**：L0 schema → L1 单元/属性测试 → L2 golden(哈希/截图 diff) → L3 headless 回放/bot → L4 VLM 评审 → L5 人类试玩。越低层越便宜，Agent 每次改动必须先过低层。
- **证据归档**：每轮迭代产出报告(JSON+截图+回放)，供人/Agent 复盘（Voyager 式"技能库"沉淀）。

## 3. 映射到 PixelGames
| 阶段 | PixelGames 现有/新增 | 说明 |
|---|---|---|
| GDD/规格 | `pxl-format`（扩展 game.toml / style.toml / level schema） | 统一 schema + `pxl validate` |
| 资产 | `pxl-gen`、`pxl-ops`、`pxl-export` | seed 化生成；导出 atlas JSON（兼容 Aseprite JSON 格式便于互通） |
| 资产 QA | `pxl-qa` | 调色板/尺寸/轮廓/动画连续性检查 + golden 哈希 + 可选 VLM |
| 关卡 | 新增 `pxl gen level`（导出 LDtk 兼容 JSON） | 可达性/难度检查放入 pxl-qa |
| 运行时 | 新增 `pxl-sim`（Rust 核心，WASM + native headless） | 固定步长、输入回放、状态哈希、截图帧导出 |
| 编排 | 新增 `pxl-harness`（或 `pxl run-stage`） | 读取 pipeline.toml，按阶段执行并输出 gate 报告 |
| 证据 | `review/<milestone>/<iter>/` | report.json、截图、diff、回放、VLM 评语 |
| 计划 | `plan/` | 里程碑与任务（Agent 从 gate 失败生成下一步任务） |
| 驱动者 | miniagent | 只调用 CLI、读 JSON；禁止绕过门禁 |

## 4. Harness 架构
```
            ┌──────────── miniagent (规划/编码/修复) ────────────┐
            │  读 plan/ → 选任务 → 改 spec/代码 → 调 pxl harness  │
            └───────────────┬───────────────────────▲───────────┘
                            │ CLI                    │ gate report JSON
   ┌────────────────────────▼────────────────────────┴───────────┐
   │ pxl-harness: pipeline.toml (stages, deps, gates, budgets)     │
   │   stage runners ── pxl gen/ops/export ── pxl-sim ── pxl-qa    │
   │   gates: schema | unit | golden-hash | replay | screenshot-   │
   │          diff | bot-playthrough | perf-budget | VLM-judge     │
   └──────────────┬───────────────────────────────┬──────────────┘
                  │ artifacts (content-addressed)  │ evidence
             build/cache/<hash>                review/<ms>/<iter>/
```
关键设计：
1. **pipeline.toml**：声明阶段 DAG、每阶段输入文件 glob、输出、gate 列表与阈值；增量执行（输入哈希未变则跳过）。
2. **统一报告格式** `gate-report.json`：`{stage, gate, status, metrics, artifacts[], diff[], hint}`，`hint` 给 Agent 可操作的修复线索。
3. **pxl-sim 契约**：`reset(seed, level) / step(input) -> state / render() -> rgba / hash()`（Gymnasium 风格），回放文件 = seed + 输入序列 + 期望哈希检查点。
4. **Golden 更新策略**：golden 变化必须显式 `--bless`，并在 review/ 留下前后对比图，防止 Agent "改测试让它过"。
5. **VLM Judge**：只作为 L4 软门禁（打分+理由，固定 rubric：可读性、风格一致、调色板、动画流畅），与硬门禁分离；记录模型/提示版本保证可复现。
6. **Bot 试玩**：品类专属策略（平台跳跃：搜索式输入规划；农场：脚本化日程），输出通关率、卡死检测、关键路径时长。
7. **品类模板库**：`templates/<genre>/`（参考 godot-demo-projects、bevy_new_2d），包含最小 spec、系统清单与默认 gate。
8. **CI**：GitHub Actions 复用同一 harness（参考 bevy_github_ci_template、godot-ci）；发布阶段 wasm 构建 + butler 推 itch.io。

## 5. 分阶段路线图
| 里程碑 | 范围 | 交付 | 退出标准 |
|---|---|---|---|
| M0 坦克 demo（1–2 周） | 打通闭环 | pipeline.toml v0、gate-report schema、pxl-sim 最小版、坦克 sprite 生成+QA、输入回放 | 同 seed 两次运行哈希一致；3 条回放 golden 绿；review/M0 有完整证据；CI 跑通 |
| M1 Celeste-like 微平台跳跃（2–3 周） | 手感与关卡 | 物理 tuning.toml、5–10 个单屏关卡(LDtk 兼容)、bot 可行性检查、截图 diff、wasm 发布到 itch(草稿) | 所有关卡 bot 可通关；死亡/重生/冲刺回放稳定；60fps；VLM 风格分 ≥ 阈值 |
| M2 Stardew-like 农场切片（3–4 周） | 系统广度 | 时间/日程、作物状态机、背包/经济、存档、UI、本地化(中/英)、音频 | 存档 round-trip；7 天模拟无软锁；经济指标在区间；UI golden；文本溢出 0 |
| M3 品类扩展 | Terraria/Undertale/Dead Cells/Octopath | 品类模板 + 专属 gate（程序化世界、对话/战斗、roguelite 种子、HD-2D 光照） | 每品类一个可玩切片 + 全绿 harness |
| M4 Live ops | 遥测与反馈闭环 | 事件埋点、崩溃上报(Sentry)、反馈 → plan/ 任务自动生成 | 发布后数据自动进入 review/ |

近期 TODO（M0）：
1. 定义 `pxl-format` 中 game/style/level/replay schema；`pxl validate`。
2. 实现 `pxl-sim` 固定步长 + 状态哈希 + 帧导出（native headless 与 wasm 共用核心）。
3. `pxl-harness run --stage all --report review/M0/iter-N/`。
4. 给 miniagent 写操作守则：先 L0–L3 再提交；禁止无 `--bless` 改 golden；每轮写 review 摘要。

## 6. 风险与对策
- 非确定性（浮点/HashMap 迭代顺序/时间）→ 定点或固定步长、有序容器、seed 注入，CI 中双跑比对。
- VLM 评分漂移 → 固定版本+rubric、只作软门禁、留人类抽检。
- Agent 刷门禁 → golden 需 bless、gate 配置受保护、review 中可审计。
- 范围膨胀 → GDD 范围预算字段由 harness 强制。

## 7. Claude 生态的游戏开发 Skills / 工作流调研与 PixelGames 适配

### 7.1 调研结果（★ 为 2026-10-21 GitHub API 实测）
| 类别 | 项目 | ★ | 要点 |
|---|---|---|---|
| 官方 Skills | anthropics/skills | 182.7k | SKILL.md 规范样例。与游戏相关的有 `algorithmic-art`（p5.js 生成艺术，seed 化）、`canvas-design`、`web-artifacts-builder`（单文件 HTML/React 产物）、`webapp-testing`（Playwright 跑页面、截图、读 console，可直接借用于 web 游戏冒烟/截图测试）、`slack-gif-creator`（帧动画约束：尺寸、帧数、体积校验）、`skill-creator`、`mcp-builder`。官方没有专门的游戏 skill |
| 官方 | anthropics/claude-code | 152.7k | 提供 CLAUDE.md、`.claude/agents`（子代理）、`.claude/commands`（slash 命令）、hooks（PreToolUse/PostToolUse/Stop 等）、plugins 和 marketplace |
| 官方 | anthropics/claude-cookbooks | 53.6k | Agent 模式与 SDK 示例 |
| 游戏工作室模板 | Donchitos/Claude-Code-Game-Studios | 27.0k | 自述 49 个角色 agent、72 个工作流 skill，按真实工作室层级组织。`.claude/` 下有 agents、skills、hooks、rules、scripts 和 settings.json。agent 包括 creative-director、game-designer、level-designer、art-director、technical-artist、gameplay/engine/ui-programmer、qa-lead/qa-tester、performance-analyst、release-manager、producer 等；skill 包括 brainstorm、quick-design、prototype、art-bible、asset-spec、asset-audit、balance-check、design-review、gate-check、milestone-review、playtest-report、qa-plan、regression-suite、perf-profile、launch-checklist、patch-notes、localize 等。**与本方案的"阶段 + 门禁"最接近** |
| 游戏 Skill 合集 | gamedev-skills/awesome-gamedev-agent-skills | 2.3k | 74 个游戏开发 skill，覆盖 Godot、Unity、Unreal、Phaser、PixiJS 等 |
| 资产 | aldegad/sprite-gen | 3.0k | 2D 精灵和动画图集生成管线（按组件逐行拼接），可对照 pxl-gen |
| 改模 | rehan-remade/universal-modder | 8.6k | Claude Code 用的 skill、工具和 fal MCP，用于给游戏做 mod |
| 叙事 | zenstory-ai/novel-to-game | 907 | 依据原著改编小说为游戏的 skill 套件（中文） |
| 品类 skill | brockmartin/roblox-game-skill | 258 | 单一平台的完整 skill 示例 |
| Godot | Randroids-Dojo/Godot-Claude-Skills | 45 | Godot 用 Claude Skills（体量小） |
| Blender | majidmanzarpour/blender-game-skills | 152 | Blender 游戏资产 skill，从概念图做到绑定 |
| MCP | ahujasid/mcp-for-blender（原 blender-mcp） | 31.9k | Claude 通过 MCP 操作 Blender 建模 |
| MCP | CoplayDev/unity-mcp | 15.3k | Claude/Cursor 操作 Unity 编辑器 |
| MCP | IvanMurzak/Unity-MCP | 4.5k | 另一套 Unity MCP 实现 |
| MCP | Glade-tool/glade-mcp | 214 | 把 MCP 客户端接到 Unity/Godot |
| MCP | Coding-Solo/godot-mcp（见上文） | 6.2k | 运行 Godot 项目并抓取调试输出 |
| 通用 Skill 清单 | ComposioHQ/awesome-claude-skills | 77.6k | Skill 大全 |
| 通用 Skill 清单 | travisvn/awesome-claude-skills | 15.5k | Skill 精选 |
| Claude Code 清单 | hesreallyhim/awesome-claude-code | 56.2k | 命令、hooks、CLAUDE.md 示例 |
| 子代理 | VoltAgent/awesome-claude-code-subagents | 25.9k | 按角色划分的 subagent 库 |
| 子代理/插件 | wshobson/agents | 41.4k | 插件化的 agent、命令和 skill 集合 |
| 方法论 | obra/superpowers | 302.9k | 工程纪律 skill：头脑风暴 → 写计划 → TDD 执行 → 验证后再宣布完成，可直接借鉴为 harness 的纪律层 |
| 其他生态 | PatrickJS/awesome-cursorrules | 41.0k | Cursor rules 合集，含游戏引擎规则 |
| 其他生态 | agentsmd/agents.md | 25.0k | AGENTS.md 开放格式（Codex、Cursor 等通用） |
| 其他生态 | openai/codex | 130.3k | Codex CLI，读取 AGENTS.md |
| 其他生态 | OpenHands/OpenHands | 91.5k | 开源编码 Agent 平台 |

未找到或未收录：alexmeckes/awesome-gamedev-claude、justinpbarnett/unity-mcp（API 未返回）。

### 7.2 可复用模式
1. **Skill 结构**：`skills/<name>/SKILL.md`。frontmatter 只写 `name` 和 `description`，description 就是触发条件，按需加载以节省上下文。正文写步骤、检查表和禁止事项，脚本和模板放在同目录（`scripts/`、`templates/`），用"渐进式披露"控制篇幅。
2. **角色子代理**：总监/制作人负责编排，下设策划（GDD、平衡）、美术（art-bible、资产规格与审计）、程序（玩法/引擎/UI）、QA（测试计划、回归、试玩报告）、性能、发布。子代理上下文相互隔离，只交换文件产物。
3. **阶段门禁型 skill**：如 `gate-check`、`milestone-review`、`project-stage-detect`。Agent 先判断项目处在哪个阶段，再跑该阶段的检查表，与本方案的 pipeline.toml 门禁一一对应。
4. **plan → implement → playtest 循环**：用 superpowers 的计划/TDD/"拿出证据才算完成"纪律，再接 webapp-testing 或引擎 MCP 实际运行、截图、读日志，最后由 playtest-report 输出问题并回写 plan。
5. **Hooks 自动化**：PostToolUse 在编辑后跑 fmt 和快速测试；Stop hook 强制跑门禁，不过就不允许结束；PreToolUse 拦截对 golden 和门禁配置的修改。
6. **记忆文件**：CLAUDE.md 和 AGENTS.md 写项目约定（seed、调色板、命令、禁止事项），rules 按目录作用域生效。
7. **引擎 MCP**：引擎操作做成工具，让 Agent"看得见、跑得起"。PixelGames 可以把 pxl 本身包成 MCP 或 CLI 工具。

### 7.3 PixelGames 适配：`.miniagent/skills` 建议集
目录约定：`.miniagent/skills/<name>/SKILL.md` + `scripts/`；可选镜像到 `.claude/skills/`（同为 SKILL.md 格式，两边共享）。根目录放 `AGENTS.md`（通用约定），`CLAUDE.md` 只写一行引用它。

| Skill | 触发（description） | 职责 / 产物 | 门禁 |
|---|---|---|---|
| `pxl-stage-detect` | 开始任何任务时，或"现在进度到哪了" | 读 plan/、review/ 和 pipeline.toml，判断当前里程碑和阶段，输出下一步 | — |
| `game-concept-gdd` | 新游戏、新 pitch、改核心玩法 | 生成或修订 game.toml 与 GDD.md（范围预算） | schema 校验 + 人工签字 |
| `genre-template` | "做一个 X-like" | 从 templates/<genre>/ 实例化 spec 和默认门禁 | validate |
| `art-bible` | 定义或修改风格、调色板 | style.toml（调色板、网格、轮廓、动画帧规范） | schema |
| `pxl-asset-gen` | 需要精灵、tileset 或图标 | `pxl gen/ops/export`，按 seed 生成并导出 atlas | 交给 pxl-asset-qa |
| `pxl-asset-qa` | 资产生成或修改后 | pxl-qa 硬检查 + golden 哈希 + VLM 风格打分 | L0–L2，L4 软门禁 |
| `level-design` | 新关卡、调难度 | 生成或修补关卡 JSON（LDtk 兼容） | 可达性检查 + bot 通关 |
| `core-loop-tuning` | 手感、平衡、数值 | 改 tuning.toml，做参数扫描并记录曲线 | replay golden、指标区间 |
| `systems-impl` | 存档、UI、音频、输入、本地化 | 按"模块 + 契约测试"实现 | round-trip、UI golden、文本溢出 |
| `replay-record` | 修完 bug 或新功能完成时 | 录 seed + 输入序列 + 哈希检查点，存入回归集 | — |
| `playtest-bot` | 每轮迭代结束时 | pxl-sim 无窗口跑 bot，检测软锁和通关率 | L3 |
| `vlm-judge` | 视觉相关改动 | 用固定 rubric 评截图，产出评分和理由 | L4 软门禁 |
| `perf-budget` | 里程碑前，或 wasm 体积变化时 | 帧时间、内存、wasm 体积报告 | 预算阈值 |
| `gate-check` | 声称"完成"之前（强制） | `pxl harness run --report review/<ms>/<iter>/` | 全部硬门禁 |
| `review-archive` | 每轮结束时 | 写 review 摘要（改动、证据、失败项），失败项回写 plan/ | — |
| `golden-bless` | 确实需要更新基准时 | 生成前后对比，要求说明理由，必须由人确认 | 人工 |
| `release-itch` | 里程碑通过后 | wasm 构建、冒烟测试、butler push（草稿渠道） | 冒烟测试 + 人工批准 |

**角色子代理**（`.miniagent/agents/` 或 `.claude/agents/`）一共 6 个：producer（编排与阶段判断）、designer（GDD、关卡、数值）、artist（art-bible、资产生成与 QA）、programmer（pxl-sim 与系统）、qa（回放、bot、VLM、门禁）、release。每个子代理只开放与职责对应的 skill 和工具。

**Hooks**
- 编辑 `crates/**` 后：`cargo fmt && cargo test -p <crate>`。
- 编辑 `assets/specs/**` 后：`pxl validate`。
- 拦截写入 `tests/golden/**` 和 `pipeline.toml` 的操作，必须走 golden-bless。
- 结束回合前跑 `gate-check` 的快速档，不通过就不允许宣布完成。

**落地顺序**：在 M0 落地 stage-detect、asset-gen、asset-qa、replay-record、gate-check、review-archive；M1 加 level-design、core-loop-tuning、playtest-bot、vlm-judge；M2 加 systems-impl、perf-budget、release-itch 和 genre-template。

## 8. 参考 Git URL 清单（★ 为 2026-10-21 实测）
- https://github.com/godotengine/godot — ★118,792 — Godot 引擎，文本场景格式与 headless
- https://github.com/godotengine/godot-demo-projects — ★9,674 — 按品类的最小示例集
- https://github.com/godotengine/godot-docs — ★5,839 — 导出/本地化/CI 文档
- https://github.com/bevyengine/bevy — ★49,181 — Rust ECS 引擎，与本项目同栈
- https://github.com/bevyengine/bevy_github_ci_template — ★262 — Bevy 官方 CI/发布模板
- https://github.com/TheBevyFlock/bevy_new_2d — ★492 — Bevy 2D 游戏起步模板
- https://github.com/NiklasEi/bevy_asset_loader — ★710 — 声明式资产加载
- https://github.com/phaserjs/phaser — ★40,525 — Web 2D 引擎
- https://github.com/raysan5/raylib — ★35,349 — 极简游戏库
- https://github.com/kitao/pyxel — ★18,430 — 复古像素引擎（约束即规格）
- https://github.com/love2d/love — ★8,851 — LÖVE 2D 框架
- https://github.com/defold/defold — ★6,408 — Defold 引擎与构建管线
- https://github.com/nesbox/TIC-80 — ★6,167 — Fantasy console（单文件卡带）
- https://github.com/cocos/cocos-engine — ★9,873 — Cocos 引擎资源管线
- https://github.com/mrdoob/three.js — ★116,720 — Web 3D 渲染（旁证）
- https://github.com/bitbrain/godot-gamejam — ★813 — Godot Game Jam 模板
- https://github.com/bitbrain/pandora — ★1,128 — Godot 数据/实体管理（数据驱动内容）
- https://github.com/game-ci/unity-builder — ★1,102 — 容器化引擎构建 CI
- https://github.com/game-ci/unity-test-runner — ★267 — 引擎测试 CI
- https://github.com/abarichello/godot-ci — ★1,147 — Godot headless 导出 + 发布 CI
- https://github.com/bitwes/Gut — ★2,763 — Godot 单元测试框架
- https://github.com/aseprite/aseprite — ★40,264 — 像素编辑器，CLI 批量导出
- https://github.com/deepnight/ldtk — ★4,336 — 关卡编辑器，JSON schema
- https://github.com/mapeditor/tiled — ★12,979 — 通用瓦片地图编辑器
- https://github.com/itchio/butler — ★996 — itch.io 增量发布
- https://github.com/Farama-Foundation/Gymnasium — ★12,704 — 环境 step/reset 接口标准
- https://github.com/openai/gym — ★37,237 — Gym 历史版本
- https://github.com/facebookresearch/habitat-lab — ★3,165 — 大规模 headless 仿真评测
- https://github.com/MineDojo/Voyager — ★7,275 — LLM 自动课程+技能库+自验证
- https://github.com/OpenBMB/ChatDev — ★34,560 — 多角色 Agent 软件/游戏生成
- https://github.com/Coding-Solo/godot-mcp — ★6,240 — Godot MCP（运行/调试）
- https://github.com/ee0pdt/Godot-MCP — ★621 — Godot MCP（场景/脚本编辑）
- https://github.com/ellisonleao/magictools — ★17,540 — 游戏开发工具大全
- https://github.com/Kavex/GameDev-Resources — ★7,078 — 游戏开发资源合集
- https://github.com/utilForever/game-developer-roadmap — ★5,787 — 游戏开发路线图
- https://github.com/leereilly/games — ★24,977 — 开源游戏清单
- https://github.com/getsentry/sentry — ★45,620 — 错误/崩溃遥测
- https://github.com/linkxzhou/PixelGames — ★0 — 本项目（Rust+WASM，pxl CLI）
- https://github.com/anthropics/skills — ★182,665 — 官方 Agent Skills（algorithmic-art、canvas-design、web-artifacts-builder、webapp-testing 等）
- https://github.com/anthropics/claude-code — ★152,693 — Claude Code（CLAUDE.md、子代理、命令、hooks、插件）
- https://github.com/anthropics/claude-cookbooks — ★53,644 — 官方 Agent/SDK 示例
- https://github.com/Donchitos/Claude-Code-Game-Studios — ★27,045 — 49 个 agent + 72 个 skill 的游戏工作室模板
- https://github.com/gamedev-skills/awesome-gamedev-agent-skills — ★2,306 — 74 个游戏开发 agent skill
- https://github.com/aldegad/sprite-gen — ★2,959 — 2D 精灵/动画图集生成管线
- https://github.com/rehan-remade/universal-modder — ★8,608 — Claude Code 游戏改模 skill + MCP
- https://github.com/zenstory-ai/novel-to-game — ★907 — 小说改编游戏 skill
- https://github.com/brockmartin/roblox-game-skill — ★258 — Roblox 游戏开发 skill
- https://github.com/Randroids-Dojo/Godot-Claude-Skills — ★45 — Godot 用 Claude Skills
- https://github.com/majidmanzarpour/blender-game-skills — ★152 — Blender 游戏资产 skill
- https://github.com/ahujasid/mcp-for-blender — ★31,870 — Blender MCP（原 blender-mcp）
- https://github.com/CoplayDev/unity-mcp — ★15,269 — Unity MCP
- https://github.com/IvanMurzak/Unity-MCP — ★4,512 — Unity MCP（另一实现）
- https://github.com/Glade-tool/glade-mcp — ★214 — Unity/Godot MCP 桥接
- https://github.com/ComposioHQ/awesome-claude-skills — ★77,588 — Claude Skills 大全
- https://github.com/travisvn/awesome-claude-skills — ★15,492 — Claude Skills 精选
- https://github.com/hesreallyhim/awesome-claude-code — ★56,167 — Claude Code 命令/hooks/CLAUDE.md 清单
- https://github.com/VoltAgent/awesome-claude-code-subagents — ★25,925 — 角色子代理库
- https://github.com/wshobson/agents — ★41,426 — 插件化 agent/命令/skill
- https://github.com/obra/superpowers — ★302,949 — 计划/TDD/验证纪律 skill
- https://github.com/PatrickJS/awesome-cursorrules — ★40,981 — Cursor rules 合集
- https://github.com/agentsmd/agents.md — ★25,029 — AGENTS.md 开放格式
- https://github.com/openai/codex — ★130,274 — Codex CLI（读取 AGENTS.md）
- https://github.com/OpenHands/OpenHands — ★91,472 — 开源编码 Agent 平台
