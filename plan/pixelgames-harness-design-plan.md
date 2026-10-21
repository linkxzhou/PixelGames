# PixelGames Harness 设计与实施计划

> 日期：2026-10-21  
> 状态：待实施的工程设计；本次仅新增计划文档，不创建 Harness 实现。  
> 实现位置：`harness/`；建议 Rust package / binary 均命名为 `pxl-harness`。  
> 优先级：**游戏设计与关卡设计 → 素材生成闭环**为第一阶段交付；引擎装配、GM 自动试玩和完整游戏 QA 后续接入。

## 1. 目标与关键决策

### 1.1 要解决的问题

当前 `pxl-cli` 已能根据确定性配方生成像素素材，但缺少将用户创意转化为游戏设计、再将设计转化为完整素材包的 Agent。Harness 不应只是调用几条 CLI，也不应首先投入自研游戏引擎，而应承担：

1. 把自然语言需求变成明确、可验证、可修改的游戏设计包。
2. 将玩法实体、关卡对象、UI 等需求转换成稳定的素材清单。
3. 驱动 Agent 编写素材配方与 DSL，通过 `pxl` 构建、检查、修复和发布素材包。
4. 管理阶段依赖、预算、权限、重试、断点恢复和证据，避免 Agent 自行宣布“完成”。
5. 后续把通过门禁的设计包与素材包装配到开源引擎，通过 GM 接口试玩并生成 QA 报告。

完整目标链路：**Brief → Design Package → Asset Package → Playable Build → GM Playtest → QA Report**。QA 发现问题后，按问题归属回到设计、素材或引擎阶段，而不是全部重新生成。

### 1.2 架构决策

| 决策 | 方案 | 理由 |
|---|---|---|
| 工程归属 | 在 `harness/` 新建独立 Rust package，实施时加入根 workspace | 保持素材库与游戏编排解耦；不把整个 Agent 塞进 `pxl-cli` |
| Agent 内核 | 参考 `miniagent`，优先独立实现最小执行内核 | 需要可控的工具、预算、阶段提交和领域契约；不依赖外置 REPL/daemon |
| 代码复制 | 明确许可证、来源版本和 notice 后，才选择性移植 | 当前第三方副本未找到明确许可证，不能默认继承仓库根 MIT |
| 首期编排 | 一个协调器 + 设计/素材角色配置 + 确定性状态机 | 不先做多 Agent 集群、通用 DAG DSL 或后台调度平台 |
| 素材接入 | 类型化 `PxlAdapter` 包装现有 CLI 子进程 | 立即复用已有生成、QA、导出；保留日后库内调用选项 |
| 数据契约 | Harness 自有版本化 JSON 契约；Markdown 为便于审阅的视图 | `pxl-format` 继续负责像素文档与 recipe，不承担 GDD/关卡/QA 全部领域 |
| 引擎 | 后续优先验证 Godot 4.x 单一适配器 | 利用现成 2D、文本资源和 headless 能力，不以新建 `pxl-sim` 为前提 |
| 自动试玩 | 引擎适配器提供版本化 GM 协议；脚本 bot 为基线，LLM 做策略探索 | 调试捷径与正常玩家行为分开，避免“GM 通关”伪装成可玩性验收 |
| 完成判定 | 程序门禁与产物提交决定状态，LLM 只提案和解释 | 防止只生成文档、只输出成功文本或放宽 QA 即被认定完成 |

### 1.3 与原调研方案的关系

本计划是 [游戏开发管线调研](game-dev-pipeline-harness-plan.md) 的收敛实施方案，不替换其背景研究：

- 将首期从“自研运行时并打通完整游戏”调整为“设计包 + 素材包的 Agent 闭环”。
- 将关卡设计前移到设计阶段，先产出逻辑地图与玩法约束；最终美术映射在素材完成后绑定。
- 将外置 `miniagent` 驱动改为 Harness 内置 Agent 执行能力；`miniagent` 是参考来源，不是运行时强依赖。
- 保留阶段门禁、输入哈希、证据归档和 GM 回放思想，但不把计划中的命令当作现有能力。
- 暂不扩展 `pxl-format` 的游戏 schema、不实现 `pxl gen level`、不新增 `pxl-sim`，不把 MCP 作为首期前置条件。

非目标：开放世界、任意品类一次成功、多模型协作平台、在线运营、自动发布商店、完整音乐/语音生成、自由修改任意引擎源码。

## 2. 当前代码事实与可复用边界

以下来自本次仓库静态检查；不代表本次已运行测试或证明生产可靠性。已有调研文档中的外部项目热度数据不作为选型依据。

### 2.1 PixelGames

| 已有内容 | 可用于 Harness | 必须补齐的部分 |
|---|---|---|
| 根 workspace 的七个素材 crate | 复用 `serde`、`serde_json`、`schemars`、`sha2` 和像素类型 | 当前没有 Harness、游戏运行时或 GM 工程，`harness/` 为空 |
| `pxl build` | recipe 内多个 sprite/tile 的生成、动画、图集、预览、校验 | 不包含游戏素材需求拆解、任务调度或事务发布 |
| `gen`、`op`、`anim`、`palette`、`tile gen` | Agent 局部调试、生成与修复 | 没有自动 `repair`；修复策略由 Harness 提供 |
| `sheet`、`render`、`inspect`、`validate` | 图集导出、PNG/GIF 预览、矩阵读取和像素 QA | 不能代替游戏关卡、碰撞、胜负和软锁验证 |
| `list`、`schema` 与 `--json` | 工具能力发现、参数契约和错误反馈 | `schema` 返回裸 schema；不是所有输出都带 `ok` |
| 素材集成/golden 测试 | adapter 契约测试与复现基线 | 没有 Agent 成功率、GM 回放和游戏 QA 测试 |

重要约束：

- 实际命令为 `pxl op`、`pxl sheet`、`pxl render`；不存在 `pxl ops`、`pxl export`、`pxl harness`。
- `pxl mcp` 与 `tile autotile` 目前是未实现入口，不可作为依赖。
- `build` 会先写输出再执行最终 QA，失败时可能已有产物，内部部分路径会覆盖文件。必须每次使用独立 staging 目录。
- `build.hash` 只代表 sheet PNG，不能作为整套素材或配方依赖的内容哈希。
- `--strict` 不是统一的全命令严格门禁；Harness 应自行处理各命令的 warnings 和 checks。
- 一份 recipe 共用 palette，同 sheet 的颜色表必须一致。首期优先全游戏单一调色板，多调色板须按 atlas 分组。
- 文本像素 DSL 单帧宽高为 1–64、最多 36 个索引；首期规格不能要求单个 DSL 帧直接覆盖大地图。地图由 tile/对象组合。
- 现有 atlas pivot 是从首帧尺寸推导的中心，不能直接作为每个 gameplay entity 的碰撞体或武器挂点。

代码依据：[workspace](../Cargo.toml)、[CLI 入口与子命令](../crates/pxl-cli/src/lib.rs)、[命令实现](../crates/pxl-cli/src/cmd.rs)、[recipe](../crates/pxl-format/src/recipe.rs)、[像素模型](../crates/pxl-format/src/model.rs)、[文本 DSL](../crates/pxl-core/src/text.rs)、[CLI 测试](../crates/pxl-cli/tests/cli.rs)。

### 2.2 miniagent

| 模块 | 已实现能力 | 复用方式 | 不直接继承的保证 |
|---|---|---|---|
| `core/engine.rs` | LLM/tool 循环、事件、压缩、停止条件、完成 gate | 参考循环与事件分层，抽离领域无关设计 | 不把文本伪工具调用自动升级为真实执行；不沿用工具名硬编码 |
| `core/llm_client.rs` | OpenAI-compatible 请求、重试、超时、usage | 新建 `ModelProvider`，参考调用与错误分类 | 移除专用网络探测/fallback；不默认支持流式、多模态或严格 schema |
| `core/tools.rs` | 工具定义、schema、执行结果 | 增加类型化结果、产物引用、执行成本和权限声明 | 字符串结果不足以完成素材验收 |
| `core/store.rs` | 会话消息与元信息落盘/重载 | 借鉴追加日志与会话标识 | 不等于工具副作用恢复、事务提交或精确续跑 |
| `goal/` | 目标、验收条件、续跑、评估 | 借鉴目标状态与失败分类 | 对话评估/token 增长不等于真实产物进度或独立 QA |
| `core/permissions.rs` | Ask/Auto/Readonly 审批 | 借鉴审批入口 | 不是文件沙箱；只读 hint 不能代替本地权限判断 |
| `skill.rs` | prompt、工具名单、步数等配置 | 收敛为版本化角色配置与知识模板 | Skill 不得扩大宿主权限和预算 |
| `mcp/` | 多种 transport 和工具 bridge | 后续可选 adapter | 不复制整套自写协议栈作为第一期基础设施 |
| `daemon/`、`repl.rs` | 后台任务、调度、交互终端 | 首期不移植 | 无需为素材闭环引入守护进程和 cron |

限制：当前副本未找到 `Cargo.toml` 或明确授权文件；源码实际使用 `ureq`，不能依赖“零外部依赖”的描述。复制前需确认上游版本、许可证、依赖版本、feature 与测试。

默认路径：**先独立实现小内核；若授权后决定移植，逐模块记录来源并通过 Harness 契约测试，不整体拷贝 `thirdparty/miniagent/rust/src`。**

## 3. 系统分层与工程布局

### 3.1 分层职责

| 层 | 核心组件 | 职责 |
|---|---|---|
| 入口 | `HarnessCli`、配置加载 | 接收 brief、选择范围、审批、查看状态与恢复 |
| 编排 | `PipelineRunner`、`StageRunner` | 决定阶段、依赖、重试、失效传播和正式提交 |
| Agent | `AgentRunner`、`ModelProvider`、`RoleProfile` | 理解需求、调用受限工具、提出设计/素材修订 |
| 领域 | `DesignService`、`AssetPlanner`、`AssetBuilder` | 维护设计与素材契约、跨文件引用和修复归属 |
| 工具 | `ToolRegistry`、`PxlAdapter`、后续 `EngineAdapter`/`GmClient` | 执行类型化操作，不接受任意 shell 字符串 |
| 验证 | `GateRunner`、后续 `QaRunner` | 独立检查真实产物，生成不可由 Agent 直接改写的报告 |
| 基础设施 | `RunStore`、`ArtifactStore`、`BudgetLedger`、`Policy` | 事件、持久化、哈希、预算、权限和日志脱敏 |

边界：Agent 决定“如何产生候选产物”，编排器决定“是否允许执行/重试”，门禁决定“候选产物是否可提交”。Agent 不持有修改自身门禁或直接置阶段成功的工具。

### 3.2 建议目录

以下均为未来实施布局，本次不创建。

| 路径 | 内容 |
|---|---|
| `harness/Cargo.toml` | 单一 `pxl-harness` package，首期避免拆成大量 crate |
| `harness/src/main.rs`、`lib.rs`、`cli.rs`、`config.rs` | CLI、库入口与配置 |
| `harness/src/agent/` | provider、loop、message、tool、role、context |
| `harness/src/pipeline/` | runner、stage、state、invalidation |
| `harness/src/design/` | brief、game、level、style、design gates |
| `harness/src/assets/` | requirements、planner、recipe binding、manifest、asset gates |
| `harness/src/adapters/` | process、pxl；后续 engine/gm |
| `harness/src/storage/` | run store、event log、artifact store、commit |
| `harness/src/policy/` | permissions、budget、approval |
| `harness/src/qa/` | 通用 gate report；后续 playtest/report |
| `harness/roles/` | designer、artist 的版本化 prompt 与允许工具；后续 builder、tester |
| `harness/templates/` | 首个品类的设计/关卡/素材模板；后续 Godot 模板 |
| `harness/schemas/` | 从 Rust 契约生成并版本化的 schema 快照 |
| `harness/tests/` | fixture、mock provider、CLI、恢复、素材闭环测试 |

依赖策略：复用 workspace 的序列化/schema/hash/CLI 依赖；HTTP、JSON Schema 验证器和进程控制库在实施时选定兼容版本并锁入 `Cargo.lock`。`schemars` 负责生成 schema，不等于运行时验证器。不搬自写 JSON parser，也不直接依赖第三方缺失 manifest 的工程。

## 4. 第一核心阶段：游戏设计与关卡设计

### 4.1 输入与范围约束

输入为用户 brief，可附品类、目标平台、风格参考、必须实现/明确排除内容。默认建议首个样板为**俯视角坦克单屏切片**，复用现有素材样例，但设计接口不绑定坦克。

未指定内容使用明确记录的默认值：单机、单屏、键盘输入、单 palette、16×16 tile、少量实体与 3 个逻辑关卡。涉及品类、核心玩法或显著扩大范围的矛盾标记为 `needs_input`，不以臆造需求掩盖冲突。默认值与用户显式要求分别记录来源。

### 4.2 设计包契约

| 产物 | 关键字段/内容 | 后续用途 |
|---|---|---|
| `brief.json` | 原始需求、约束、假设、排除项、目标范围 | 保留用户意图和审批依据 |
| `game-design.json` | `schema_version`、`game_id`、genre、core_loop、actions、entities、rules、win/lose、scope、requirement_ids | 玩法与实体的结构化事实源 |
| `GDD.md` | 玩法概述、规则解释、设计理由、范围、已知风险 | 人工阅读；规则必须对应 JSON 中的稳定 ID |
| `style-guide.json` | 逻辑分辨率、tile/sprite 尺寸、palette、透明索引、轮廓、方向、动画约束、nearest filtering | 对素材生产建立统一约束 |
| `tuning.json` | 参数 ID、单位、默认值、范围、引用规则 | 后续运行时/难度验证，避免参数只藏在文案中 |
| `levels/<level_id>.json` | grid、tile size、逻辑 tile、实体 placement、spawn/exit、目标、trigger、difficulty、seed | 不依赖实际图集的逻辑地图 |
| `level-design.md` | 每关教学目标、挑战组合、关键路径和预期体验 | 便于审阅和关卡迭代 |
| `asset-requirements.json` | 稳定 asset ID、用途、消费实体/关卡、尺寸、方向、动画、优先级、必要/可选 | 素材阶段的唯一需求入口 |
| `acceptance-plan.json` | requirement → gate/test、预期结果、证据类型、所属阶段 | 防止完成后临时降低验收目标 |
| `design-review.json` | 硬门禁、软评审、风险、待确认项及审批状态 | 决定设计包是否可发布 |

JSON 为规则事实源；Markdown 可包含额外理由，但不能定义只存在于文案的可执行规则。修改 Markdown 中涉及玩法的内容必须形成 JSON 修订提案，并重新生成一致视图。

### 4.3 Agent 执行流程

1. 归纳 brief，识别范围、冲突和缺失条件，保存假设。
2. 选择品类能力模板，检查当前素材与后续引擎模板是否能够表达该玩法。
3. 生成玩法设计、实体/动作/规则与胜负条件，显式描述状态迁移。
4. 生成逻辑关卡：先教学，再组合挑战，再完成目标；地图中的 tile/entity 均用稳定 ID 引用。
5. 生成 style guide 和素材需求，不先画图再倒推玩法。
6. 将玩法目标转换成验收要求；标记设计阶段可验证与必须等引擎接入的条目。
7. 执行设计硬门禁；Agent 根据结构化问题定点修复，默认最多 3 次修复，不无限重写。
8. 输出设计审阅包；通过程序门禁且获得所需审批后，提交不可变设计 revision。

首期一个 designer 角色可依次完成 GDD、关卡和美术需求；无需三个并发 Agent 同时写同一设计。

### 4.4 设计门禁

**硬门禁：**

- schema、枚举、ID 唯一性、引用闭合、尺寸/范围预算合法。
- 必要动作有规则，胜负条件引用有效状态，不允许只写“体验良好”。
- 关卡网格尺寸正确，出生点/目标点在界内，必须对象存在，阻挡与出生位置不冲突。
- 俯视角静态网格可进行 flood-fill/A* 可达性检查；动态机关、敌人行为和战斗可解性标记为后续验证，不能由静态通路检查代替。
- 每个可见实体、逻辑 tile 和必要 UI 都能关联素材需求；允许复用同一 asset，但关联必须显式。
- 动画名、方向数、尺寸、颜色数可被当前素材路径表达，超出能力时阻塞或请求缩减范围。
- 每个必要需求至少对应一个验收项，并写明在哪个阶段验证。

**软评审：**玩法一致性、教学节奏、难度曲线、辨识度和风格合理性。LLM 意见只提供理由和风险；“好玩”不能被 schema 或单一评分证明。

平台跳跃等后续品类需要专属运动模型/引擎验证，不能将网格连通直接视为跳跃可达。

### 4.5 设计变更与审批

默认在设计冻结前进行一次人工审阅，审阅对象是 revision digest，而不是一段聊天。批准或拒绝需绑定 `game_id + design_revision + digest`。

无人值守运行可使用用户预先批准的品类/范围策略，但 Agent 不能自行授予批准。核心规则、palette、tile size 或必要素材范围变化使旧批准失效；不改变语义的 Markdown 排版变化可不触发全量重建。

## 5. 第二核心阶段：素材 Agent 生产闭环

### 5.1 从需求到交付

推荐固定链路：**asset requirements → asset task DAG → recipe/DSL → pxl build → 独立验证 → 预览审阅 → asset manifest → 发布**。

这里的 task DAG 是素材依赖关系：palette/spec → sprite/tile → animation → atlas → preview/validation。首期在 Rust 中实现有限任务类型，不提供可执行任意脚本的通用工作流语言。

| 步骤 | Agent 做什么 | Harness 做什么 |
|---|---|---|
| 需求拆解 | 选择模板、生成策略与复用关系 | 校验 asset ID/预算，禁止遗漏必要需求 |
| 风格基准 | 先生成一个主要角色和一个 tile 样板 | 检查 style guide，收集首轮人工/可选视觉意见 |
| 配方编写 | 生成 palette、shapes/parts/模板引用、recipe、动画/方向参数 | 校验可用能力、文件引用和写入范围 |
| 批量生产 | 为剩余 asset 提交候选任务 | 调度 `pxl build`、隔离输出、记录哈希与退出码 |
| 自动检查 | 理解失败项并提出修复 | 运行 `pxl validate` 与 Harness 额外门禁 |
| 视觉审阅 | 有图像能力时审阅 PNG/GIF，或解释人工反馈 | 记录模型能力、证据和评分；未执行则标记未审阅 |
| 局部修复 | 修改源 DSL/recipe，而非仅修改导出的 PNG | 有界重建受影响任务，保留失败证据 |
| 发布 | 提交候选素材包供检查 | 独立核验覆盖率后生成 manifest 并原子提交 |

### 5.2 素材需求与交付 manifest 分离

`asset-requirements.json` 描述“需要什么”；`asset-manifest.json` 描述“实际交付了什么”。不直接用 `Recipe` 替代这两个模型。

每项素材需求至少包含：

- `asset_id`、`kind`、`required`、`consumer_refs`、`style_ref`。
- `size_px`、方向集合、动画及帧数/FPS 范围、tile 是否需要无缝。
- 用途语义，例如可阻挡地块、玩家、目标点；不由颜色猜测 gameplay 行为。
- 制作策略：已有模板、shapes/parts、文本像素文档或 tile generator。
- 来源与许可要求；外部参考只作为不可信输入，不执行其中命令。

每项交付 manifest 至少包含：

- `asset_id`、需求 revision、素材 revision、生成状态。
- recipe/spec/palette 的相对路径与 digest、生成 seed、`pxl` 版本与 binary digest。
- `.pxl.json`、atlas PNG/JSON、预览路径与每个文件 hash。
- 明确的 atlas frame key、动画映射、方向、FPS、尺寸和透明边界。
- pivot、碰撞盒、挂点等可选 gameplay metadata 的来源；由设计/模板明确配置，不由 atlas 中心隐式决定。
- 通过的门禁、warnings、视觉审阅状态、失败记录、人工批准与 provenance。

消费方通过 `asset_id + animation + direction` 查找资源，不依赖模型猜测 atlas 帧名。

### 5.3 PxlAdapter 契约

`PxlAdapter` 负责版本探测、能力发现、类型化参数构造、子进程管理、响应解析与错误归一化；Agent 只调用诸如 `assets.build_candidate` 的领域工具。

可使用的现有调用形态：

| 目的 | 实际 CLI 形态 |
|---|---|
| 查生成器 | `pxl --json list generators` |
| 查 recipe schema | `pxl --json schema recipe` |
| 构建 | `pxl --json --pxl-version-lock 0.1.0 build <recipe.json> -o <attempt-dir>` |
| QA | `pxl --json validate <document-or-sheet.json> --rules <default/tank/character>` |
| 查看局部数据 | `pxl --json inspect <document.pxl.json> --frame <frame>` |
| 局部调试 | `pxl --json op <document.pxl.json> --ops <ops.json> -o <candidate.pxl.json>` |

上述 `<...>` 为参数占位符。实施时用 `Command` 参数数组，而不是拼接 shell。`--pxl-version-lock` 目前是前缀匹配，严格锁定还需独立检查版本和 binary digest。

实现要求：

1. 分别解析普通 `ok/error` 响应、`validate.checks`、`build.outputs` 与裸 schema，保留原始 stdout/stderr。
2. 同时检查进程退出码、JSON 结构、必要输出文件存在及哈希；退出 0 不等于素材包完整。
3. `--json` 与 `-o -` 不混用；全部图像/文档输出到文件，日志有大小上限。
4. 并发排空 stdout/stderr，支持超时与进程组清理，避免大输出阻塞或只终止父进程。
5. preflight 校验 recipe 引用、palette/spec/template、preview/sheet 输出路径；所有输入必须在项目或批准模板根内，输出必须位于该 attempt。
6. 不仅限制传入 `-o`：recipe 自带路径也可能越界。拒绝绝对写路径、`..` 越界和不受控符号链接；对自定义 recipe 使用受控 staging 输入副本。
7. `list/schema` 不完整时使用已验证的版本化能力表；不能编造 `shapes`/`game` schema 查询入口。
8. 仅执行已安装且批准的 `pxl` binary，不在运行中由 Agent 自动安装或替换工具。

错误策略：`E_IO(1)` 先识别环境/权限；`E_USAGE(2)`、`E_SCHEMA(3)`、`E_SEMANTIC(4)` 返回候选修订；`E_QA(5)` 返回检查项进行素材修复；`E_EXISTS(6)` 检查目录隔离/执行恢复，不默认打开全局覆盖。子进程崩溃、非 JSON、超时另归为基础设施错误。

### 5.4 素材硬门禁与视觉质量

- 所有 required asset 交付率 100%，无悬空帧、动画、方向或消费引用。
- 每个候选通过与自身适配的 `pxl validate` 规则；例如不能给缺少地块的单独 sprite 强行套完整 tank sheet 验证。
- Harness 额外检查规格尺寸、必需动画帧数、方向集合、palette 一致性、atlas 映射和文件完整性。
- schema 通过不代表版本、引用或语义通过；这些分别验证。
- 相同已冻结配方、全部依赖、seed 和工具版本，在干净输出目录重建，确定性产物 hash 一致。时间戳/运行 ID 等非确定字段不参与此比较。
- 大图场景采用 tile/对象拼装；音效、字体等当前 `pxl` 不支持的类型显式为外部依赖/后续事项，不能伪装成已生成。
- 程序 QA 通过不保证素材好看。VLM 是可选软评审，没有图像 provider 时必须显示 `not_run`，不能用模型读文件名来代替视觉检查。
- 若用户将视觉签字设为必要验收，缺失审阅进入 `waiting_approval`；否则保留风险提示，不能声称完成了视觉 QA。

### 5.5 修复、缓存与增量失效

- 默认每项任务允许初次生成 + 最多 3 次修复；同输入同错误重复出现时提前终止，转人工处理。
- CLI 调试产生的 `op` 若被接受，必须回写 recipe 的 post/方向/动画或源文档，并从干净目录复建；禁止只提交一次性生成物。
- 缓存键覆盖规范化输入、传递依赖、seed、生成器版本、binary digest 和 gate 版本。构建缓存与验证缓存分开，gate 改动只复验而非必然重画。
- 设计素材规格改变 → 失效对应素材及下游 atlas/引擎/试玩；palette 改变 → 失效同 palette 组；关卡布局变化但素材需求未变 → 保留素材缓存。
- 已冻结配方的再执行可确定性验证；同一自然语言 brief 重新调用 LLM 不承诺产物逐字或逐像素相同。

## 6. Agent 执行模型与工具边界

### 6.1 最小 Agent 内核

`AgentRunner` 接收阶段任务、角色配置、输入 revision、工具权限及预算，返回候选 artifact refs、问题或停止原因。流程固定为：

1. 从存储读取当前状态与必要摘要，按任务加载相关 schema/模板。
2. 调用 provider，获取文本或原生结构化 tool call。
3. 验证工具存在、arguments schema、宿主权限、预算与阶段状态。
4. 写入执行意图，调用工具，记录实际结果与 artifact digest。
5. 依据结果继续修复，或向阶段 runner 提交候选完成请求。
6. 阶段 runner 独立运行 gate；通过后提交，失败则返回结构化问题或停止。

provider 即便支持严格 JSON 输出，本地仍需反序列化/schema/语义校验。首期支持一个 OpenAI-compatible provider 和完全离线的 fake provider；模型地址/名称可配置，不写死内部路由。

上下文以结构化设计摘要、当前任务、最近失败和相关资产为主；不把全部像素矩阵、日志和会话永久追加到每次请求。长日志存文件，工具返回摘要与引用。

### 6.2 角色不是多个独立进程

| 角色 | 工具范围 | 不允许 |
|---|---|---|
| designer | 读取 brief/模板、写设计候选、验证设计、提交审阅 | 调用引擎 shell、改素材 gate、直接批准自己 |
| artist | 读取冻结设计、写素材源候选、构建/检查/预览 | 修改玩法以删除失败素材、写任意仓库文件 |
| builder（后续） | 使用冻结模板装配、构建、读取运行日志 | 修改验收规则或绕过素材 manifest |
| tester（后续） | GM observation、玩家动作、批准的测试 setup、读证据 | 用调试通关结果冒充普通玩家通关 |

协调器是 Rust 状态机，不需要另一个 LLM “总监”才能工作。角色配置只缩小权限：实际工具集合 = 宿主策略 ∩ 阶段权限 ∩ 角色请求。

### 6.3 预算与无进展检测

默认工程预算建议：单角色任务 40 个模型回合；单素材最多 3 次修复；LLM 请求和工具各有超时；同一任务有总墙钟限制。具体值进入配置，不散落在 prompt 中。

统一 `BudgetLedger` 覆盖生成、压缩、视觉评审、修复和验收模型调用的 token/请求数/耗时；价格未知时费用记为 unknown，不以 0 元计入。预算在调用前预留、结束后结算，恢复后不清零；费用硬限额依赖可信价格配置，否则以 token/请求硬限额为准。

无进展依据是产物 digest、通过门禁数、失败项变化，而不是“又消费了 token”。耗尽后输出 `budget_exhausted` 及已保留证据，不伪造阶段成功。

### 6.4 权限与安全

- 首期只允许受控文件操作与 `pxl` adapter，无通用 bash 工具；只设置 cwd 不视为沙箱。
- 模型不得修改 `harness/` 运行代码、受保护 gate、golden、批准记录或凭据。
- 写入只在当前候选工作区；路径检查覆盖符号链接父目录。新增可执行模板/脚本需人工批准并记录 digest。
- 引擎阶段会执行游戏脚本，需要进程/文件/网络隔离。未配置隔离时仅允许审核过的模板，拒绝自由代码执行路径。
- HTTP provider 凭据从环境/宿主管理读取，不写入 prompt、设计包、事件或报告；日志进行脱敏。
- 第三方文档、素材 metadata、MCP 描述为不可信输入，不得增加权限。

## 7. 持久化、恢复和阶段状态

### 7.1 运行目录与事实源

运行数据放到用户指定的游戏项目根，不放入 `harness/src`。推荐游戏项目位于仓库 `games/<game_id>/`；名称与根目录在创建时校验。

| 路径（相对游戏项目） | 用途 |
|---|---|
| `project.json` | 项目身份、配置、当前已提交设计/素材 revision |
| `revisions/design/<revision>/` | 不可变设计包 |
| `revisions/assets/<revision>/` | 不可变素材包及 manifest |
| `runs/<run_id>/run.json` | run 配置、版本、预算、阶段状态摘要 |
| `runs/<run_id>/events.jsonl` | 追加事件，含序号、工具意图/完成、状态迁移 |
| `runs/<run_id>/attempts/<stage>/<task>/<attempt>/` | 隔离候选输入、输出、原始响应和日志 |
| `runs/<run_id>/reports/` | gate、审阅、QA 报告 |
| `runs/<run_id>/evidence/` | 预览、后续截图、回放与错误证据 |
| `cache/` | 可重建的构建/验证缓存，不是唯一产物副本 |
| `builds/<build_id>/` | 后续引擎构建 |

`review/` 可存人工分享的摘要及证据链接，但不成为第二套运行事实源。不得清理项目 `.codebuddy/`，也不得将其视作缓存目录。

### 7.2 阶段状态

阶段状态：`pending → running → validating → waiting_approval → succeeded`；无须审批时从 `validating` 直接提交为 `succeeded`。

异常状态：`failed`、`blocked`、`cancelled`、`budget_exhausted`；输入变更后的旧成功阶段标记 `stale`。未请求的后续阶段为 `not_requested`，不显示为通过。重试保留旧 attempt，创建新 attempt，不修改历史结果。

首期固定阶段：`design`、`assets`；后续加入 `engine`、`playtest`、`qa`。局部素材任务可并行，但首次实现串行也可验收；不得并发写同一个 palette/atlas。

### 7.3 可恢复执行与提交

1. run 创建后立即持久化 run ID、配置、输入 digest 和预算，而非等待首次模型调用完成。
2. 每个工具任务分配稳定 task ID 与 attempt ID；副作用执行前持久化 `started` 意图。
3. 子进程只在 attempt staging 中写输出，完成后持久化结果、退出码、文件清单和 digest。
4. gate 全部满足且必要审批有效后，在同文件系统中原子发布 revision，并原子更新当前 revision 指针；关键写入执行 flush/fsync，传播 IO 错误。
5. 恢复时持项目写锁，检查运行版本、输入 digest、已提交 revision 和在途工具，不仅恢复聊天记录。
6. `started` 无结果的构建先对账：确认旧进程是否仍在运行，再核验文件；不得无条件重复执行或把残留文件认定为成功。无法证明完整则隔离旧 attempt 后重建。
7. checkpoint/日志损坏、哈希不符或授权策略变化时停止并报告，不能静默忽略写入错误。

保证目标是“可对账、可重试、正式发布原子化”，不是笼统宣称所有外部副作用 exactly-once。首期禁止自动发布、付款等不可逆外部动作。

## 8. 后续阶段：开源引擎装配

### 8.1 选型建议与验证前提

建议以 **Godot 4.x** 为第一个候选，具体小版本在引擎 spike 中锁定。选择依据是 2D 生态、文本资源、headless 入口与减少自研工作量，不是本次已经验证了集成。

| 候选 | 适用条件 | 本计划安排 |
|---|---|---|
| Godot | 桌面/Web 2D、文本场景、现成编辑器与调试能力 | 默认首个 spike |
| Phaser | 产品明确优先浏览器、团队更熟悉 TypeScript | 可替换的后续选择，不与 Godot 首期并行实现 |
| Bevy | 产品要求 Rust gameplay、团队愿意维护 ECS/工具集成 | 保留选项，不因为 Harness 用 Rust 就自动选它 |

引擎接入前必须验证：锁版本后的无头启动、PNG/atlas 导入、动画映射、nearest/pixel snapping、固定 tick 控制、GM 通信、退出/超时、截图路径。Headless 成功不能证明图形截图可用；必要时测试用渲染进程/Xvfb 或专用 runner 单独验证。

### 8.2 EngineAdapter

接口职责：`capabilities`、`materialize`、`build`、`launch`、`stop`、`collect_logs`。输入为冻结设计 revision、素材 revision 和受审模板版本；输出为 build manifest、运行入口、版本与错误报告。

首个实现采用“审核过的品类模板 + 数据注入”，不让 Agent 从空白自由生成任意 gameplay 脚本。Godot adapter 将 atlas frame rect/animation 转换为引擎资源；Aseprite JSON 可导出不代表引擎会自动正确导入。

逻辑地图保持引擎无关 JSON；之后可增加 LDtk/Tiled 导出，但未经 schema 校验不能声明兼容。引擎消费 tile/实体语义与显式碰撞定义，不从贴图像素颜色推断关卡规则。

设计目标超出模板能力时返回 `unsupported_feature`，先升级模板或缩减需求；不得默默省略玩法后报告成功。

## 9. 后续阶段：GM 接口与 AI 自动试玩

### 9.1 GM 协议

协议建议 `gm.v1`，采用本地 IPC 或仅绑定 loopback 的 WebSocket，握手含短期 token、build digest 和能力列表。GM 只在测试构建启用，正式发布关闭。

| 操作 | 参数/返回 | 约束 |
|---|---|---|
| `hello` | 协议版本、build digest、动作集合、测试能力 | 不兼容或错误构建立即拒绝 |
| `reset` | level ID、seed、批准的 scenario；返回 observation | 重置 tick 与 RNG，保留 scenario 来源 |
| `observe` | tick、实体摘要、玩家状态、目标进度、事件 | 定义 player/debug 两种可见性 |
| `step` | 动作集合、持续 tick 数；返回状态、事件、terminal | 固定 tick、每次步数有上限、不依赖墙钟 |
| `capture` | 指定 tick 的图像证据引用 | 图像绑定同一 build/session/tick |
| `snapshot` | 受版本约束的状态和可选 digest | 不承诺任意引擎全状态跨版本恢复 |
| `debug.set_state` | 定向测试用 teleport/spawn/数值设置 | 特权通道，记录 taint，不计正常通关 |

每条请求携带 session、request ID 与预期 tick；`step` 重复 request ID 返回原结果，不能重复推进游戏。连接中断后先查询已执行请求或重置重放，不能盲目重试非幂等动作。

### 9.2 试玩策略与证据

- **确定性脚本 bot**：固定 seed/输入序列，验证移动、阻挡、目标触发、胜负和重置，作为基础回归。
- **搜索/随机策略**：覆盖边界、碰撞与异常序列，产生可重放输入。
- **LLM tester**：从 observation 和必要截图规划短动作序列，选择探索目标；不逐帧调用模型。
- **场景测试**：允许 GM 设置边界状态验证特定规则，但报告单列为 scenario coverage。

正常通关必须使用 player observation 与玩家动作，禁止 `set_win`、传送到终点、无敌等捷径。使用 debug 信息的策略标记为特权测试，不与玩家视角成功率混合。

回放记录：build/design/assets/template/protocol 版本、seed、初始 scenario、每次动作与 tick、状态摘要、截图、特权操作标记和终止原因。

确定性承诺范围首先限定为锁定引擎、模板、平台和配置的受控逻辑；同 seed 不自动保证浮点物理跨平台逐位相同。需要时对关键离散状态做精确比较，对物理位置做明确容差断言，不先建设完整自研仿真核。

## 10. QA 报告与缺陷回流

### 10.1 分层 QA

| 层级 | 检查 | 执行时机 |
|---|---|---|
| D0 | 设计 schema、引用、范围、逻辑地图静态约束 | 设计阶段 |
| A0 | 素材 schema、CLI QA、覆盖率、映射、复现 | 素材阶段 |
| E0 | 导入、构建、启动、资源引用 | 引擎阶段 |
| P0 | GM 契约、玩法规则、胜负、回放、崩溃/软锁 | 试玩阶段 |
| V0 | 素材/游戏视觉审阅、截图对比 | 有视觉能力时执行，默认软评审 |
| H0 | 人工设计/美术/玩法签字 | 按项目策略执行 |

首期输出设计/素材质量报告，必须显示后续游戏 QA 为 `not_run`，不能将其命名为“全游戏 QA 通过”。

### 10.2 报告契约

`GateReport` 至少包含 gate ID/version、stage、status（`passed/failed/blocked/not_run`）、输入 digest、metrics、结构化 failures、evidence refs、执行工具版本和时间。

最终 `qa-report.json` 与 `qa-report.md` 来自同一结构化报告，包含：

- 本次范围、构建与输入 revisions、总体判定、执行和未执行项目。
- requirement → test → evidence 覆盖表；区分静态检查、脚本回放、LLM 探索和人工评审。
- 崩溃、通关/失败、超时、软锁候选、动作/状态覆盖、截图和回放入口。
- 缺陷 ID、严重级别、责任阶段、最小复现、期望/实际、证据、建议修复。
- 预算消耗、审批、特权测试标记、环境限制及未决风险。

软锁检测应综合时间窗、位置/状态/目标进度及规则上下文；静止不必然是软锁。LLM 未通关只能报告该策略在预算内失败，不能直接证明关卡不可通关；一条成功回放也不能证明覆盖充分。

### 10.3 回流规则

素材缺帧/辨识度问题回到 assets；关卡阻断/目标矛盾回到 design；碰撞或资源加载错误回到 engine；证据不足回到 playtest。

缺陷创建修订任务，但修改必要验收条件、golden 或核心规则仍需审批。修复后重新执行受影响门禁与回归，旧报告绑定旧 digest，不自动继承为新版本通过。

## 11. 面向用户的操作界面（拟新增）

以下为 **Harness 将来提供的接口设计，不是现有可执行命令**。所有子命令支持机器可读响应，长过程事件写日志，最终 stdout 输出一份 JSON 摘要。

| 拟议命令 | 用途 |
|---|---|
| `pxl-harness init --project <path> --brief <brief-file>` | 创建游戏项目与输入快照，不修改数据库 |
| `pxl-harness run --project <path> --until design` | 仅生成设计候选/审阅包 |
| `pxl-harness approve --project <path> --stage design --revision <revision> --digest <digest>` | 人工批准确定版本 |
| `pxl-harness run --project <path> --until assets` | 复用已批准设计，完成素材闭环 |
| `pxl-harness status --project <path> --json` | 展示阶段、预算、阻塞、产物入口 |
| `pxl-harness resume --project <path> --run <run-id>` | 对账后恢复未完成任务 |
| `pxl-harness retry --project <path> --task <task-id>` | 在权限/预算允许时创建新 attempt |
| `pxl-harness verify --project <path> --scope assets` | 无需 LLM，独立验证冻结产物 |
| `pxl-harness report --project <path> --run <run-id>` | 生成审阅/质量摘要 |
| `pxl-harness run --project <path> --until qa` | 后续完整闭环；未配置引擎时明确阻塞 |

CLI 成功完成调用与阶段通过需区别：`waiting_approval`、`blocked` 等写入结构化状态，并通过独立退出码区分于成功。审批前执行到等待点不意味着设计已正式发布。统一退出码表在 H0 契约中冻结，避免与被调用 `pxl` 的退出码混淆。

默认支持前台单次运行；长期 daemon、Web UI、MCP server、任务调度和多 Agent 并发不是首期要求。

## 12. 实施里程碑与任务拆分

不依赖主观工期承诺，按可运行产物与退出标准推进。**H0–H3 是本次优先建设范围；H4–H5 是后续事项。**

### H0：最小 Harness 地基

任务：

1. 新建 `harness/` package 并加入 workspace；建立 CLI、配置、统一错误与版本契约。
2. 定义 `Run`、`Stage`、`ArtifactRef`、`GateReport`、角色和预算模型。
3. 实现 provider trait、fake provider、一个真实 provider、schema 校验和最小工具循环。
4. 实现受控文件工具、预算账本、append-only 事件、项目锁与候选目录。
5. 实现进程 adapter 和 `PxlAdapter`；先用已有 tank recipe 验证真实素材工具接入。
6. 确认是否移植 miniagent；复制路径必须先通过来源/许可证审查。

退出标准：离线测试可模拟一次 tool call；真实 `pxl build/validate` 的输出被正确归一化；错误 JSON、超时、越权、预算耗尽均能结束且留证；无须安装 miniagent。

### H1：设计 Agent 闭环

任务：定义设计/关卡/style/素材需求/验收 schema；建立坦克品类模板；实现 designer 工具、静态 gate、修复和人工冻结；生成 Markdown 审阅视图。

退出标准：从 brief 得到可审阅且引用闭合的设计包，含 3 个逻辑关卡；必要素材覆盖所有可见实体/tile；故意遗漏目标、越界坐标、无效素材引用或超过范围预算均失败；审批绑定 digest，修改后旧批准无效。

### H2：素材 Agent 闭环

任务：实现需求到任务/recipe 映射、样板先行、批量构建、CLI QA、额外语义 gate、修复、manifest 和原子发布；独立 `verify` 不依赖 LLM。

退出标准：从 H1 设计包生成完整必要素材包；所有必要动画/方向可从 manifest 找到；真实 CLI QA 通过；失败产物不进入已发布 revision；同源双构建哈希一致；Agent 修复必须回到源配方且复建成功。

### H3：首期可靠性交付

任务：断点恢复、预算续记、缓存与失效传播、故障注入、质量报告与试运行评估。

退出标准：

- `brief → design approval → assets` 可由 CLI 连续完成；其中人工审批作为明确暂停点。
- 在模型返回后、构建期间、构建结束未提交、revision 已提交未更新摘要等位置中断，恢复后均不误报成功、不覆盖已发布产物。
- 改一个素材只重建相应依赖；改 palette 正确失效相关组；改纯文档不无故重画。
- 报告列出未执行的引擎/游戏试玩检查，不以素材完成替代游戏完成。
- 固定模型、版本与预算进行至少 5 次全新 Agent 试运行，目标 ≥4 次无需额外人工修复完成设计/素材；正常审批不算修复介入。记录每次成本和失败原因；该小样本是工程冒烟目标，不是统计可靠性承诺。

**H3 完成即可独立交付，不等待引擎与 GM。**

### H4：单一引擎模板与 GM

任务：Godot spike、锁定版本、坦克模板、数据/atlas 导入、构建/启动与 GM v1、正常玩家动作和调试通道隔离。

退出标准：冻结设计/素材能装配可玩切片；正常玩家动作通过基础规则测试；reset/step/request 去重/超时可测试；保留日志与截图；未实现玩法明确报错。

### H5：自动试玩与游戏 QA 回流

任务：脚本回放、LLM tester、softlock 候选分析、coverage、JSON/Markdown QA、缺陷修订与重跑。

退出标准：完整链路输出可玩构建和证据齐全的 QA 报告；至少验证正常通关、失败/重置、碰撞/资源加载场景；注入错误能被发现、归属并复现；特权 GM 测试不能计入玩家通关率。

## 13. 测试与验证计划

| 测试类型 | 首期必须覆盖 |
|---|---|
| 契约单元测试 | schema、ID/引用、关卡网格、asset 映射、状态迁移、digest 规范化 |
| Agent 测试 | fake provider 的合法/非法 tool call、无进展、修复耗尽、错误完成声明、压缩/评审预算计入 |
| CLI adapter 测试 | 普通 JSON、裸 schema、退出码 1–6、非 JSON、stdout 大输出、超时、缺失输出 |
| 素材集成测试 | 使用真实 tank recipe 的 build/validate/inspect；目录隔离、失败残留不发布、动画映射 |
| 恢复测试 | 各提交边界崩溃、坏日志、锁冲突、重复 resume、预算持久化、在途任务对账 |
| 权限测试 | `..`、绝对路径、符号链接父目录、recipe 内输出路径、受保护文件和未批准工具 |
| 增量测试 | 单资产、共享 palette、gate 版本、关卡布局和文档变更的依赖失效 |
| 端到端 fixture | 无 LLM key 的固定设计/工具响应闭环，产物覆盖与报告一致 |
| 在线评估 | 显式启用真实 provider，记录成功率/成本；默认 CI 不消耗付费 API |

实施后运行 workspace 格式检查、`cargo test -p pxl-harness`，并对受影响的 `pxl-cli` 契约执行回归。不得自动更新 golden；当前素材测试使用 `UPDATE_GOLDEN=1` 更新基准，未来 Harness 必须保护该能力，不能让 Agent 设置后“修绿”。

后续增加 GM 协议契约、回放一致性、渲染证据与 QA 缺陷注入测试。真实引擎/图形环境不可用时明确标记跳过，不用 mock 通过宣称真机试玩通过。

## 14. 风险、待验证项与收敛原则

| 风险/待验证项 | 处理策略 |
|---|---|
| miniagent 许可与依赖缺失 | 默认独立实现；授权确认后才能复制并记录来源 |
| 设计文案丰富但不可执行 | JSON 规则事实源 + 稳定 ID + 引用/范围门禁 |
| 当前 pxl 无法覆盖任意像素风格 | 能力模板、样板先行、显式 unsupported，不承诺任意图像生成 |
| 素材 QA 通过但观感差 | 预览/人工审阅；可选 VLM 且不伪造视觉能力 |
| CLI 部分写入/覆盖/路径逃逸 | attempt staging、输入引用验证、独立产物提交与恢复对账 |
| LLM 重试成本和上下文增长 | 统一预算、源产物引用、有界修复、无进展终止 |
| 脚本/引擎执行的宿主权限 | 首期无 shell，后续模板审查与隔离；cwd 不是沙箱 |
| 开源引擎 headless/截图/物理不一致 | H4 spike 锁版本，明确平台边界和图形 runner |
| Agent 修改验收来降低难度 | gate/golden/审批保护，修订需要独立权限 |
| 规划扩张为通用 Agent 平台 | 首期仅两阶段、两个角色、一个品类样板、一种素材 adapter |

首期成功定义不是“自动生成任意完整游戏”，而是：**用户提供创意，Harness 能产出经过审阅、可机器验证的设计与关卡包，并自主驱动现有 `pxl` 将其变成完整、可复现、可增量修复的像素素材包，同时保留后续引擎/GM/QA 的稳定接入契约。**

## 15. 参考与实施依据

仓库内：

- [原游戏开发管线调研](game-dev-pipeline-harness-plan.md)
- [已有 miniagent 集成建议](miniagent-integration.md)（历史设计，以当前源码为准）
- [素材架构](architecture.md)、[CLI 设计](cli-design.md)、[素材管线](asset-pipeline.md)
- [miniagent core](../thirdparty/miniagent/rust/src/core/)、[goal](../thirdparty/miniagent/rust/src/goal/)、[runtime](../thirdparty/miniagent/rust/src/runtime.rs)、[skill](../thirdparty/miniagent/rust/src/skill.rs)
- [pxl CLI](../crates/pxl-cli/src/)、[素材 QA](../crates/pxl-qa/src/lib.rs)、[tank recipe](../examples/tank/tank.recipe.json)、[golden 测试](../crates/pxl-cli/tests/golden.rs)

引擎 spike 待核对的官方来源：

- Godot：https://github.com/godotengine/godot
- Godot 官方文档：https://docs.godotengine.org/en/stable/
- Godot 示例：https://github.com/godotengine/godot-demo-projects
- Phaser：https://github.com/phaserjs/phaser
- Bevy：https://github.com/bevyengine/bevy

本计划未重新进行外部项目版本/许可证调研或运行引擎；引擎与模板的具体版本、授权和能力需在 H4 实施前验证。
