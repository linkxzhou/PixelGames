# Harness v3：通用 Agent 能力计划（tools / MCP / goal）

日期：2026-10-22。范围：`harness/` crate 的 agent 层升级；基于对 `harness/src/`（v2 实测）与 `thirdparty/miniagent/rust/src/`（结构分析）的源码核对。本文件只做规划，不改代码。

> **✅ 状态（2026-10-22）：V3-A / V3-B / V3-C / V3-D 已全部实施并通过验证（工作区待提交）。**
> - V3-A：`agent/tools/` 模块化（trait + read_only/activity/source 元数据、fs/todo 内置工具、`additionalProperties` 修复、`Permission` 扩位 Todo/Mcp/Goal、designer/artist 授予 todo）。
> - V3-B：`agent/mcp/`（config/client/registry，stdio JSON-RPC、`${VAR}` 展开、readOnlyHint 权限映射、失败隔离）。
> - V3-C：`agent/goal/`（状态机/evaluator/runtime、`run_with_goal` 续跑、`GoalEvaluated` 事件、goal.json 持久化；assets 的 condition 由冻结契约生成）。
> - V3-D：只读工具同轮并行（scoped threads）、上下文压缩（`agent/compress.rs`，>96KiB 换 digest 桩）、CLI `agent` 子命令。
> - 验证：`cargo test --workspace` 全绿（176 项）、CLI 端到端冒烟通过。原文保留作为设计与验收依据。

## 1. 现状核实

### 1.1 harness v2 agent 层（实测）

| 位置 | 现状 | 限制 |
|---|---|---|
| `agent/tools.rs` | `ToolDef{name, description, parameters, needed, run}` 闭包式注册；`ToolRegistry::dispatch` 做 权限交集 + 最小 JSON Schema 校验 | 无 `read_only`/`activity` 元数据；无 MCP 桥接点；Schema 不校验 `additionalProperties` |
| `pipeline/design_stage.rs` `registry()` | 5 个工具（read_brief / read_template / request_review 等）内联闭包 | 工具与 stage 绑死，无法单独复用或测试 |
| `pipeline/assets_stage.rs` `registry()` | 10 个工具内联闭包（约 250 行） | 同上；`assets.status` 已是事实上的 todo，但没有通用机制 |
| `agent/runner.rs` | 串行工具执行；逐回合持久化 ledger；tool-results 落盘审计 | 无并行执行、无上下文压缩、无自动续跑语义 |
| `policy/mod.rs` | `Permission::{ReadProject, WriteCandidate, Pxl, Engine}` 四值枚举，host∩stage∩role 交集 | 无 Mcp / Todo / Goal / Network 类权限位 |
| `agent/provider.rs` | `ModelProvider` trait + OpenAI 兼容 + Fake（脚本化） | 可保留，是测试基座 |
| CLI | `init / run / approve / status` | 无通用 agent 入口（只能跑游戏流水线） |

### 1.2 miniagent 对应能力（`thirdparty/miniagent/rust/src/`）

| 模块 | 内容 | 对 harness 的价值 |
|---|---|---|
| `tools/base.rs` | `Tool` trait：`name / description / input_schema / read_only / activity / execute` | 元数据驱动的权限与审计，正是 v2 缺的 |
| `tools/fs.rs` | Read / Write / Edit / Glob / Grep 五件套，root 沙箱 | 通用 agent 必需；需嫁接到 `policy::path` 的 WriteRoot |
| `tools/todo.rs` | `TodoManager` + `TodoWriteTool`，状态渲染进 prompt | 结构化任务管理，跨回合可见 |
| `tools/mcp_client.rs` + `mcp/` | `McpRegistry`（start/stop/reload/status）、`McpToolBridge`（远程工具 → 本地 Tool）、config（`${ENV_VAR}` 展开、stdio/sse/streamable_http 三种 transport）、`readOnlyHint` 透传 | 让 harness 可挂任意 MCP server（含未来 `pxl mcp`，见 miniagent-integration.md 方案 B） |
| `goal/` | `GoalRuntime`（状态机 active→complete/paused、auto-continue、防抖、续跑上限、eval 连错熔断）+ `GoalEvaluator`（独立评估器按 completion_condition 编译验收清单逐轮判定）+ `get_goal/create_goal` 工具 | 把"素材是否齐了"从 60-turn 硬上限 + 聚合 gate 升级为**目标驱动 + 独立验收** |
| `core/engine.rs` | `run_single / run_parallel` 工具执行、compact、reflection、rollback | 长循环成本控制（battle-city 实测 78 次调用 / 1.54M token） |
| `core/permissions.rs` | ask/auto/readonly 三模式 | 交互式场景才需要；harness 批处理保持权限交集，不引入 ask 模式 |

### 1.3 依赖方式判定

`thirdparty/miniagent/rust/` **没有 Cargo.toml**，不是可编译 crate（另存在等价 Python 实现）；且其面向终端交互（REPL/daemon、防抖、外部输入抑制），与 harness 的批处理、审计、attempt 沙箱哲学不同。**v3 不引入 miniagent 依赖，采用"结构移植 + harness 原语重写"**：保留 harness 的权限交集 / 事件审计 / 确定性内核，把 miniagent 的工具元数据、MCP、goal 三块机制按 harness 风格重写进 `agent/`。

## 2. 目标架构

```
harness/src/
  agent/
    tools/          # v3-A：trait 化 + 通用内置工具
      mod.rs        #   Tool trait（含 read_only/activity）、ToolRegistry、Schema 校验增强
      fs.rs         #   read/glob/grep（读受 template_roots+attempt 约束）、write/edit（WriteRoot 内）
      todo.rs       #   TodoManager + todo.write 工具，状态注入下一回合 prompt
      mcp.rs        # v3-B：McpToolBridge —— McpRegistry 工具适配成 ToolDef
    mcp/            # v3-B：config / registry / transports(stdio 优先)
    goal/           # v3-C：models（GoalRecord/GoalStatus）、runtime、evaluator
    runner.rs       # v3-D：并行执行（可选）、compact 钩子、goal 续跑判定
    provider.rs     # 保留
  pipeline/         # 游戏层不变：stage registry() 改为组装 agent 内置工具 + stage 专属工具
  policy/           # Permission 扩位
```

分层原则：`agent/` 不得 import `pipeline/`、`contracts/`（通用层零游戏知识）；`pipeline/` 继续独占 pxl 语义。

## 3. 实施批次

### V3-A：工具体系重构（tools.rs → tools/ 模块）

1. `ToolDef` 增加 `read_only: bool` 与 `activity: String`（执行摘要，进事件日志）；新增 `source: ToolSource::{Builtin, Stage, Mcp, Goal}` 便于审计与过滤。
2. `Tool` trait 化：`fn spec(&self) -> ToolSpec`、`fn read_only(&self) -> bool`、`fn execute(&self, args, ctx) -> Result<ToolOutcome>`；`ToolDef` 保留为 trait object 包装，现有 stage 闭包可低成本迁移（先加 `impl Tool for ...` 适配器，不一次性重写 15 个工具）。
3. JSON Schema 校验补 `additionalProperties: false` 支持（miniagent 已用，防小模型塞废字段）。
4. 新增通用工具：`fs.read / fs.glob / fs.grep / fs.write / fs.edit / todo.write`。读写路径仍走 `policy::path::WriteRoot` 与 template_roots 白名单——**沙箱语义不放宽**，只是把 stage 里重复出现的 `read_candidate / write_candidate` 收敛为通用实现，stage 保留薄包装（名字不变，模型 prompt 无感）。
5. 工具注册集中化：`pipeline/*_stage.rs` 的 `registry()` 改为 `ToolRegistry::new().with_builtin(cfg).with(stage_tools)`，stage 文件只声明专属工具。
6. `Permission` 扩位：`Todo`（所有角色默认授予）、`Mcp`（host 白名单控制）、`Goal`。序列化兼容：新枚举值走 serde 默认，旧 run 目录事件仍可解析。

验收：`cargo test -p pxl-harness` 全绿；15 个既有工具行为不变（名字、schema、权限位逐一对照测试）；`additionalProperties` 违例返回 usage 错误；`fs.*` 无法逃出 attempt/template_roots（越界路径测试）。

### V3-B：MCP 接入

1. 新增 `agent/mcp/`：`McpServerConfig`（stdio 优先；sse/streamable_http 留接口不实现）、`${ENV_VAR}` 展开、`McpRegistry`（start_all/stop_all/status，drop 时清理子进程）。
2. `McpToolBridge`：把远程 `list_tools` 结果包装成 `Tool`（`read_only` 取 `readOnlyHint`），命名 `mcp_<server>_<tool>`（与 miniagent 约定一致）。远程工具的权限映射：`readOnlyHint=true → ReadProject`，否则需要 host 显式把该 server 列入 `mcp_write_servers` 才映射 `WriteCandidate`；未映射的权限直接不可见（不注册）。
3. 配置来源：项目目录 `harness.json` 的 `mcpServers` 字段（沿用 miniagent `mcpServers` JSON 形状，迁移成本低）+ host 策略白名单；`pxl mcp`（miniagent-integration.md 方案 B）落地后即可零成本挂载。
4. 失败隔离：单个 server 启动失败记事件、跳过，不阻塞 stage；MCP 工具执行超时（默认 60s）返回 `ToolOutcome::fail` 而非 panic。
5. 事件：`Event::McpServerStarted/Failed/ToolCalled` 复用 `ToolStarted/ToolFinished`（source=Mcp），审计链不断。

验收：脚本化 fake stdio MCP server（测试内置，无网络）列 2 个工具，agent 能调用并审计；readOnlyHint 映射与白名单拒绝各有测试；server 崩溃后 harness 继续；无 `~/.miniagent/mcp.json` 时行为与 v2 完全一致。

### V3-C：Goal 与验收闭环

1. `agent/goal/models.rs`：`GoalRecord{objective, completion_condition, status(active/paused/complete), history≤64, tokens/time_used}`，状态机迁移校验；持久化到 run 目录 `goal.json`。
2. `agent/goal/evaluator.rs`：独立 `GoalEvaluator`（复用 `ModelProvider`，与主对话模型可不同配置）：首轮把 completion_condition 编译为验收清单并缓存（criteria_source 失效才重编），每轮末快照对话判定 yes/no；evaluator 自身异常与"判定 no"分离计数，连错 ≥3 熔断为 paused 并告警。
3. `agent/goal/runtime.rs`：`maybe_evaluate`（每轮末）、`should_auto_continue`（续跑上限、防抖可关——批处理场景 force=true）、`continuation_prompt`（下一轮注入"目标未达成 + 未满足清单项"）。
4. 与 pipeline 结合（重点，不是照抄）：assets stage 的包级 task 改造——goal 的 completion_condition 由 harness 从**冻结需求契约**生成（如"所有 required 资产 built+validated 且 manifest 完整"），不由模型自报；`create_goal` 工具不开放给游戏流水线角色（模型不能自定验收标准），只在通用 agent 模式暴露。`max_fixes` 与续跑上限取 min，防饿死调度（P0-C 计划）继续由 asset_queue 承担，goal 只做"整体是否达成"判定。
5. `todo.write` 与 asset_queue 关系：todo 是模型自用便签（prompt 注入），asset_queue 是 harness 真值（gate 依据），两者不合并，事件中都留痕。

验收：FakeProvider 脚本化"永远差一个资产"场景：续跑到上限停止且事件含每轮 evaluator 结论；evaluator 连错熔断为 paused；goal 完成时 events 有 complete 记录及 evidence；冻结契约生成的 condition 不含模型自由文本；通用 agent 模式下 create_goal/get_goal 可用。

### V3-D：Runner 增强 + 通用 agent 入口

1. 并行工具执行：同回合内 `read_only` 工具可并行（`Pxl` 写类保持串行，构建顺序敏感）；事件按完成序追加，call_id 保证可回放。
2. 上下文压缩：回合数或 prompt 字节超阈值时，把早期 tool 结果替换为 digest + 路径引用（tool-results 已落盘，天然支持）；不做 LLM 摘要（保持确定性）。直接回应 battle-city 实测的 prompt 膨胀（3,634 → 33,143 token）。
3. CLI 新增 `agent` 子命令：`pxl-harness agent --cwd <dir> -p "<task>" [--mcp-config ...] [--max-turns N]`，走同一 `agent/` 层 + 通用工具 +（可选）goal，不经过 pipeline——这是"通用 agent 能力"的交付形态；流水线 `run` 命令行为不变。
4. resume 明确不在本计划（沿用 P0-D 的边界结论：先逐回合持久化 + 事件对账，跨进程 resume 另行立项）。

验收：并行执行下事件序列仍可重放（同 call_id 集合）；压缩后模型仍能通过引用路径取回完整结果；`agent` 子命令在无 MCP 配置时可用纯内置工具完成一个文件整理任务（FakeProvider 脚本测试）；`run` 命令产物与 v2 逐字节一致（黄金对比）。

## 4. 批次依赖与优先级

```
V3-A（工具重构）──► V3-B（MCP）──► V3-D（runner/CLI）
      └──────────► V3-C（goal，可与 B 并行，依赖 A 的 trait 化）
```

建议顺序 A → C → B → D：goal 机制直接缓解素材阶段最痛的"60-turn 盲跑"（battle-city 1.54M token 实测），收益最快；MCP 在 `pxl mcp` 落地前只有通用价值。V3-A/B/C/D 均需在 `harness-asset-feedback-repair-plan.md` 的 P0-A~D 修复之后开工或并行不冲突（该计划改反馈与调度，本计划改能力底座；两者都动 `assets_stage.rs` 的批次需错开提交）。

## 5. 风险

1. **通用化稀释流水线安全边界**：fs 工具、MCP 都扩大攻击面。缓解：一切读写仍过 `policy::path`；MCP 写权限需 host 白名单显式授予；通用 agent 模式的 cwd 即其唯一写根。
2. **evaluator 双倍模型成本**：每轮一次验收判定。缓解：条件编译为可关（无 evaluator 时回退 v2 行为）；验收清单缓存后单轮判定 prompt 很小；evaluator 可配便宜模型。
3. **miniagent 双实现漂移**：Python/Rust 两版机制不完全同步，移植以 Rust 版为准并在本文档引用文件路径，不追溯 Python 行为。
4. **并行执行破坏构建顺序**：assets 阶段 palette→build 有依赖。缓解：默认串行，并行仅限 read_only 且 stage 可声明 `parallel_safe=false` 一票否决。

## 6. 本次边界

- 只读了 `harness/src`、`thirdparty/miniagent/rust/src`（outline + 关键文件全文）与 `plan/` 既有文档；未编译 miniagent（无 Cargo.toml），未运行真实模型。
- miniagent 的 daemon、skill 加载、reflection、REPL 不在本计划范围（harness 无交互终端场景；roles/*.md 已覆盖 skill 职能）。
- 与 `harness-asset-feedback-repair-plan.md` 不重叠：该文件管"反馈质量与调度"，本文件管"能力底座"；冲突文件（`assets_stage.rs`、`runner.rs`）提交时按两计划批次错峰。
