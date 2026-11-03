# v3.0 Plan v7：step 化流水线（大 step 版）

> 性质：架构重组计划，**不含实现代码**。
> 状态：待评审｜更新：2026-10-26
> 与既有 plan 的关系：不替代 `pxl-multi-size-ascii-svg-plan.md` / `pxl-pixelart-pipeline-v2-plan.md` / `pxl-thirdparty-algorithm-benchmark-plan.md`。本文件只回答"谁在什么时候做什么、产物怎么交接"。
> 输入材料：`thirdparty/{ai-game-spritesheets, Claude-Code-Game-Studios, game-creator, gamestudio-subagents}` 四套开源管线（解剖见 §2）。

---

## 0. TL;DR

### 0.1 一句话

`steps/` 顶层只有 **5 个大 step**（每个一个 bin）；所有小步骤都是大 step 内部的**子命令**，不拆目录、不拆 crate；`crates/` 原样整体嵌入 `step-300`，内部 11 个 crate 保持原名原结构。

### 0.2 流程

```
step-100-design ──→ step-200-art-spec ──→ step-300-asset-build ──→ step-400-package ──→ step-500-engine
     设计                素材规格              素材生成                交付包                引擎接入
    （LLM）              （LLM）              （= crates/）           （确定性）            （macroquad）
  init/run/gate/       run/lint/capabilities  plan/build/...         run/check/approve     check/slice/build
  approve/status                             （全部子命令）          /status
```

### 0.3 五条铁律

| # | 铁律 | 为什么 |
|---|---|---|
| 1 | **顶层只有 5 个大 step** | `steps/` 目录一眼看完整个流程；再多就回到"阶段=目录爆炸"的老问题 |
| 2 | **小步骤 = 子命令，不拆目录** | `step-100 init/run/gate/approve` 是一个 bin 的四个动作，不是四个 crate |
| 3 | **`crates/` 原样嵌入 step-300** | 11 个 crate 是同一道工序的工位；不改名、不编号、不打散，`cargo test -p pxl-core` 照旧 |
| 4 | **LLM 只在 step-100 / step-200** | 只有"写设计"和"写素材规格"需要创造性；构建必须确定性 |
| 5 | **step-200 禁止构建** | 创作与构建分离，失败归属才清楚（现状 `assets_stage.rs` 混在一起） |

### 0.4 本期范围

`step-100` ~ `step-400` 全量 + `step-500 check/slice`；`step-500 build/playtest` 只留子命令占位（对应用户"暂时先做到素材交给引擎这一步"）。

---

## 1. 制作人视角：这条线该怎么切

### 1.1 游戏制作的四个真实阶段

一个制作人按**风险**排期，不按代码模块排期：

| 制作阶段 | 要回答的问题 | 风险 | 对应大 step |
|---|---|---|---|
| **立项 + 设计（Greenlight）** | 做什么？多大范围？给谁看？ | 做错方向 | step-100 |
| **垂直切片（Vertical Slice）** | 核心循环成立吗？画质够吗？管线通吗？ | 做完才发现不好玩 | `--scope slice` 回路 + `step-500 slice` |
| **内容量产（Production）** | 按清单批量产出 | 产能、一致性、返工 | step-200 → step-400 全量 |
| **集成与打磨（Integration）** | 素材进引擎能跑吗？手感对吗？ | 接缝处崩 | step-500 |

**当前仓库的缺口**：没有"垂直切片"。`harness` 是 `design → assets 全量` 一把梭。四个开源项目里，`Claude-Code-Game-Studios` 把垂直切片做成了独立 gate（`vertical-slice-report.md:93` 的 `PROCEED / PIVOT / KILL / NOT ASSESSED`），`ai-game-spritesheets` 用 **anchor**（一张锚点图）承担同样的"先定真源再派生"职责（`prompts/03-neutral-anchor.md:3`）。本 plan 把两者合成 `step-500 slice` 子命令——它判定"切片能不能放行量产"，本质是引擎侧问题（能不能玩），所以归引擎 step，不单独占顶层目录。

### 1.2 为什么 `crates/` 是一个大 step

`crates/` 下 11 个 crate 共享同一个输入（spec/recipe）、同一个输出（PxlDoc + PNG/GIF/sheet）、同一套确定性约束：

```
pxl-core ─┬─ pxl-format ─┬─ pxl-gen ─┐
          ├─ pxl-art ────┼─ pxl-svg  ├─ pxl-ops ─┬─ pxl-engine ─┬─ pxl-wasm
          ├─ pxl-qa      └───────────┘            │              └─ pxl-cli ← bin
          └─ pxl-export ──────────────────────────┘
```

**分开就是 11 个假阶段**：`pxl-core` 没有 bin，`pxl-format` 只输出 schema，谁也不能独立"完成一件事"。

判定标准：**step 的边界落在"产物形态变化"处，不落在"代码依赖"处。**

- spec → 帧：形态变化 ✅
- 帧 → PNG：形态变化 ✅
- 帧 → 帧（放大/描边/量化）：形态不变，是工序内部精加工 ❌

所以 `crates/` 整体是 `step-300-asset-build`：对外一个 bin（`step-300`）、一组输入（`work/200-art-spec/`）、一组输出（`work/300-build/`）。内部 11 个 crate 是工位，允许随意重构，不影响上下游。

### 1.3 为什么 LLM 只占两段

四个项目共同的教训——LLM 负责"写下来"，确定性代码负责"做出来"和"验一遍"：

| 项目 | LLM 用在 | 确定性代码用在 | 分界后果 |
|---|---|---|---|
| ai-game-spritesheets | 8 个 prompt | **没有**（08 归一化全人工） | 最后一步成瓶颈（`08:81-83` 自述最耗人工） |
| Claude-Code-Game-Studios | 49 agent 写文档 | gate 检查 | 分了，但检查无执行体 |
| game-creator | agent 写码 + QA 判读 | `verify-runtime.mjs` 等 | **QA 独立、只回结构化文本**（`game-creator.md:119-133`） |
| gamestudio-subagents | 12 agent | 几乎没有 | phase 永不推进（`project_manager.py:250`） |

`pxl` 走真索引色 + 确定性路线，分界可以划得比四家都干净：

- step-100：LLM 写设计 JSON
- step-200：LLM 写 SVG / ASCII / shapes / recipe
- step-300 之后：**零 LLM、零网络、零随机**

### 1.4 为什么 brief 并入 step-100 而不是独立 step

brief 是人写的一句话/一页纸，design 是 LLM 产出的设计包——它们是**同一个"设计域"的输入和输出**。拆成两个顶层 step 的代价：两个 bin、两套目录、一条没人需要的审批链。合并后 brief 只是 `step-100 init` 子命令的输入固化，`step-100` 的完整职责是"从想法到设计包"。

这正是"大 step 内部小步骤不拆分"的第一个应用。同理：

- **slice / playtest 并入 step-500**：切片的核心问题是"能不能玩"（引擎侧问题），build/playtest 本来就是引擎构建；它们是 `step-500` 的四个子命令，不是四个 step。
- **gate / approve 并入各自的 step**：审批是阶段内部的动作，不是阶段。

---

## 2. 四个开源项目的流程解剖

> 引用均指可迁移的**流程设计**，不涉及代码复制。路径与行号已核对。

### 2.1 ai-game-spritesheets：资产内容流的范本

**形态**：纯 prompt + 参考图仓库，9 步人工管线，零代码（`README.md:37-47`）。自述定位：`README.md:35`「Image gen ≈ 20% of the work. The other 80% is the pipeline below.」

| 步骤 | 做什么 | 关键设计 |
|---|---|---|
| 01 box art | 概念图 | 只用文本，不给图参考 |
| 02 south anchor | 南向锚点 | **纯 prompt + 像素网格**；禁止用 box art 当参考（会跑偏成 painterly，`02:17-21`），并留反例 `02b-south-from-boxart-rejected.png` |
| 03 neutral anchor | 剥特效 | "**Anchors are boring on purpose**"（`03:3`）：特效会沿所有下游帧复制 |
| 04 directional | 西/北锚点 | **东向不生成，直接水平翻转**（`04:61`） |
| 05 walk | i2v | 只吃 1 张锚点；**明确禁止**再给网格图（`05:25`） |
| 06/07 attack/idle | 5×2 图集 | 固定 10 帧、固定帧序（`06:19-21`） |
| 08 normalization | 归一化 | **全部帧重贴同一画布，锁 `center_x` 与 `bottom_y`**（`08:42-46`） |

**三个可迁移判断**：

1. **锚点是单一真源，其余全部派生**。一条锚点错误放大成 ×8~12 帧 × 3 方向 × 3 动画（`06:3`）。
2. **归一化只能放最后**：要先有一个"已批准的动画表"当标尺（`08:38`），前置没有基准。
3. **别信名义网格，按 alpha 包围盒实测**（`08:17`）。`PxlDoc` 声明 16×32 不代表内容占满。

**不可迁移**：`mixels` 假像素容忍（`02:73`）、chroma 抠图、i2v 挑帧、re-roll 控制流——pxl 走真索引色 + 声明式尺寸，这些问题不存在。

→ 落到：§4.2（recipe 是锚点、特效只作 op）、§4.3（`bottom_y`/`center_x` 变 gate）、实测包围盒。

### 2.2 Claude-Code-Game-Studios：gate 与裁决的范本

**形态**：49 agent + 74 skill + 62 模板的"工作室流程规约"，7 阶段（`workflow-catalog.yaml:78-505`）。

**三个最有价值的设计**：

**① gate 三件套**（`gate-check/references/gate-*.md`）

```
Required Artifacts:   ← 可机器查（路径/glob/字段）
Quality Checks:       ← 需读内容判断
Tier reductions:      ← 哪些条目在 minimal 档可放宽
```

**② `NOT ASSESSED` 是一等裁决**（`5-verdict.md:58-65`）。优先级 `FAIL > CONCERNS > NOT ASSESSED > PASS`。触发条件很细：文件存在但全是占位、perf 报告有数字但零 profiler 数据、test runner 配了但启动失败。**"无法检查"不算通过**。

**③ tier 放宽只放宽"必须存在"，永不放宽"必须能用"**（`gate-production.md:83-86`）。

两条纪律：**永不为制造 PASS 自动补文件**（`gate-check/SKILL.md:149-152`）；**PASS 前做 Chain-of-Verification 5 问，≥2 条必须真读文件**（`5-verdict.md:118-163`）。

**垂直切片**（`vertical-slice-report.md`）：验证问题必须是"体验 + 可行性"双问（`:10-15`），`Build Velocity Log` 当产能基线（`:34-48`），四值裁决 + 每种结果独立后续（`:93,103-165`）。**建了但没人玩过 = NOT ASSESSED，永不通过**（`gate-production.md:54-57`）。

**不可迁移**：49 角色、三档 rigor × 三档 review_mode 的 81 格矩阵、`studio-tests/`（测 prompt 写法，CLI 化后作废）、"May I write this file?" 逐文件询问（换 `--overwrite`）。

→ 落到：§5.3（四值裁决）、`step-500 slice`（§4.5）、`step.json` gates 结构。

### 2.3 game-creator：断点续跑与 QA 判决的范本

**形态**：Claude Code 插件，三条并行流水线（一次性 10 步 / 无人值守 5 步 / 里程碑 3 阶段）。

**三个最有价值的设计**：

**① `progress.md` 断点续跑**（`agents/game-creator.md:181-204`）：**原 prompt 逐字置顶**（续跑不丢需求）、**每步完成强制更新**、`Known Issues` 显式记录"没做哪步"。

**② QA 独立、只回结构化文本**（`game-creator.md:119-133`）：`QA RESULT: PASS|FAIL` + 逐 phase 行 + ISSUES，**禁止返回图片**，5 phase 顺序固定、critical 早停。**产物是可机读的判决，不是描述**——这是"独立 step"的真正含义。

**③ `autofix-history.json` 让重试有记忆**（`game-creator.md:148-163`）：每轮追加 `{step, issue, fix_attempted, result}`，下轮先读它；3 次上限后**跳过继续**而不是死循环。

配套两条：**`.meta.json` 由生成脚本写，不由 agent 手写**（`scripts/meshy-generate.mjs:339-391`），契约落在代码里 agent 无法违反，且只存可再生 ID 不存过期 URL（`gigachad.meta.json:8`）；**模板自带验证脚本**（`README.md:172`），新项目天生可验证。

**不可迁移**：Phaser/Three/Vite/Playwright 全套、`window.render_game_to_text()`、Play.fun 变现。

→ 落到：§5.2（`step.json` 字段）、`attempts.jsonl`、`--dump-state`。

### 2.4 gamestudio-subagents：引擎交付契约与反面教材

**形态**：12 agent 的"工作室模板"，5 阶段。

**真正该抄的：`engine_configs/*.json` 是"引擎能力清单"，不是项目配置**（`godot_config.json:4-23`）：

| 键 | 回答什么问题 |
|---|---|
| `project_structure.source` | 素材该放哪个目录 |
| `best_practices.naming_conventions` | 文件该叫什么 |
| `required_files` | 缺什么就算没交付 |
| `platforms` | 导出目标长什么样 |

这四项正是"素材生成器 → 引擎导入器"之间最容易出错的四类信息。**做成可校验的 schema 而不是提示词，就得到跨引擎的资产交付闸门**。

第二个可抄：**handoff 三态 + 双签**（`handoff_template.md:24-27,137-146`）：`APPROVED / NEEDS REVISION / REJECTED` + Sending/Receiving/Producer 三段签名。

**三个必须避开的反面**（该仓库自身硬伤）：

1. **phase 永远停在 `Market Analysis`**：只有 `init_project.py:800` 写过一次，`project_manager.py:250` 只在 startover 重置，**没有任何代码推进 phase**；`ORCHESTRATOR: TRANSITION`（`master_orchestrator.md:374-379`）只是提示词。→ **状态必须由代码推进**。
2. **模板与脚本不对齐**：`project_config_template.json` 有 9 个顶层块，`init_project.py:785-823` 只写 6 个。→ **契约必须 `--contract` 自证**。
3. **假测试**：`test_project_workflow.py:61-74` 定义 `expected_structure` 后直接 `print PASS`，零 assert。→ **gate 必须有可复现断言**。

→ 落到：`step-500 engine.json` 四件套、`revision.manifest.json` 三态、三条反面写进铁律。

### 2.5 交叉对比矩阵

| 维度 | ai-game-spritesheets | Claude-Code-Game-Studios | game-creator | gamestudio-subagents | 本 plan |
|---|---|---|---|---|---|
| 阶段数 | 9 人工步 | 7 phase | 3/5/10（三套） | 5 phase | **5 大 step** |
| 状态推进 | 无（人工） | 提示词 + 用户点头 | `progress.md` 复选框 | **无（硬伤）** | **`step.json` 由代码写** |
| 产物契约 | prompt 内自然语言 | 7 个 `CONTRACT.md` | `.meta.json`（脚本写） | `handoff_template` | **JSON Schema + `step.json`** |
| 闸门 | 无 | gate 三件套 + 4 值 | 5 phase 验证（可跳过） | 4 个布尔位 | **三件套 + 4 值** |
| 无法评估 | 无此概念 | **NOT ASSESSED** | 无 | 无 | **采纳** |
| 重试控制 | re-roll | 不自动修复 | `autofix-history`（3 次上限） | 无 | **`attempts.jsonl`** |
| 切片 | anchor | **vertical slice** | milestone | prototype mode | **`step-500 slice`** |
| 引擎交付 | 无 | 无 | `asset-pipeline.md` 命名规则 | **`engine_configs` 四件套** | **`engine.json`** |
| 最大缺陷 | 最后一步全人工 | 复杂度 81 格矩阵 | Web 生态绑定 | 流程不可执行 | 用"5 大 step + 契约"避开 |

---

## 3. 目标形态

### 3.1 目录树

```
PixelGames/
  steps/                        # ★ 顶层只有 5 个大 step，一眼看完整个流程
    step-100-design/            #   大 step 1：设计（brief → design package，LLM）
    step-200-art-spec/          #   大 step 2：素材规格（requirements → SVG/ASCII/JSON，LLM）
    step-300-asset-build/       #   大 step 3：素材生成（= crates/ 原样整体嵌入，CLI）
    step-400-package/           #   大 step 4：交付包（build → asset-package，确定性）
    step-500-engine/            #   大 step 5：引擎接入（package → macroquad）

  libs/                         # ★ 共享库：不是流程，不进 steps/
    pxl-agent/                  #   原 harness/src/{agent,policy,config,skills,adapters/process}
    pxl-store/                  #   原 harness/src/{digest,storage,qa}
    pxl-contract/               #   新：step.json 信封 + 裁决 + 退出码 + Schema

  games/battle-city/
    brief.json                  #   唯一手写输入
    runtime/                    #   保持：macroquad 实现，本期不动
    work/                       #   step 产物（替代 revisions/ 与 runs/）

  web/                          #   保持，只改 wasm 引用路径
  assets/                       #   保持：内置调色板与模板
  vendor/miniquad               #   保持
```

**结构规则**（三条，回答"什么能出现在哪一层"）：

1. `steps/` 顶层**只允许**大 step 目录：一个大 step = 一个阶段 = 一个 bin。小步骤（子命令）、角色、模板、内部模块全部封装在大 step 内部。
2. `step-300` 内部是 `crates/` 的**原样平移**：11 个 crate 不改名、不编号、不重组。它们是工序内部的工位，不对外。
3. 被两个以上 step 依赖、且没有业务流程的代码放 `libs/`。**库永远不叫 step**。

### 3.2 每个大 step 的内部结构

**step-100-design**（单 crate，小步骤 = 子命令）

```
step-100-design/
  Cargo.toml                    # crate: step-100-design ｜ bin: step-100
  src/
    main.rs
    cmd/
      init.rs                   # 小步骤：brief → brief.snapshot + pipeline.json
      run.rs                    # 小步骤：LLM 设计循环（designer 角色）
      gate.rs                   # 小步骤：gates，零模型（CI 用）
      approve.rs                # 小步骤：审批绑定 digest
      status.rs                 # 小步骤：读 step.json 汇报
    contracts/                  # Brief / GameDesign / StyleGuide / LevelDef / AssetRequirements / AcceptancePlan
    stage/                      # 原 design_stage 主体逻辑
    review/                     # 原 review_view
  roles/                        # designer.md / designer_zh.md
  templates/                    # top_down_tank/（原 harness/templates）
  tests/
    fixtures/                   # brief.min.json / brief.full.json / w100 冻结产物
```

**step-200-art-spec**（单 crate）

```
step-200-art-spec/
  Cargo.toml                    # bin: step-200
  src/
    cmd/
      run.rs                    # 小步骤：LLM 写 spec（artist 角色），--scope all|slice
      lint.rs                   # 小步骤：本地校验（零模型零构建）
      capabilities.rs           # 小步骤：转发 step-300 list / schema
      status.rs
    spec/                       # spec-manifest 模型与读写
  roles/                        # artist.md / artist_zh.md
  tests/
    fixtures/                   # asset-requirements.json / w200 冻结产物
```

**step-300-asset-build**（= `crates/` 原样，多 crate）

```
step-300-asset-build/
  pxl-core/                     # 工位：原语（no_std）
  pxl-format/                   # 工位：契约 + JSON Schema
  pxl-gen/                      # 工位：Bollinger/形状/噪声/ASCII
  pxl-svg/                      # 工位：SVG 光栅化
  pxl-art/                      # 工位：重建/像素化/MMPX/rotSprite
  pxl-ops/                      # 工位：变换 + 动画宏
  pxl-qa/                       # 工位：profile QA
  pxl-export/                   # 工位：PNG/GIF/sheet/preview
  pxl-engine/                   # 工位：IO-free 装配
  pxl-wasm/                     # 工位：web 入口（[lib] name = "pxl_wasm" 冻结）
  pxl-cli/                      # 工位：bin step-300（+ pxl 别名），全部子命令
```

**step-400-package**（单 crate）

```
step-400-package/
  Cargo.toml                    # bin: step-400
  src/
    cmd/
      run.rs                    # 小步骤：覆盖检查 + 内容寻址 + 打包
      check.rs                  # 小步骤：重算 digest 自检
      approve.rs                # 小步骤：审批（三态）
      status.rs
    gates/                      # 原 assets_stage 的 frame_index / png_ok / rules_for_sheet
    manifest/                   # asset-manifest / coverage / asset-package 模型
  tests/
```

**step-500-engine**（单 crate）

```
step-500-engine/
  Cargo.toml                    # bin: step-500
  src/
    cmd/
      check.rs                  # 小步骤：asset-package 契约校验（本期实现）
      slice.rs                  # 小步骤：垂直切片判定 PROCEED/PIVOT/KILL/NOT ASSESSED（本期实现）
      build.rs                  # 小步骤：cargo build native/wasm（占位，后续）
      playtest.rs               # 小步骤：headless 回放（占位，后续）
  engine.json                   # 引擎能力清单：目录映射/命名/必需文件/平台（四件套）
  skills/macroquad/             # 原 harness/skills/macroquad
  tests/
```

### 3.3 step 总表

| step | bin | 职责 | LLM | 小步骤（子命令） | 输入 → 输出 |
|---|---|---|---|---|---|
| **100 design** | `step-100` | 从想法到设计包 | ✅ | `init` / `run` / `gate` / `approve` / `status` / `migrate` | `brief.json` → `work/100-design/` |
| **200 art-spec** | `step-200` | 从需求到素材规格 | ✅ | `run` / `lint` / `capabilities` / `status` | `work/100-design/` → `work/200-art-spec/` |
| **300 asset-build** | `step-300` | 从规格到素材（crates 整体） | 否 | `plan` / `build` / `compile` / `render` / `sheet` / `qa` / `gen` / `op` / `anim` / `palette` / `tile` / `inspect` / `profile` / `schema` / `list` / `enhance` / `pixelize` | `work/200-art-spec/` → `work/300-build/` |
| **400 package** | `step-400` | 从产物到交付包 | 否 | `run` / `check` / `approve` / `status` | `work/300-build/` → `work/400-package/` |
| **500 engine** | `step-500` | 从交付包到引擎 | 否 | `check` / `slice` / `build`\* / `playtest`\* | `work/400-package/` → `work/500-engine/` |

`\*` = 占位，后续实现。

### 3.4 命名规则

| 对象 | 规则 | 例 |
|---|---|---|
| 大 step 目录 | `step-<NNN>-<kebab>`，`100` 起、步长 `100` | `step-300-asset-build` |
| crate / bin | crate 同目录名；bin = `step-<NNN>` | `step-300-asset-build` / `step-300` |
| 小步骤 | 大 step 的**子命令**，不再有目录层级 | `step-300 plan` |
| 共享库 | `libs/<crate>`，永不带 step 编号 | `libs/pxl-agent` |
| 契约 id | `<domain>/v<N>` | `art-spec/v1` |

`x50` 编号（150/250/…）预留给未来插入的大 step（如 `step-150-visual-review`）。**不用于小步骤**——小步骤加子命令即可，零成本。

---

## 4. 大 step 逐个定义

### 4.1 step-100-design（设计）

**职责**：designer 角色调文本模型，从人的想法产出**纯 JSON 设计包**。不碰像素、不碰素材。

**输入**：`brief.json`（人写，唯一手写输入）

**输出**：`work/100-design/`

| 文件 | 内容 | 关键字段 |
|---|---|---|
| `brief.snapshot.json` | canonical 化 + digest | `notes` 原文逐字保留 |
| `template.json` | `top_down_tank` 模板展开 | |
| `pipeline.json` | 5 个 step 的版本 + 契约 id + `slice` 定义 | |
| `game-design.json` | 玩法、实体、胜负 | `core_loop`、`fail_condition` |
| `style-guide.json` | 尺寸档、调色板、风格 | `profile`、`palette`、`outline` |
| `levels.json` | 关卡字符矩阵、出生点 | `grid`、`spawn` |
| `asset-requirements.json` | 资产需求（step-200 唯一硬输入） | 每项 `id/kind/size/anims/acceptance[]` |
| `acceptance-plan.json` | 验收断言 | |
| `reports/design.gates.json` + `design-review.md` | gates + 人读评审 | |
| `step.json` | 信封 | |

`asset-requirements.json` 草案：

```json
{
  "schema_version": 1,
  "assets": [
    {
      "id": "player_tank",
      "kind": "shapes",
      "size": [16, 16],
      "anims": [{ "name": "move_S", "frames": 4, "fps": 8 }],
      "acceptance": [
        { "id": "silhouette.symmetric", "assert": "left_right_mirror" },
        { "id": "foot.baseline", "assert": "bottom_y_constant" }
      ]
    }
  ]
}
```

**CLI**

```
step-100 init    --brief <file> --out <dir> [--overwrite]
step-100 run     --in <dir> --out <dir> [--provider fake|openai] [--max-turns N] [--until draft|gates]
step-100 gate    --in <dir>                                    # 零模型
step-100 approve --in <dir> --digest sha256:...
step-100 status  --in <dir>
step-100 migrate --from <old project dir> --out <dir>          # M5
```

**引用**

| 设计 | 出处 | 为什么 |
|---|---|---|
| `notes` 原文逐字保留、不摘要 | game-creator `game-creator.md:181-202` | 续跑不丢需求；后续 step 都能读到原话 |
| brief + design 合并、不拆两个 step | 本 plan §1.4 | 同一设计域的输入/输出；拆开只多两个 bin |
| 需求必须带可测 `acceptance[]` | Claude-Code-Game-Studios `templates/story.md:98-99` | 没有断言的需求不可验收，会在 step-400 变成"看起来还行" |
| 系统分层 Foundation→Core→Feature→Presentation | Claude-Code-Game-Studios `design/CLAUDE.md:7-21` | 决定资产生产顺序：底座先做 |
| `--until draft` 硬绊线 | game-creator `make-game/SKILL.md:42`（"No code before plan"） | gates 没过就不许进 step-200 |
| 立项只留 3 项必需产物 | Claude-Code-Game-Studios `workflow-catalog.yaml:80-140` | 单人项目不需要 5 份立项文档 |
| **反例**：phase 靠提示词推进 | gamestudio-subagents `project_manager.py:250` | 该仓库 phase 永不前进；我们由 `step.json.status` 代码写 |
| **反例**：49 个角色 | Claude-Code-Game-Studios `studio-tests/agents/` | 单人管线 1 个 designer + 1 个 artist 足够 |

**约束 + 验收**

- 非空目录二次 init → `6`；缺 `game_id` → `3`；`game_id` 不匹配 `^[a-z][a-z0-9-]{1,31}$` → `3`；`genre` 不存在 → `4`。
- 不读 `work/200+`；不调 `step-300`。
- fake provider e2e → 全部 JSON + gates pass + 退出码 `10`（待审批）。
- 篡改 `game-design.json` 一字节 → `approve` 失效 → `gate` 返回 `15`。
- `pipeline.json` 必须含 `slice` 字段（否则 `step-500 slice` 无法工作）。
- 同一 brief 两个目录 init，`step.json.outputs[].digest` 完全一致（零时间戳）。

---

### 4.2 step-200-art-spec（素材规格）

**职责**：artist 角色调文本模型，把资产需求变成**每个资产一组 spec 文件**。全流程唯一"用 LLM 描述像素"的阶段。

**输入**：`asset-requirements.json` + `style-guide.json`（可来自任意路径，不依赖 step-100 的目录结构）
**输出**：`work/200-art-spec/`（或 `.slice/`）

```
palette.json                       # 共享调色板
spec-manifest.json                 # 每资产: spec_kind / files[] / status / digest / scope
reports/art-spec.gates.json
<asset-id>/
  <asset-id>.svg | .ascii.txt | .shapes.json
  <asset-id>.parts.json            # 可选：骨架类
  <asset-id>.recipe.json           # 必须：装配配方（= 锚点）
step.json
```

**CLI**

```
step-200 run          --in <dir> --out <dir> [--scope all|slice] [--asset <id>] [--provider ...] [--max-turns N]
step-200 lint         --in <dir>                  # 零模型、零构建
step-200 capabilities                              # 转发 step-300 list / schema
step-200 status       --in <dir>
```

**引用**

| 设计 | 出处 | 为什么 |
|---|---|---|
| **`recipe.json` 是锚点，动画与特效全部派生** | ai-game-spritesheets `03-neutral-anchor.md:3`（"Anchors are boring on purpose"） | 锚点错 1 处放大成 N 帧；源 spec 必须可复用 |
| **特效只作 recipe 里的 op，绝不进源 spec** | ai-game-spritesheets `06:3`、`04:46-48` | 对应 pxl 的 `hurt_flash`/`death_dissolve` 是 build 时 op |
| **每个入参声明"角色 + 禁抄边界"** | ai-game-spritesheets `02:26-28`、`06:29-31`（"Use it **only** to enforce X. Do not copy its content."） | 多份参考混用互相污染；prompt 层最便宜的一致性保险 |
| **Preserve / Change 成对差分块** | ai-game-spritesheets `03:32,40-51` | 修复轮只允许改一处，其余逐项 must-preserve |
| **东向 = 水平翻转，不生成** | ai-game-spritesheets `04:61` | pxl 已实现（`README.md:52`），补成 step-300 对称性断言 |
| **QA 与创作分离 → 禁止构建** | game-creator `game-creator.md:119-133` | 创作步返回"规格"，构建步返回"产物"，失败归属才清楚 |
| **反例**：写死 5×2 / 10 帧 | ai-game-spritesheets `06:19-21` | 帧数应由 `asset-requirements.anims` 声明 |
| **反例**：N 变体人工挑 | ai-game-spritesheets `03:31,67` | 违反幂等；改用 seed + golden |
| **反例**：`re-roll` 作控制流 | ai-game-spritesheets `03:67`、`07:59` | 与确定性冲突；用 seed + 退出码表达失败 |

**约束（本 plan 最重要的一条）**

- **禁止构建**：`assets.build` 工具删除；`auto_check_pass`（`harness/src/pipeline/assets_stage.rs:1503`）移出；任何 `cargo` / `step-300 build` 调用视为缺陷。
- **只允许两个只读外部调用**：`step-300 list --json`、`step-300 schema <kind> --json`。
- 版本握手读 `pipeline.json.steps`，废弃 `which pxl` + 二进制 sha256（`harness/src/adapters/pxl.rs:64-104` 整体废弃）。
- `lint` 必须调 `step-300 plan --dry-run` 验证每个 `recipe.json` 可解析。
- 单资产失败不阻断整批：manifest 标 `status: "missing" | "invalid"`，整体退出码 `11`。
- **不写 `work/300-build`**，不产生任何 PNG。

**验收**

- 删掉 `work/300-build` 后 `step-200 run` 仍成功。
- `rg 'auto_build|auto_check_pass|build_report' steps/step-200-art-spec/src` 零命中。
- `events.jsonl` 的 proc 事件只有 `step-300 list` 与 `step-300 schema`。
- `--scope slice` 只产出 `slice.assets[]` 里的资产，`spec-manifest.json` 标 `scope: "slice"`。

---

### 4.3 step-300-asset-build（素材生成 = `crates/` 整体）

**职责**：确定性像素构建。**唯一做像素运算的大 step**，也是 `crates/` 的唯一对外形态。

**输入**：`work/200-art-spec/`（或 `.slice/`）
**输出**：`work/300-build/`

```
index.json                     # 全资产 → 产物路径 / digest / qa status
<asset-id>/
  doc.pxl.json
  sheet.json
  *.png / *.gif / preview.png
  qa.json
  build-report.json
step.json
```

**CLI（新增批量入口 + 原 `pxl` 全部子命令）**

```
# 批量：step 之间唯一推荐的入口（新增）
step-300 plan     --in <w200> --out <w300> [--scope all|slice] [--asset <id>] [--dry-run] [--jobs N]

# 单件：原 pxl 语义完全不变
step-300 build    <recipe> -o <dir>
step-300 compile  <spec> -o <dir>          # svg / ascii / shapes / parts → doc
step-300 render   <doc> [--frame F] -o <file>
step-300 sheet    <doc...> -o <dir>
step-300 qa       <file> --rules <r>
step-300 gen | op | anim | palette | tile | inspect | profile | schema | list
step-300 enhance | pixelize                # 重建支线，非主链
```

**内部工位与子命令的对应**（工位不对外，仅供定位代码）：

| 工位（crate） | 承载的子命令 |
|---|---|
| `pxl-format` | `schema` / `list` |
| `pxl-gen` | `gen` / `tile` / ASCII 编译 |
| `pxl-svg` | `svg render/lint/matrix` |
| `pxl-art` | `enhance` / `pixelize` |
| `pxl-ops` | `op` / `anim` |
| `pxl-qa` | `qa` |
| `pxl-export` | `render` / `sheet` / preview |
| `pxl-engine` | `compile`（统一装配）/ `inspect` |
| `pxl-core` | 原语（被全部使用） |
| `pxl-wasm` | web playground（独立产物） |
| `pxl-cli` | bin `step-300` 的命令层与全部文件 IO |

**引用**

| 设计 | 出处 | 为什么 |
|---|---|---|
| **归一化是最后一道工序，不在每步做** | ai-game-spritesheets `08-normalization.md` + `08:38` | 前置没有"标尺"；且是不变量层，一次全量扫描即可 |
| **锁 `center_x` / `bottom_y` 跨帧相等** | ai-game-spritesheets `08:42-46` | "角色不抖"的唯一可判定条件 → 落成 `qa.json` 断言 |
| **按实测包围盒，不信名义网格** | ai-game-spritesheets `08:17,29-34` | 防"声明 16×32 实际只占下半"的静默错误 |
| **结构性对称：东向 = 翻转西向** | ai-game-spritesheets `04:61` | 省一次生成 + 像素级保证对称 |
| **QA 输出结构化判决，不输出图片** | game-creator `game-creator.md:119-133` | 下游只读 JSON；截图只进 `reports/` 供人看 |
| **`--dry-run` 只解析零落盘** | game-creator `Known Tolerances` + Claude-Code-Game-Studios dry-run 思路 | 让 step-200 的 lint 能安全调用 |
| **失败重试有记忆** | game-creator `game-creator.md:148-163`（`autofix-history.json`） | 落成 `attempts.jsonl`：`{asset, issue, fix, result}`，3 次后标 `blocked` 不阻塞整批 |
| **`.meta` 由脚本写不由 agent 写** | game-creator `meshy-generate.mjs:339-391` | `build-report.json` 由 step-300 生成，不许模型手写 |

**约束**

- 只读 `--in`，只写 `--out`。
- `--dry-run` 不产生任何文件（用目录 digest 断言）。
- 单资产失败不中断整批；`index.json` 记 `status`，整体退出码取最严重者。
- 只用退出码 `0-6`，不引入 `10-16`（保持对旧调用方兼容）。
- **golden 不变**：`pxl-cli/tests/golden/hashes.json` 58 条全部保持。
- **crate 名全部不变**：`use pxl_core::` 一行不用动；`cargo test -p pxl-cli` 照旧。

**验收**

- `step-300 build examples/tank/tank.recipe.json` 与旧 `pxl build` 的 sha256 完全一致。
- `step-300 plan` 对 battle-city 20 资产全量构建，`index.json` 可独立复算。
- 原 `pxl-cli/tests/cli_cases/` 12 个用例全部迁移并通过。
- `step-300 plan --scope slice` 只构建 1 个资产，耗时 < 全量 10%。

---

### 4.4 step-400-package（交付包）

**职责**：把"构建产物"收敛成"引擎可消费的包"，并做覆盖检查、内容寻址、审批。

**输入**：`work/300-build/index.json` + `--requirements <asset-requirements.json>`
**输出**：`work/400-package/`

| 文件 | 内容 |
|---|---|
| `asset-manifest.json` | 每资产：需求 vs 产物、帧数、sheet、digest、`DeliveryStatus` |
| `coverage.json` | 覆盖率、缺失、多余 |
| `revision.manifest.json` | 不可变 revision + 清单 digest（三态 `APPROVED / NEEDS REVISION / REJECTED`） |
| `publish-request.json` | 发布请求 |
| `asset-package.json` | **引擎唯一入口** |
| `attempts.jsonl` | 修复历史（不进 digest） |

`asset-package.json` 草案：

```json
{
  "schema_version": 1,
  "package_id": "battle-city@a1792735985822",
  "engine": { "name": "macroquad", "contract": "engine/v1", "frame_max": 256, "palette_max": 62 },
  "palette": { "path": "palette.json", "digest": "sha256:..." },
  "sheets": [
    { "id": "units", "path": "units.sheet.json", "digest": "sha256:...", "frames": 24 }
  ],
  "assets": [
    { "id": "player_tank", "kind": "animated", "sheet": "units",
      "anims": { "move_S": { "frames": 4, "fps": 8 } } }
  ],
  "levels": { "path": "levels.json", "digest": "sha256:..." }
}
```

**CLI**

```
step-400 run     --in <w300> --requirements <asset-requirements.json> --out <w400>
step-400 check   --in <w400>
step-400 approve --in <w400> --digest sha256:...
step-400 status  --in <w400>
```

**引用**

| 设计 | 出处 | 为什么 |
|---|---|---|
| **gate 三件套**：可机器查的产物 / 需人判的质量 / 可放宽项 | Claude-Code-Game-Studios `gate-check/references/gate-*.md` | 分层本质是"别让机器假装能判审美" |
| **`NOT ASSESSED` 压过 PASS** | Claude-Code-Game-Studios `5-verdict.md:58-65` | 缺 sheet、缺 digest、manifest 全占位 → 不是通过 |
| **tier 放宽只放宽"存在"，不放宽"能用"** | Claude-Code-Game-Studios `gate-production.md:83-86` | 快速模式可以少做资产，但不能有坏资产 |
| **永不为制造 PASS 自动补文件** | Claude-Code-Game-Studios `gate-check/SKILL.md:149-152` | 缺就是缺，补等于作弊 |
| **交付清单四件套：目录/命名/必需文件/平台** | gamestudio-subagents `godot_config.json:4-23` | 变成 `step-500 engine.json` 的 schema，不是提示词 |
| **handoff 三态 + 双签** | gamestudio-subagents `handoff_template.md:24-27,137-146` | 落成 `revision.manifest.json.status` |
| **清单由脚本写，不许模型手写** | game-creator `meshy-generate.mjs:339-391` | `asset-manifest.json` 由 step-400 生成 |
| **只存可再生 ID，不存过期 URL** | game-creator `gigachad.meta.json:8` | 清单里只放相对路径 + digest |

**约束**

- **不做像素运算**：只读文件、算 digest、比对。解码逻辑在 step-300 的 `pxl-qa` / `pxl-export`。
- 缺失需求资产 → `11`；产物 QA `error` → `5`；额外产物 → `warning`；digest 不符 → `15`。
- `asset-package.json` 所有 `path` 相对工作区根。
- `attempts.jsonl` 不参与 digest。

**验收**

- battle-city 20 资产 → 覆盖率 100%，`status: "ok"`。
- 删 `units.sheet.json` → `11`；改一字节 PNG → `15`；manifest 换全零 digest → `15`。
- `step-400 check` 幂等（二次运行 digest 相同）。

---

### 4.5 step-500-engine（引擎接入）

**职责**：引擎侧契约守门 + 垂直切片判定。本期实现 `check` 与 `slice`；`build` / `playtest` 占位。

**输入**：`work/400-package/asset-package.json`（`slice` 另读 `work/*/.slice/`）
**输出**：`work/500-engine/`（`engine-check.json` / `slice-report.json`）

**CLI**

```
step-500 check    --in <w400> [--runtime games/battle-city/runtime]   # 本期
step-500 slice    --work <work dir> --out <w500>                      # 本期：垂直切片判定
step-500 build    ...                                                  # 后续：原 harness build_stage
step-500 playtest ...                                                  # 后续：原 harness playtest_stage
```

**`check` 校验项（5 条）**

| # | 断言 | 失败码 |
|---|---|---|
| 1 | 每个 `sheets[].path` 存在且 digest 匹配 | `15` |
| 2 | 每个 frame 像素索引 `< palette.len()` | `4` |
| 3 | `assets[].anims` 键 ⊆ sheet 的 `meta.frameTags` | `4` |
| 4 | `levels.json` 引用的 tile id 全在 `sheets[]` | `11` |
| 5 | `engine.frame_max` / `palette_max` ≥ 实际产物上界 | `4` |

**`slice` 判定**（读 `work/{200,300,400,500}` 的 `.slice/` 产物）：

```json
{
  "schema_version": 1,
  "verdict": "PROCEED",
  "questions": [
    { "id": "loop.playable", "answer": "yes", "evidence": "500-engine/.slice/engine-check.json" },
    { "id": "art.scale_ok", "answer": "yes", "evidence": "300-build/.slice/index.json" },
    { "id": "pipeline.repair_rounds", "answer": "3", "evidence": "400-package/.slice/attempts.jsonl" }
  ],
  "missing": [],
  "next": "step-200 run --scope all"
}
```

**引用**

| 设计 | 出处 | 为什么 |
|---|---|---|
| **引擎能力清单四件套** | gamestudio-subagents `godot_config.json:4-23` | `engine.json` 是"引擎接受什么"的声明式契约：目录映射/命名/必需文件/平台 |
| **`required_files` 白名单** | 同上 `godot_config.json:20-23` | 缺什么就算没交付，可机器判定 |
| **四值裁决 `PROCEED/PIVOT/KILL/NOT ASSESSED`** | Claude-Code-Game-Studios `vertical-slice-report.md:93` | 把"要不要继续投"变成可归档文件 |
| **"建了但没跑过 = NOT ASSESSED，永不通过"** | Claude-Code-Game-Studios `gate-production.md:54-57` | 防"资产造出来了就算完成" |
| **验证问题 = 体验 + 可行性双问** | Claude-Code-Game-Studios `vertical-slice-report.md:10-15` | 只问"好不好看"会漏掉"能不能量产" |
| **Build Velocity Log 当产能基线** | Claude-Code-Game-Studios `vertical-slice-report.md:34-48` | 落成 `pipeline.repair_rounds`：切片返工几轮，预测全量几轮 |
| **3 次修复上限** | game-creator `game-creator.md:148-163` | 超过 3 轮修不好 → 不是"再试试"，是规格有问题 → `PIVOT` |
| **机器可读状态 `--dump-state`** | game-creator `CLAUDE.md:103-105`（`render_game_to_text()` / `advanceTime(ms)`） | 让 playtest 能"读游戏"而不是"读像素"（后续子命令用） |
| **切片归引擎 step、不占顶层** | 本 plan §1.1 | 切片核心问题是"能不能玩"；顶层只放大 step |
| **反例**：引擎配置塞进 agent 提示词 | gamestudio-subagents `agent_customizer.py:141-177` | 按 agent 名切片注入，脆弱且不可校验 |

**约束**

- `slice` 任何一项 `missing` 非空 → 强制 `NOT ASSESSED`，不允许 `PROCEED`。
- `PROCEED` 需要四份 slice 产物齐全且 digest 互相对得上。
- `KILL` / `PIVOT` 只写报告，**不删除任何产物**。
- 不读 runtime 源码；不启窗口；不跑 cargo（本期）。

**验收**

- battle-city 的 `asset-package.json` → `check` 全 pass，退出码 `0`。
- `palette_max: 61` → `4`；删一个 frame → `11`；改 sheet digest → `15`。
- 四份 slice 齐全 → `PROCEED`；删 `500-engine/.slice/` → `NOT ASSESSED`（不是 PASS）；`repair_rounds > 3` → `PIVOT`。
- 报告二次生成 digest 相同。

---

## 5. 契约层

### 5.1 唯一接口是文件系统

1. **跨 step 不许 `use` 对方 crate**（`libs/*` 除外）。需要信息就加子命令 + JSON。
2. **输入契约 = 上次输出目录 = fixture 目录**。三者结构必须完全一致，否则"独立运行"是假的。
3. **上游只读**。每个 step 只写自己的 `--out`；需要索引就写自己的目录。

### 5.2 `step.json` 信封

```json
{
  "schema_version": 1,
  "step": "step-200-art-spec",
  "step_version": "0.1.0",
  "contract": "art-spec/v1",
  "status": "ok",
  "scope": "slice",
  "inputs":  [{ "path": "asset-requirements.json", "digest": "sha256:..." }],
  "outputs": [{ "path": "spec-manifest.json", "digest": "sha256:..." }],
  "verdict": "PASS",
  "gates": {
    "required_artifacts": [{ "id": "spec-manifest.json", "exists": true }],
    "quality_checks":    [{ "id": "spec.recipe_refs", "result": "pass" }],
    "not_assessed":      []
  },
  "error": null
}
```

字段设计的三处引用：

| 字段 | 出处 | 为什么 |
|---|---|---|
| `scope` | game-creator `progress.md` 阶段复选框（`game-creator.md:181-202`） | 一个字段同时表达"跑到哪"和"跑了多少" |
| `verdict`（PASS/CONCERNS/NOT ASSESSED/FAIL） | Claude-Code-Game-Studios `5-verdict.md:58-65` | 四值比布尔多出"没查"这一档 |
| `gates` 三件套 | Claude-Code-Game-Studios `gate-*.md` | 机器可查 / 需人判 / 未评估 分开记录 |

**硬规则**：不含时间戳、耗时、绝对路径、主机名。可观测性只写 `work/_runs/<id>/events.jsonl`。

### 5.3 裁决四值

| 裁决 | 含义 | 退出码 | 后续 |
|---|---|---|---|
| `PASS` | 全部 required 存在且 quality 通过 | `0` | 进下一步 |
| `CONCERNS` | 有 warning，可显式接受 | `0` + `--accept-risk` | 记录后进下一步 |
| `NOT ASSESSED` | 有 required 缺失或无法检查 | `11` | **必须补齐重跑** |
| `FAIL` | 有 error | `5` | 修复后重跑 |

**优先级**：`FAIL > CONCERNS > NOT ASSESSED > PASS`（抄 `5-verdict.md:58-65`）。

**触发 `NOT ASSESSED` 的条件（写死，不许放宽）**：

1. required artifact 不存在；
2. 文件存在但内容是空/全占位（如 `assets: []`）；
3. gate 报告里 `checks` 为空数组；
4. 上游 `step.json.status ≠ ok`。

### 5.4 退出码

| code | 含义 | 适用 |
|---|---|---|
| 0 | ok / PASS | 全部 |
| 1 | IO | 全部 |
| 2 | usage | 全部 |
| 3 | schema | 全部 |
| 4 | semantic | 全部 |
| 5 | gate / qa failed | 全部 |
| 6 | output exists | 全部 |
| 10 | waiting_approval | 100 / 400 |
| 11 | blocked / NOT ASSESSED | 200 / 400 / 500 |
| 12 | failed | 全部 |
| 13 | budget_exhausted | 100 / 200 |
| 14 | lock | 100 / 200 / 400 |
| 15 | integrity | 全部 |
| 16 | provider / tool | 100 / 200 |

**约束**：`step-300` 只用 `0-6`（保持对旧调用方兼容）。

### 5.5 工作区布局

```
<game>/
  brief.json
  pipeline.json
  work/
    100-design/
    200-art-spec/
      .slice/                 # 切片范围（独立子目录，不靠 flag 区分）
      <asset-id>/             # 全量
    300-build/
      .slice/
    400-package/
      .slice/
    500-engine/
      .slice/
      slice-report.json
    _runs/<run-id>/
      events.jsonl            # 不进 digest
      attempts.jsonl          # 不进 digest
      budget.json             # 不进 digest
```

**`.slice/` 用独立子目录**：`step-500 slice` 只读 `*/.slice/`，全量产物不会被误判；也方便一键删除重跑切片。

### 5.6 确定性

| 规则 | 说明 |
|---|---|
| canonical JSON | 对象键排序，digest 不依赖序列化顺序 |
| 文件 digest | `sha256:<hex>` |
| 输入指纹 | 所有输入 digest 的有序聚合 |
| step 版本入指纹 | `pipeline.json.steps[*].version` 变化即失效下游 |
| 事件日志不参与 | `work/_runs/**` 永不进 digest |
| 幂等 | 同输入二次运行，`step.json.outputs[].digest` 完全一致 |

---

## 6. 引用指引总表

> 用法：改代码前先查这张表，知道某个约束"为什么存在、出自哪里、能否放宽"。

| # | 我们的设计 | 出处项目 | 原文位置 | 落在 | 能否放宽 |
|---|---|---|---|---|---|
| 1 | 锚点单一真源，动画/特效全部派生 | ai-game-spritesheets | `prompts/03-neutral-anchor.md:3` | step-200 | 否 |
| 2 | 特效只作 build 时 op，绝不进源 spec | ai-game-spritesheets | `06-attack-spritesheet.md:3`、`04:46-48` | step-200/300 | 否 |
| 3 | 归一化锁 `bottom_y` / `center_x` | ai-game-spritesheets | `08-normalization.md:42-46` | step-300/400 | 否 |
| 4 | 按实测包围盒而非名义网格 | ai-game-spritesheets | `08:17,29-34` | step-300 | 否 |
| 5 | 东向 = 水平翻转 | ai-game-spritesheets | `04:61` | step-300 | 否 |
| 6 | 每个入参声明"角色 + 禁抄边界" | ai-game-spritesheets | `02:26-28`、`06:29-31` | step-200 | 是（prompt 层） |
| 7 | Preserve / Change 成对差分 | ai-game-spritesheets | `03:32,40-51` | step-200 修复轮 | 是 |
| 8 | 反例入库（负样本语料） | ai-game-spritesheets | `02b-south-from-boxart-rejected.png` | `tests/fixtures/rejected/` | 是 |
| 9 | gate 三件套（产物/质量/放宽） | Claude-Code-Game-Studios | `gate-check/references/gate-*.md` | 全部 | 否 |
| 10 | `NOT ASSESSED` 一等裁决 | Claude-Code-Game-Studios | `5-verdict.md:58-65` | 全部 | 否 |
| 11 | tier 只放宽"存在"，不放宽"能用" | Claude-Code-Game-Studios | `gate-production.md:83-86` | step-400/500 | 否 |
| 12 | 永不为制造 PASS 自动补文件 | Claude-Code-Game-Studios | `gate-check/SKILL.md:149-152` | step-400 | 否 |
| 13 | 垂直切片四值裁决 | Claude-Code-Game-Studios | `vertical-slice-report.md:93` | `step-500 slice` | 否 |
| 14 | "没跑过 = NOT ASSESSED" | Claude-Code-Game-Studios | `gate-production.md:54-57` | `step-500 slice` | 否 |
| 15 | 需求必须带可测 acceptance | Claude-Code-Game-Studios | `templates/story.md:98-99` | step-100 | 否 |
| 16 | 系统分层 Foundation→Presentation | Claude-Code-Game-Studios | `design/CLAUDE.md:7-21` | step-100 | 是 |
| 17 | 原 prompt 逐字置顶 + 阶段复选框 | game-creator | `game-creator.md:181-204` | step-100 / `step.json.scope` | 否 |
| 18 | QA 只回结构化判决、禁图、critical 早停 | game-creator | `game-creator.md:119-133` | step-300/400 | 否 |
| 19 | `autofix-history` 记忆失败 + 3 次上限 | game-creator | `game-creator.md:148-163` | step-300 / `step-500 slice` | 是（次数） |
| 20 | `.meta.json` 由脚本写不由 agent 写 | game-creator | `meshy-generate.mjs:339-391` | step-400 | 否 |
| 21 | 只存可再生 ID，不存过期 URL | game-creator | `gigachad.meta.json:8` | step-400 | 否 |
| 22 | 模板自带验证脚本 | game-creator | `README.md:172` | 各 step `tests/` | 是 |
| 23 | 机器可读状态 `--dump-state` | game-creator | `CLAUDE.md:103-105` | step-500（后续） | 是 |
| 24 | 引擎能力清单四件套 | gamestudio-subagents | `godot_config.json:4-23` | `step-500 engine.json` | 否 |
| 25 | handoff 三态 + 双签 | gamestudio-subagents | `handoff_template.md:24-27,137-146` | step-400 | 是（签名人数） |
| 26 | 状态必须由代码推进，不靠提示词 | gamestudio-subagents（**反例**） | `project_manager.py:250` | 全部 `step.json` | 否 |
| 27 | 契约必须 `--contract` 自证 | gamestudio-subagents（**反例**） | `init_project.py:785-823` | 全部 | 否 |
| 28 | gate 必须有断言，禁止 `print PASS` | gamestudio-subagents（**反例**） | `test_project_workflow.py:61-74` | 全部 | 否 |
| 29 | 写死帧数/网格 = 倒退 | ai-game-spritesheets（**反例**） | `06:19-21` | step-200 | 否 |
| 30 | `re-roll` 不能作控制流 | ai-game-spritesheets（**反例**） | `03:67`、`07:59` | step-200/300 | 否 |

---

## 7. 现状 → 目标映射

### 7.1 `crates/` → `steps/step-300-asset-build/`

| 现 | 新路径 | crate 名 | bin |
|---|---|---|---|
| `crates/pxl-core` | `steps/step-300-asset-build/pxl-core` | **不变** | — |
| `crates/pxl-format` | `…/pxl-format` | 不变 | `step-300 schema` |
| `crates/pxl-gen` | `…/pxl-gen` | 不变 | `step-300 gen` |
| `crates/pxl-svg` | `…/pxl-svg` | 不变 | `step-300 svg` |
| `crates/pxl-art` | `…/pxl-art` | 不变 | `step-300 enhance` |
| `crates/pxl-ops` | `…/pxl-ops` | 不变 | `step-300 op` |
| `crates/pxl-qa` | `…/pxl-qa` | 不变 | `step-300 qa` |
| `crates/pxl-export` | `…/pxl-export` | 不变 | `step-300 render` |
| `crates/pxl-engine` | `…/pxl-engine` | 不变 | `step-300 inspect` |
| `crates/pxl-wasm` | `…/pxl-wasm` | 不变 | `[lib] name = "pxl_wasm"` 冻结 |
| `crates/pxl-cli` | `…/pxl-cli` | 不变 | `step-300` + `pxl` |

**只改路径，不改 crate 名**。146 个 `.rs` 的 `use pxl_core::` 一行不动，golden 不变。

### 7.2 `harness/` → 各大 step + libs

| 现文件 | 新归属 | 处理 |
|---|---|---|
| `src/main.rs` | — | 删（被 5 个 step bin 取代） |
| `src/cli.rs`（init） | `step-100` `init` 子命令 | 重写 |
| `src/cli.rs`（run/status/approve） | 各 step 对应子命令 | 拆 |
| `src/cli_agent.rs` | `libs/pxl-agent` | 保留为可选 bin |
| `src/config.rs` | `libs/pxl-agent` | 改名 `agent.config.json` |
| `src/digest.rs` | `libs/pxl-store` | 原样 |
| `src/error.rs` | `libs/pxl-contract` | 退出码表（§5.4） |
| `src/skills.rs` | `libs/pxl-agent` | 原样 |
| `src/contracts/design.rs` | `step-100/src/contracts/` | 整体归属设计域 |
| `src/contracts/assets.rs` | `step-400/src/manifest/` | 原样 |
| `src/agent/**` | `libs/pxl-agent` | 原样 |
| `src/pipeline/design_stage.rs` | `step-100/src/stage/` | 搬迁 + 换 IO |
| `src/pipeline/assets_stage.rs` 创作部分 | `step-200/src/` | **拆分**（见下） |
| `src/pipeline/assets_stage.rs` gates 部分 | `step-400/src/gates/` | **拆分** |
| `src/pipeline/asset_queue.rs` | `step-300`（调度）+ `step-400`（指纹） | 拆 |
| `src/pipeline/build_stage.rs` | `step-500` `build` 子命令 | 暂留原位 |
| `src/pipeline/playtest_stage.rs` | `step-500` `playtest` 子命令 | 暂留原位 |
| `src/pipeline/runner.rs` | — | 删；编排交给 `scripts/pipeline.sh` |
| `src/pipeline/state.rs` | — | 删；状态改为 `step.json` |
| `src/pipeline/review_view.rs` | `step-100/src/review/` | 原样 |
| `src/adapters/process.rs` | `libs/pxl-agent` | 原样 |
| `src/adapters/pxl.rs` | `step-200/src/cmd/capabilities.rs` | 瘦身：只留 `list`/`schema`/`version` |
| `src/policy/**` | `libs/pxl-agent` | 原样 |
| `src/storage/**` | `libs/pxl-store` | 原样 |
| `src/qa/mod.rs` | `libs/pxl-contract` | `GateReport` 模型 |
| `roles/designer*.md` | `step-100/roles/` | 原样 |
| `roles/artist*.md` | `step-200/roles/` | 原样 |
| `skills/macroquad/SKILL.md` | `step-500/skills/` | 原样 |
| `templates/top_down_tank/**` | `step-100/templates/` | 原样 |
| `tests/cli_e2e.rs` | 各 step `tests/` + `tests/pipeline_e2e.rs` | 拆 + 新增 |

### 7.3 CLI 命令去向

| 现命令 | 新 |
|---|---|
| `pxl gen / tile / palette / op / anim` | `step-300` 同名子命令 |
| `pxl ascii import/render` | `step-300 compile` |
| `pxl svg render/lint/matrix` | `step-300 svg` |
| `pxl sprite build` | `step-300 compile` |
| `pxl pixelize / enhance` | `step-300 pixelize / enhance` |
| `pxl sheet / render / inspect` | `step-300` 同名 |
| `pxl validate / profile / schema / list` | `step-300` 同名 |
| `pxl build` | `step-300 build`（单件）+ `step-300 plan`（批量，新增） |
| `pxl-harness init` | `step-100 init` |
| `pxl-harness run --until design` | `step-100 run` |
| `pxl-harness run --until assets` | `step-200 run` + `step-300 plan` + `step-400 run` |
| `pxl-harness approve` | `step-100 approve` / `step-400 approve` |
| `pxl-harness status` | 各 step `status` |
| `pxl-harness agent` | `libs/pxl-agent` 的 `agent` bin |
| `pxl-harness run --until build/playtest` | `step-500 build / playtest`（后续） |

### 7.4 工作区数据迁移

| 现路径 | 新路径 |
|---|---|
| `games/*/project.json` | `work/100-design/` + `pipeline.json` |
| `games/*/revisions/design/d*/` | `work/100-design/` |
| `games/*/revisions/assets/a*/` | `work/200-art-spec/` + `work/300-build/` |
| `games/*/runs/r*/events.jsonl` | `work/_runs/<id>/events.jsonl` |
| `games/*/revisions/assets/a*/out/` | `work/300-build/<asset-id>/` |

脚本 `scripts/migrate-v3-workspace.py`：幂等、旧目录只读保留、迁移后 `step-400 check` 能通过。

---

## 8. 实施里程碑

### M0：契约与骨架（不改行为）

- `steps/` 5 个空 crate + `libs/` 3 个库骨架；
- `steps/contracts/`：`step.schema.json` + 5 个业务 schema；
- `steps/README.md`：大 step 总表、子命令清单、单跑矩阵、`--contract` 用法；
- workspace `members` 更新（旧 crate 保留）。

**验收**：`cargo build --workspace` 绿；每个 `step-NNN --help` / `--contract` 可用；现有 `cargo test` 结果不变。
**回滚**：删 `steps/` 与 `libs/`。

### M1：`crates/` 迁入 `step-300-asset-build`（纯路径）

- `crates/` 整体 `git mv` 到 `steps/step-300-asset-build/`；
- workspace members 与 `[workspace.dependencies]` 的 path 改路径；`build.sh` / `scripts/build-wasm.sh` 改路径；
- `pxl-cli` 增加 `[[bin]] name = "step-300"`，保留 `pxl`；
- 新增 `plan` 子命令与 `--dry-run`。

**验收**：`cargo test --workspace` 全绿；golden 58 条不变；`step-300 build examples/tank/tank.recipe.json` 与旧 `pxl build` sha256 一致；`./build.sh wasm` 成功。
**回滚**：单提交 revert。

### M2：`step-200` 落地（职责切分，最关键）

- 抽 `libs/pxl-agent` / `libs/pxl-store` / `libs/pxl-contract`；
- 新建 `step-200-art-spec`，搬 `assets_stage.rs` 创作链；
- **删** `assets.build` 工具与 `auto_check_pass`；`adapters/pxl.rs` 瘦身为 `capabilities`；
- 新增 `lint` 与 `--scope slice`。

**验收**：删 `work/300-build` 后 step-200 仍成功；`lint` 全绿；源码零 `auto_build`；proc 事件只有两个只读命令。
**回滚**：`harness` 原文件保留，切回入口即可。

### M3：`step-100` 落地

- 搬 `design_stage.rs` + `roles/designer*` + `review_view.rs` + `templates/`；
- `init` / `run` / `gate` / `approve` / `--until draft`。

**验收**：fake provider e2e → 全部 JSON + gates pass + 退出码 `10`；篡改后 `15`。
**回滚**：同上。

### M4：`step-400` 落地

- 从 `assets_stage.rs` 抽 gates，从 `asset_queue.rs` 抽指纹；
- 新增 `coverage.json`、`asset-package.json`、`attempts.jsonl`；
- 四值裁决 + 三件套 + 三态审批。

**验收**：battle-city 覆盖率 100%；删 sheet → `11`；篡改 → `15`；幂等。
**回滚**：`revisions/` 只读保留。

### M5：`step-500 check + slice` + 迁移

- `step-500 check`（5 项断言）+ `engine.json` 四件套；
- `step-500 slice`（四值裁决，读 `.slice/` 产物）；
- `step-100 migrate` + `scripts/migrate-v3-workspace.py`；
- 迁移三个 `games/*`。

**验收**：切片回路 `--scope slice` 全链可跑；四份 slice 齐全 → `PROCEED`；缺一份 → `NOT ASSESSED`；`repair_rounds > 3` → `PIVOT`；迁移幂等。
**回滚**：旧目录只读，`work/` 可整体删。

### M6：收尾

- 删 `harness/`；删 `pxl` 兼容 bin；
- 更新 `README.md` / `plan/README.md` / `scripts/skills/pxl/**` / `build.sh`；
- 新增 `tests/pipeline_e2e.rs`（slice → PROCEED → 全量 → package → check）。

**验收**：`cargo test --workspace` 全绿；`rg 'pxl_harness'` 零命中；e2e < 3 min（fake provider）。
**回滚**：tag。

---

## 9. 验收矩阵

| 维度 | 怎么查 | 通过标准 |
|---|---|---|
| 单跑 | 用 `tests/fixtures/` 逐 step 运行 | 每个 `step-NNN` 退出码 `0` |
| 隔离 | 运行前删所有下游目录 | 上游 step 仍成功 |
| 幂等 | 同输入二次运行 | `step.json.outputs[].digest` 一致 |
| 契约自证 | `step-NNN --contract` | 输出 JSON 且能被 `step-300 schema` 校验 |
| 确定性 | golden | 58 条 sha256 不变 |
| 职责 | 源码扫描 | step-200 无 `Command::new`（capabilities 除外）；step-400 无像素解码 |
| 裁决 | 构造缺产物 | 必须 `NOT ASSESSED`，不得 `PASS` |
| 切片 | `--scope slice` | 只出 1 资产，耗时 < 全量 10% |
| 全链 | `tests/pipeline_e2e.rs` | slice → PROCEED → 全量 → package → check |

**fixture 链**：每一步把输出冻结成下一步的 fixture（`steps/step-NNN/tests/fixtures/`）。这是"独立运行"的实操保证。

---

## 10. 风险与决策点

### 10.1 风险

| # | 风险 | 缓解 |
|---|---|---|
| R1 | `crates/` 路径搬迁打断 `build.sh` / `web/` / `skills` | M1 单提交 + `rg` 清单 + `./build.sh wasm` 进验收 |
| R2 | `assets_stage.rs` 84KB 拆分回归 | 先定契约再搬代码；用 battle-city 现有 revision 逐文件 diff |
| R3 | `pxl` 与 `step-300` 双入口混乱 | 一个 M 周期后 `pxl` 打弃用提示（D2） |
| R4 | LLM step 不可测 | 每 step 自带 `tests/fake_turns/*.json`；CI 全用 fake |
| R5 | runtime 依赖旧 manifest | M4 只增字段不改字段；runtime 本期零改动 |
| R6 | `pxl_wasm` 产物名不可改 | `[lib] name` 冻结 |
| R7 | 切片回路被跳过（图快） | `step-400` 全量 mode 检查 `500-engine/slice-report.json` 是否 `PROCEED` |

### 10.2 决策点

| # | 决策 | 推荐 | 备选 |
|---|---|---|---|
| D1 | `crates/` 内部 11 个 crate 是否改名 | **不改**，只改路径 | 加前缀（收益为零，改 146 文件） |
| D2 | `pxl` bin 是否永久保留 | **保留到 M6**，之后改别名 | 永久双入口 |
| D3 | `harness` 的通用 `agent` 命令 | 保留为 `libs/pxl-agent` 的 bin | 删除 |
| D4 | `step-500 slice` 是否本期实现 | **M5 实现**（成本低、价值高） | 只留占位 |
| D5 | 编排方式 | `scripts/pipeline.sh` 串行调用 | 保留 `step-all` 薄 bin |
| D6 | 是否强制切片 | **强制**：全量前必须 `PROCEED` | 可选（`--skip-slice`） |

---

## 11. 不做清单

- 不写工作流引擎（DAG / 重试 / 并发）。编排用 shell，失败即停。
- 不引入数据库、消息队列、对象存储。
- 不改任何像素算法、prompt 文本、角色定义。
- 不做多游戏模板（`top_down_tank` 之外不新增）。
- 不实现 `step-500 build / playtest`（只留占位）。
- 不删除 `examples/` / `review/` / `games/*/revisions`。
- 不重写 `web/`，只改 wasm 引用。
- 不移植四个开源项目的具体实现（Web 技术栈、i2v、chroma 抠图、Play.fun）。
- 不做 `pxl mcp`（维持现状未实现）。
- **不给小步骤建目录**：新增一个小步骤 = 加一个子命令，永远不动 `steps/` 顶层。

---

## 12. 附录：与上一版（v6）的差异

| 上一版（v6） | 本版（v7） | 原因 |
|---|---|---|
| 8 个顶层 step（含 650 slice-gate、700 playtest） | **5 个顶层大 step** | 顶层只放大 step；slice/playtest 降级为 `step-500` 子命令 |
| brief 独立成 `step-100-brief` | **brief 并入 `step-100-design`** 的 `init` 子命令 | 同一设计域的输入/输出，拆开只多两个 bin |
| 共享库放 `steps/_shared/` | **共享库移到 `libs/`** | `steps/` 顶层只允许大 step；库永远不叫 step |
| 大 step 内部结构未展开 | **§3.2 逐个展开内部目录** | 明确"小步骤 = 子命令"落在哪里 |
| （无） | **新增结构规则三条**（§3.1） | 回答"什么能出现在哪一层"，防止结构再次漂移 |

---

## 13. 实施记录

### M0 完成（2026-10-26）

- `steps/` 4 个 step 骨架 crate（step-100/200/400/500，每个 bin 的 `--help` 与 `--contract` 可用）+ `libs/` 3 个库骨架（`pxl-agent`、`pxl-store` 为 M2 占位，`pxl-contract` 已实现退出码表、四值 `Verdict`（含优先级组合 `and()`）、`step.json` 信封 serde 结构与 3 个单测）。
- `steps/contracts/`：9 个 JSON Schema 草案（step / pipeline / brief / design / art-spec / asset-build / asset-package / engine / slice）。
- `steps/README.md`：大 step 总表、结构规则三条、契约入口、单跑说明。
- step-300 骨架未建（按计划：M1 直接由 `crates/` 平移形成，避免临时 crate）。
- 验收：`cargo build --workspace` 全绿；4 个 bin `--help`/`--contract` 可用；现有测试结果不变。

### M1 完成（2026-10-26）

- `crates/` 11 个 crate 整体迁入 `steps/step-300-asset-build/`（`git mv` + shell `mv` + `git add`，git rename 检测正常；**crate 名全部不变**，`use pxl_core::` 零改动）。
- 修复 30 处深度敏感的相对路径：`pxl-engine/tests/examples.rs`（12 处 `include_str!`）、`pxl-wasm/src/lib.rs`（2 处）、`pxl-gen/src/{builtin,bollinger}.rs`（16 处）、`pxl-cli/src/cmd/build.rs`（2 处）、`pxl-cli/tests/{support/mod.rs,golden.rs}`（`CARGO_MANIFEST_DIR` 层级 +1）。
- 根 `Cargo.toml`（members + `[workspace.dependencies]` path）、`build.sh`（`WASM_SOURCES`）、`scripts/build-wasm.sh` 路径同步更新。
- `pxl-cli` 增加 `[[bin]] name = "step-300"`（与 `pxl` 指向同一 `main.rs`；`pxl` 为兼容别名保留至 M6）。
- 新增 `plan` 子命令（`cmd/plan.rs`）：`--in`（spec-manifest.json 发现模式 + `<id>/<id>.recipe.json` 扫描模式）、`--asset` 过滤、`--scope` 标签、`--dry-run`（零落盘）、`--qa-config` 透传；逐 recipe 复用 `build_cmd` 全管线（staging / QA / publish / `.build.json`），失败不中断整批，写 `index.json`（对齐 `asset-build/v1` 契约），最差状态决定退出码。
- 新增 4 个集成测试（`tests/cli_cases/plan.rs`）：plan 与 build 逐字节一致、`--dry-run` 零落盘（含失败路径）、单 recipe 失败不中断批次且退出码 5、spec-manifest 发现 + `--asset` 过滤 + scope 标签。
- 验收实测：
  - `cargo test --workspace`：**564 passed / 0 failed**（golden 58 条不变；harness e2e 全过；`pxl-engine` examples 修复后全过）；
  - `step-300 build examples/tank/tank.recipe.json` 与 `pxl build` 输出 `diff -r` **逐字节一致**；
  - `step-300 plan` 对 battle-city 存量 15 个 recipe：14 ok / 1 `probe` 失败（`unknown palette cycle 'tread'` ——存量数据问题，旧 `pxl` 同样报错，与本迁移无关）；二次运行 `index.json` sha256 一致（幂等）；
  - `./build.sh wasm` 成功，`pxl_wasm_bg.wasm` 1089140 bytes，`[lib] name = "pxl_wasm"` 冻结生效。
- git 状态：迁移与新文件已 stage（224 个路径），用户其余未提交变更（`.gitignore`、`examples/` 等）保持 unstaged，未 commit。

### M2 完成（2026-10-26）

**libs 抽取（双轨保持）**

- `harness/src/error.rs` → `libs/pxl-contract/src/error.rs`（`HarnessError` + 冻结退出码表，与 M0 的 `Verdict`/`step.json` 信封同库）。
- `harness/src/{digest.rs,storage/,qa/}` → `libs/pxl-store`（+ `VERSION` 常量替代原 `crate::VERSION`）。
- `harness/src/{agent/,policy/,config.rs,skills.rs,adapters/process.rs}` → `libs/pxl-agent`（process 成顶层模块 `pxl_agent::process`）。
- `harness/src/adapters/pxl.rs` → **`libs/pxl-agent/src/pxl_adapter.rs` 整体移入**（与 plan §7.2 的"瘦身后进 step-200"有偏差：`ToolContext` 类型上持有 `Arc<PxlAdapter>`，适配器必须在 agent lib 里；step-200 只消费其只读子集 `list`/`schema`，完整版供 harness 双轨，M6 删 harness 时再裁剪）。新增 `bin_path()` 访问器；`which_pxl` 的 target 探测改为 `../../target`（crate 移到 libs/ 后层级 +1）。
- `harness/roles/` 与 `harness/skills/` 随 `role.rs`（`include_str!` 编译期绑定）移到 `libs/pxl-agent/{roles,skills}/`。与 plan §7.2 的"roles 分到 step-100/200"偏差：角色 prompt 与 agent 内核编译耦合，M2 先整体随库；step-200 经 `artist_for(lang)` 使用。
- harness 的 `lib.rs`/`adapters/mod.rs` 改为 re-export（`crate::agent`、`crate::error`、`crate::adapters::pxl` 等路径不变），剩余 stage（design/build/playtest/runner/cli）零改动继续工作。
- 分层：`pxl-contract ← pxl-store ← pxl-agent`，无环；workspace `[workspace.dependencies]` 登记三个 path 依赖。

**step-200-art-spec 落地**

- 新 crate 完整实现（不再是 M0 骨架）：`run` / `lint` / `capabilities` / `status` 四个子命令。
- `run`：读 `asset-requirements.json`（唯一硬输入）→ artist 角色（`pxl_agent::agent::role::artist_for`）+ `AgentRunner` 工具循环 → 从磁盘实况组装 `spec-manifest.json`（对齐 `art-spec/v1`）→ 写 `step.json` 信封（`pxl-contract` 的 `StepEnvelope`，`verdict` 由 gates 推导：缺失资产强制 `NOT_ASSESSED`）。`--scope slice` 读 `pipeline.json.slice.assets`；`--asset` 过滤；`--provider fake|openai`（fake 经 `STEP_200_FAKE_TURNS` 脚本，openai 经 `--env` .env）；缺失资产 → 退出码 `11`（blocked），单资产失败不中断。
- 工具面 6 个，**无任何构建工具**：`assets.read_requirements` / `read_style` / `write_candidate`（路径白名单：根级仅 `palette.json`，子目录仅 `.json|.svg|.txt`，拒绝 `..`、绝对路径、越层） / `list_capabilities` / `schema`（两者仅经 `PxlAdapter` 调 `pxl --json list|schema`，即 step-300 双 bin 的只读面）。
- `lint`：spec-manifest 逐文件存在 + digest 复核 + 调 `step-300 plan --dry-run`（唯一 step 级调用，零落盘）；报告写 `reports/art-spec.lint.json`。
- events/budget 落在 `<out>/../_runs/<out-name>/`（目录随输出唯一化，杜绝并发共享 append-only 日志——迁移后首轮测试暴露的教训）。
- 测试：lib 单测 2（路径白名单、**注册表无 build 工具**）+ 集成 5（fake 全量产出 + envelope PASS；缺失资产 `NOT_ASSESSED`+exit 11 不中断；`--scope slice` 过滤；lint 经真实 `step-300 plan --dry-run` 全绿且零 PNG 产出；调 build 工具 → unknown tool 存活且无像素产物）。
- 修两个迁移暴露的既有测试缺陷：`pxl_adapter` 测试的 examples 路径少一层 parent；`policy/path` 测试的 temp 目录以 pid 命名导致同进程并发互删（改为按测试名键控）。

**M2 验收实测**

- `rg 'auto_build|auto_check_pass|build_report' steps/step-200-art-spec/src` **零命中**；
- step-200 的 pxl 调用面源码级确认：仅 `list` / `schema`（工具）+ `plan --dry-run`（lint，零落盘）；
- 删 `work/300-build` 不影响 step-200（代码无该路径引用，集成测试全程在无 300-build 的 temp 目录通过）；
- 双轨：`cargo test --workspace` **54 套件全绿、571 passed / 0 failed**（含 harness e2e、golden 58 条、pxl-agent 66 lib 测试、step-200 7 测试）；
- git：288 个路径已 stage，未 commit。

### M3 完成（2026-10-26）

**step-100-design 落地（不再是 M0 骨架）**

- 子命令全套：`init` / `run` / `gate` / `approve` / `status`（`migrate` 留 M5 占位）。
- 吸收三块 harness 实现（复制适配，harness 双轨保留至 M6）：
  - `contracts.rs`（479 行）= `harness/src/contracts/{mod,design}.rs` 合并——Brief/GameDesign/StyleGuide/LevelDef/AssetRequirements/AcceptancePlan/DesignReview + `validate_ids`；
  - `gates.rs`（423 行）= `design_stage.rs` 的 gates 主体（契约解析/id 引用/关卡网格/出生点-出口可达性/资产覆盖/语义检查），签名从 `&ToolContext` 改为 `&Path` 使 `gate` 子命令零模型零 run-store；
  - `review.rs`（339 行）= `review_view.rs`，Markdown 输出改到 `reports/design-review.md`。
- `templates/top_down_tank/` 复制入 crate（harness 原位保留，`read_template` 只认 step-100 内置根）。
- `init`：game_id 校验 `^[a-z][a-z0-9-]{1,31}$`（对齐 brief/pipeline schema）、genre 必须对应内置模板；产出 `brief.snapshot.json`（notes 原文逐字）+ `template.json` + `pipeline.json`（5 step 版本锁 + slice 透传）+ `step.json`；二次 init 未加 `--overwrite` → 退出码 `6`（按 plan §5.4 的 pipeline 表，而非 harness 表的 4）。
- `run`：designer 角色（`designer_for(lang)`）+ 5 工具（`read_brief` / `read_template` / `write_candidate`（DESIGN_FILES 白名单 + JSON 预校验）/ `validate` / `request_review`）；**零 pxl 调用**（ToolContext.pxl 用 unavailable 占位，单测断言注册表无 build/pxl 工具）。`--until draft` → `status: draft` + `NOT_ASSESSED` + 退出码 0；gates 通过 → `revision.manifest.json`（DESIGN_FILES ∪ brief.snapshot ∪ review.md 的排序 digest 聚合）+ `reports/design.gates.json` + review.md → `waiting_approval` + 退出码 `10`；gates 失败 → 退出码 `12`。
- `approve --digest`：实时重算 manifest 聚合比对——错误 digest `15`，正确写 `approval.json`；`gate` 在 approval 存在时复核实时 digest，篡改即 `15`（integrity）。
- 测试：吸收单测 3（stable_ids / review 渲染 / 无 build 工具）+ 集成 5（init 锁 pipeline + 幂等 + 坏 id/genre 退出码 3/4；fake run → exit 10 + 全产物 + envelope PASS；approve 绑定 digest + 错 digest 15 + 篡改后 gate 15；--until draft → 0/draft/NOT_ASSESSED；gate 零模型）。

**契约断层修复（M3 引入回归的当场对齐）**

- M0 的 `design.schema.json` 草案（`assets[]`/`id`）与吸收的 harness `AssetRequirements`（`requirements[]`/`asset_id`/`strategy`）不一致，step-200 读不懂 step-100 产物。**向已验证实现靠拢**：schema 重写为 harness 结构（`requirements[].asset_id/kind(sprite|tile)/strategy(shapes|tile_gen|svg|ascii|parts)/anims/consumer_refs`），step-200 的读取/过滤/组装同步适配（`strategy` 映射 spec 方言：`tile_gen`→shapes），fixture 更新。
- 设计决策：harness 契约是被 gates + e2e 验证过的实现，M0 草案是纸面设计；冲突时实现胜出，草案改写。

**M3 验收实测**

- step-100 测试 **8/8**（lib 3 + cli 5）；
- fake e2e → 全部设计 JSON + gates `passed` + 退出码 `10` ✓；篡改一字节 → `gate` 退出码 `15` ✓；`--until draft` → `0` + `status: draft` ✓；
- **三 step 实链打通**：`step-100 init → run(10) → approve(0) → step-200 run --scope slice（spec-manifest 1 ok，envelope PASS）→ step-300 plan（index ok）`——100 的产物被 200 消费、200 的产物被 300 构建，全部经文件契约；
- 双轨全量：**55 套件、579 passed / 0 failed**（harness e2e 与 golden 不动）；
- git：317 个路径已 stage，未 commit。

### M4 完成（2026-10-27）

**step-400-package 落地**

- 四个子命令：`run` / `check` / `approve` / `status`。**零像素解码**（只读文件、算 digest、比对），符合 plan §4.5 的硬规则。
- 吸收映射（§7.2）：`assets_stage.rs` 的 gates（`frame_index` / `png_ok` / `find_sheets` / `rules_for_sheet`）收敛为「build manifest 声明的每个文件是否存在且 digest 一致」；`asset_queue.rs::input_fingerprint` 思想落成 `revision.manifest.json` 的有序聚合。
- `run`：读 `index.json`（step-300 plan 产出）+ `asset-requirements.json`（design/v1）；逐 recipe 读 `.build.json` 的 `outputs{}`/`qa.ok` 做内容寻址；覆盖率 = requirements 的 `asset_id` ↔ 各 build 的 `<sprite>.pxl.json`；产出 `asset-manifest.json` / `coverage.json` / `asset-package.json`（引擎唯一入口）/ `revision.manifest.json`（三态 `pending|approved|needs_revision`）/ `publish-request.json` / `attempts.jsonl` / `step.json`。
- **裁决分层（按 M4 验收语义定）**：缺需求资产 → `NOT_ASSESSED` + `11`；build 声明的文件缺失（交付缺口）→ `NOT_ASSESSED` + `11`；文件字节与 manifest digest 不符（篡改）→ `FAIL` + `15`；额外产物 → 记 `coverage.extra`，不阻断。
- `check`：重算 `asset-package.json` 引用的 palette/sheets/levels 的 digest + revision 有序聚合；篡改 `15`、缺失 `11`。
- `approve`：与 step-100 同构的 digest 绑定（错误 `15`），成功后 `revision.manifest.status = approved` + `approval.json`。
- `attempts.jsonl`：失败 recipe（如 battle-city 的 `probe`）与缺失资产逐行记录 `{asset, issue, result}`——为后续重试回路留出「有记忆的失败」（plan §6 引用 19）。
- 修两处有序聚合陷阱：`publish-request.json` 会在 revision 之后写入，必须排除出文件集；`digest_pairs` 有序，写入与重算必须同样 `sort()`。

**M4 验收实测**

- step-400 测试 **7/7**（有效包 → 0/PASS/pending；缺需求资产 → 11 + `coverage.missing`；删 build 产物 → 11；改一字节 → 15；run 幂等 + check 通过；打包后篡改 → check 15、删除 → check 11；approve 绑定 digest）。
- **真实数据（battle-city）**：`step-300 plan` 15 recipe（14 ok / `probe` 失败，存量 palette cycle 缺陷）→ `step-400 run` → **coverage 13/13、missing 空、exit 0**；`asset-package.json` 含 14 sheets / 13 assets（`player_tank` animated，anims `fire/idle/move`）；`revision.status = pending`；`attempts.jsonl` 记录 `probe` 的 `unknown palette cycle 'tread'`；二次 run `asset-package.json` sha256 完全一致（幂等）；`check` 15 项全过。
- 双轨全量：**56 套件、586 passed / 0 failed**。
- git：319 个路径已 stage，未 commit。

### M5 完成（2026-10-27）

**step-500-engine 落地**

- `engine.json` 四件套（吸收 gamestudio-subagents 的 `engine_configs` 模式）：`project_structure`（目录映射）/ `naming_conventions` / `required_files` / `platforms` + `limits`。
- `check` 五项断言：`sheet_files_exist`（文件 + digest，缺失 11 / 篡改 5）、`palette_index_in_range`（颜色非空、≤ 引擎上限；像素级索引由 step-400 的构建保证）、`anims_in_frame_tags`（`meta.frameTags` **数组**（`{name}`）或旧 sheet 的 `<sprite>_<anim>_<dir>_<i>` 键推断）、`level_tiles_in_sheets`（只校验 `blocks=true` 或非 floor 的 tile；floor 允许引擎纯色填充）、`engine_limits_respected`（sheet 有帧、包声明上限不超引擎）。
- `slice` 四值裁决：四份 `.slice/` 产物缺任一 → `NOT_ASSESSED`（11）；`repair_rounds > 3` → `PIVOT`（5）；engine-check 与 300 index 全绿 → `PROCEED`（0）；`KILL` 保留人工写（不自动判定）。`--scope slice` 让切片包只承诺切片资产：levels 里切片未交付的 tile 记 `warning` 而非 fail（量产补齐）。
- `build` / `playtest` 维持占位（plan §11）。

**M4 收尾修复（step-500 断言的前置）**

- `asset-package.json` 的 `sheets[].frames` 从恒 0 改为读 `sheet.json` 的实际帧数，并新增 `frame_tags`。
- `levels` 改为**复制进包**（`<out>/levels/`），包自包含，引擎拿一个目录即可。
- package 的 sheet/palette 路径改为 `relative_from(package, file)` 真实相对计算（原实现用 `--in` 的目录名，slice 的 `300-build/.slice` 会算出 `../.slice/...` 错路径）。
- `step-400 run` 新增 `--scope all|slice` + `--pipeline`：切片只要求 `slice.assets` 内的资产。

**step-100 migrate**

- `migrate --from <legacy project> --out <work root>`：最新 `revisions/design/<id>` → `work/100-design`（补 `pipeline.json` + `step.json`）、旧 `revisions/assets` → `work/legacy-assets`（原样归档，旧格式无 spec-manifest 不可机转）、`runs/` → `work/_runs`。幂等（已存在文件跳过）。

**M5 验收实测**

- step-500 测试 **8/8**（5 项断言正反例 + slice 三值 + 缺证据不通过）。
- **真实数据（battle-city）**：`step-400 run --levels <design>/levels` → `step-500 check` **5/5 pass、exit 0**（package `battle-city@aba56f032744`，14 sheets / 13 assets）。
- 迁移 battle-city：design 12 文件 + legacy assets 88 文件 + runs 归档；二次运行复制 0 文件（幂等）。
- 双轨全量：**57 套件、586+ passed**（此时含 harness）。

### M6 完成（2026-10-27）

- **`pxl` bin 退役**：`pxl-cli` 只保留 `[[bin]] step-300`；`which_pxl` 改为优先 `step-300`、回退 `pxl`（兼容旧 target）；测试改用 `CARGO_BIN_EXE_step-300`（连字符原名）。
- `build.sh` / `scripts/gen-pxlgen.sh` / `scripts/tools/v3_make.py` / `scripts/skills/pxl/**` 的 `pxl` 命令统一改 `step-300`；`cargo -p pxl-cli` 保留（package 名未变）。
- `README.md` 重写 Layout（5 大 step + libs + step-300 内部工位表）与全部命令示例。
- **`harness/` 移出 workspace** 并从 `.gitignore` 屏蔽搜索；**物理删除因审批不可用未能执行**——磁盘保留只读副本，用户可执行 `rm -rf harness` 完成（git 历史可恢复）。`harness` 的 `build_stage` / `playtest_stage` 实现随目录退役，`step-500 build/playtest` 按占位重建；`runtime/replay.rs` 未受影响。
- 新增 `steps/step-500-engine/tests/pipeline.rs`：**全链 e2e**（fake provider，1.9s）——
  A. step-100 init → run(10) → approve(0)；
  B. 切片回路：200 `--scope slice` → 300 plan → 400 `--scope slice` → 500 check `--scope slice` → 500 slice **PROCEED**；
  C. 量产：200 全量 → 300 → 400 → 500 check **5/5**。
- 修 fixture 三处真实契约问题（均由 step-500 断言暴露）：recipe 未声明 anims → 补 `RecipeAnim.frames`；两帧内容相同 → `duplicate` QA 规则失败 → 第二帧加 `shift`；动画单帧 → `static_single_frame` 失败 → 2 帧。

**M6 验收实测**

- `cargo test --workspace`：**53 套件、556 passed / 0 failed**；
- `rg 'pxl_harness'`：代码零命中（仅本 plan 的历史记录提一次）；
- 全链 e2e：**1.9s**（< 3 min 要求）；
- golden 58 条保持；`step-300 build examples/tank` 与旧 `pxl build` 逐字节一致（M1 起持续验证）。

### 遗留与后续

| # | 事项 | 处理 |
|---|---|---|
| L1 | `harness/` 物理目录仍在磁盘（已移出 workspace + gitignore） | 用户执行 `rm -rf harness` |
| L2 | `step-500 build` / `playtest` 占位 | 按 plan §4.5/§4.8 重建（runtime/replay.rs 保留） |
| L3 | `scripts/migrate-v3-workspace.py` 未建（Rust `step-100 migrate` 已可用） | 脚本仅是批量包装，可后补 |
| L4 | battle-city `probe` recipe 的 palette cycle `tread` | 归 step-200 的 artist 修复回路（真 provider 接入后处理） |
| L5 | `steps/step-200-art-spec/tests/fixtures/fake_turns_slice.json` | e2e 已改用 `fake_turns_full` + `--scope slice`，该文件可清理 |
