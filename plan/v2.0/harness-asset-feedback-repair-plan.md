# Harness 素材生成反馈与修复闭环计划

日期：2026-10-22。范围：battle-city 素材阶段；本次只产出计划，不修改实现、冻结设计、审查原文或现有运行数据。

> **状态（2026-10-22）：**
> - ✅ **P0-A**（统一诊断与可回放反馈）已实施——提交 `23d05c3`：`adapters/pxl.rs` 保留 hint/path/exit_code、逐回合账本、诊断事件字段（serde default 兼容旧日志）。
> - ✅ **P0-B**（资产待检队列消除 w 漏洞）已实施——`pipeline/asset_queue.rs`（728 行）：missing/pending_build/build_failed/built/validated/blocked 状态机、指纹 dirty 传播、同轮延迟构建防抖、队列查询工具。
> - ✅ **P0-C**（修复调度与反馈门禁）已实施——per-asset 连续失败 3 次上限 → blocked、coverage 门禁、manifest 从成功记录生成并绑 digest。
> - ✅ **P0-D**（预算崩溃一致性）已实施——`policy/budget.rs` snapshot（temp+rename 原子写、绑定 seq）、事件重放对账、zero-snapshot 重建测试（78 次/1,545,488 token 场景）。
> - 🔶 **P1-A** 部分：`AnimMode::{dynamic,static}` 已贯穿 recipe/qa/cli（单帧冲突已修，原创 fixtures 已加，见 `examples/static_bullet/`）；effect/one-shot 专门 QA policy **未做**。
> - 🔶 **P1-B** 部分：artist 角色 checkpoint 说明已更新；`assets.read_requirements` 补齐与受限 schema 查询工具已开（`assets_stage.rs` schema 工具）；宏元数据（参数/默认值/示例）**未做**。
> - ⬜ **批次 E**（上下文 checkpoint 受控整轮复跑）未实施；上下文压缩部分由 V3-D 的 `agent/compress.rs` 承担。
> - 回归命令不变：`cargo test --workspace`（2026-10-22 全绿，176 项）。原文保留作为验收依据。

## 1. 核实结果与证据边界

依据 `review/v4-pr4-battle-city-assets/`，并直接解析 `events.jsonl`、对紧凑 JSON 参数计算 SHA-256 后匹配构建目标；不使用“最近写过哪个素材”猜测目标。

- 412 条事件，78 次已完成模型调用；第一轮 60 次，第二轮 18 次。末条是第二轮 turn 18 的 started，没有 finished，不能算成第 79 次完成调用。
- prompt 1,486,155 + completion 59,333 = **1,545,488 token**。只累加 `model_turn_finished`；`candidate_written` 中的 token 是父回合副本。
- 29 次构建：6 成功、23 失败。player_tank 1/9、enemy_tank 4/2、bullet 1/5、explosion 0/7（成功/失败）；w **0 次构建**。
- required 13 项，仅 player_tank、enemy_tank、bullet 有交付；10 项缺失。w 写过一组 specs，其余 8 项没有写入，explosion 多次修复仍未交付。
- `failures.md` 的 explosion 6 次、w 构建失败 1 次，以及 enemy/bullet 成败数均已过时，不作为统计依据。`missing-assets.md` 虽正确注明 w 没收到错误，其“Root cause: model”归类仍需在后续审查更正：配方存在错误，但没有触发构建并反馈是 **Harness 闭环缺口**。
- 用户确认 explosion 七次均因帧差过大失败；原始事件只能独立证明七次失败，不能恢复每次 QA 文本。最后留存配方的本地复现记录是 42.96% 与 51.17%（shift-adjusted 50.78%），上限 35%。不要将最后一次诊断冒充七次历史 stdout。
- w 留存配方为 `pulse_glow(steps=2)`，本地复现为 `E_SCHEMA` / `macro pulse_glow has no argument 'steps'` / `pulse_glow takes: ramp`。运行时没有 w 构建，模型不可能通过该次工具结果获知错误。
- 单字母 b/x/w/h/g/t 是冻结设计的合法资产 ID，不应改名来“修复缺失”。

## 2. 源码结论

| 位置 | 当前行为 | 修改必要性 |
|---|---|---|
| `harness/src/pipeline/assets_stage.rs:214–248` | write 只验证 JSON 语法、路径和模板重复 | recipe 写入成功不代表能构建；需要待检状态和自动检查机制 |
| 同文件 `324–358` | 显式 build 将 message/error_code 和最多 8 个 failed_checks 返回模型 | 已有 live 反馈，不是所有失败都没反馈；但 hint、完整诊断和历史没有保存 |
| 同文件 `360–404` | validate 从 checks 取 `message`/`detail` 字段，而 CLI `checks_json`（`cmd.rs:1265–1280`）只输出 `{id, pass, value, limit}` | **失败项一律退化成 `"{id}: failed"`，value/limit 全部丢失**；build 路径靠 `Value::to_string()` 幸存，validate 路径完全损失，需专属回归 |
| `harness/src/adapters/pxl.rs:307–398` | normalize 提取 error.code/message，不提取 hint/path | 即使真的调用 w build，宏签名 hint 也会在结构化反馈链中丢失 |
| `harness/src/agent/runner.rs:113–170` | 正常返回的失败 Outcome 在事件中 error_code 为 null，exit_code 恒为 null；结果只放入内存消息 | 不能审计模型实际收到什么，也不能跨修复轮继承失败细节 |
| `assets_stage.rs:662–846` | gates 检查帧、PNG、manifest、specs 和已有 sheets；不构建已写未构建 recipe | w 类错误直到外部人工复现才暴露；已有 sheet 验证通过不等于包完整 |
| 同文件 `849–994` | 整个包一个 agent task，所有资产共享 60-turn 上限；下一轮重新建消息，只继承聚合门禁 | explosion 修复持续占用回合，其他 required 项被饿死 |
| 同文件 `717–723` | PNG check 被 missing frames 连带置 false，但 failures 只包含坏 PNG | 有效 PNG 也出现不可行动的 `value=png` 失败，应拆分覆盖与字节检查 |
| 同文件 `406–524` | publish 根据磁盘 frame 前缀判 delivered，seed 固定 1，animations/spec_paths 为空 | 需要成功构建记录与输入版本绑定，而非只信磁盘残留 |
| `crates/pxl-qa/src/lib.rs:60–120,207–267` | default 对每个 anim 强制至少 2 帧、1%–35% 帧差；只豁免非循环透明末帧的上限 | 冻结设计有合法单帧 idle/fly；爆炸扩张也不能直接等价于循环 flicker |
| `crates/pxl-ops/src/macros.rs:680–687` | pulse_glow 只接受 ramp，本身输出两帧，默认 olive | 不应添加 steps 来迁就错误调用；且亮度脉冲未必满足水波语义 |
| `harness/src/pipeline/runner.rs:239–262` / `policy/budget.rs:84–110` | 素材阶段返回后才保存 budget，工具耗时结算传 0，save 直接覆写 | 被杀时账本仍为零；需要逐回合持久化与事件重建 |

## 3. 实施顺序

### P0-A：统一诊断与可回放反馈

修改 `adapters/pxl.rs`、`agent/tools.rs`、`agent/runner.rs`、`storage/event_log.rs` 和 `assets_stage.rs`。

1. 为 pxl outcome 保留 `code/message/hint/path/exit_code/elapsed_ms`，checks 保持对象而非二次 JSON 字符串。build/validate 使用同一归一化与渲染逻辑；根据实际 pxl 输出契约做测试，不假设存在 message/detail。
2. 诊断文件保存完整 checks、输入配方/spec/palette digest、输出 digest 和调用标识；模型消息可以截断，但包含总数、截断标识及完整报告路径。成功结果也记录版本绑定。
3. 记录真正渲染给模型的工具结果或其不可变文本 artifact，并由事件引用；正常返回的 `ok=false` 也填写 pxl 错误码及退出码，不仅记录 HarnessError。
4. 新事件字段采用 serde default 等兼容策略；原有 412 条事件必须仍可解析。不要改写旧日志补造不存在的诊断。模型回合增加 attempt 或唯一 turn_id，避免修复轮 turn 重置后歧义。
5. 跨轮 repair prompt 带入每个资产最新诊断与 digest，禁止将旧输入的错误当作当前输入错误。敏感信息按字段白名单与脱敏策略落盘，不把 provider 配置、请求 headers 或环境写进报告。

验收：模拟 `E_SCHEMA` 时模型消息和诊断均包含 `pulse_glow takes: ramp`；模拟 `E_QA` 时包含 check id/value/limit；同一失败在事件、报告、模型反馈之间可按 call_id 对齐；**validate 失败项保留完整 value/limit，不再出现裸 `"failed"`**（对照 `cmd.rs:1265–1280` 的 checks 输出字段做解析测试）。

### P0-B：Harness 管理资产待检队列，消除 w 漏洞

主改 `assets_stage.rs`，必要时在 pipeline 下抽出小模块；状态通过 run 存储持久化。

1. 每项 required 维护 `missing_specs / pending_dependencies / pending_build / build_failed / built / validated / blocked`，以及输入指纹、最新报告、构建和修复次数。JSON write 成功只代表保存成功，同时标记受影响资产 dirty。
2. 一轮模型响应中的全部 writes 执行完后再检查完整依赖，避免 shapes/palette 尚未写出时构建；先运行同轮显式 build，自动检查只补齐未覆盖的最新指纹，不重复构建。
3. Harness 对完整且 dirty 的 recipe 自动构建，模型不显式调用也能在下一次 complete 前获得诊断。缺依赖返回缺失依赖列表；不是永远跳过。自动构建反馈注入协议需明确选型（二选一，随批次 B 定案）：(a) 轮末统一汇总进下一回合 user 消息或 repair note；(b) 以独立 Harness 状态消息注入，携带合成 call id 与诊断引用。OpenAI 式协议不允许无对应 assistant tool_call 的孤儿 tool message，方案 (a) 保守且无协议风险，倾向先做 (a)。自动构建必须有独立事件与资源计量。
4. 防抖：palette 改写会级联使全部 13 资产 dirty，本轮 palette 重写多达 7 次，若逐回合逐资产自动构建会放大成本。规则：palette 变更延迟到「本轮存在其余依赖已就绪的资产」或轮末扫尾时统一重建；每资产每轮自动构建上限 1 次，同指纹重复构建复用上次结果。
5. 为包提供状态查询，列明哪些写过却未 build、哪些失败、下一待办项。阶段轮末及 publish 前扫尾；所有当前 specs 必须有可追踪检查状态。
6. 改写 palette/spec 会使依赖输出失效。仅当前输入指纹对应成功构建且验证通过的输出可进入 frame coverage 和 manifest；旧成功产物不能为新失败配方背书。可先使用指纹隔离输出目录，再按成功记录索引，避免删除用户运行目录。

验收：复现第二轮 turn 4 同时写 explosion 与 w 后只显式 build explosion；w 仍被自动检查且错误在下一回合反馈。相同输入不重复 build，改 palette 后相关资产重新检查。palette 连续改写 3 次不触发超过 3 次重建（防抖生效）。

### P0-C：修复调度与反馈门禁

1. 先让全部 required 项获得首次生成/检查机会，再进入深度修复。按 priority 排序但不得无限占用；成功资产默认停止无必要润色。
2. 对单项连续相同失败设置暂定 3 次上限；指纹改变但仍是同类错误也计入修复额度，不能靠微改绕过。到限标 blocked、保存诊断、调度下一项，不能结束整个包导致其他项无机会。全局预算耗尽时明确列出未尝试项。
3. 先保留包级 task 加调度队列，降低改造风险；若日志证实上下文仍过大，再拆成 per-asset task。保留总 request/token 上限，不能仅扩大 60 turns。
4. `coverage.frames` 管缺项，`coverage.png` 只管已知输出的字节有效性，或显式列出缺项并改正 limit/value；任何 failed check 都须有可行动说明。门禁加入 pending/build_failed/stale 输出检查。
5. publish/manifest 从成功记录生成，真实填写 seed、spec_paths、animations、输入/输出 digest；校验完整方向、动画、帧数、尺寸，不能用存在任意 `{id}_*` 帧替代需求达成。

验收：固定模型脚本令 explosion 永远失败，w/hq_base/HUD/tiles 仍能获得首次尝试；已有坏输入的残留 PNG 不得发布；PNG 全有效但缺资产时不会提示“PNG 坏了”。

### P1-A：先明确 QA 契约，再修 explosion 与单帧素材

涉及 `pxl-qa/src/lib.rs`、`pxl-format/src/recipe.rs`、`pxl-cli/src/cmd.rs`、Harness 需求契约/设计门禁/工具 schema/角色说明。

- **不全局取消 35% 上限，不把两个相同帧当修复，也不擅改冻结设计。** 首选在当前规则内制作更渐进的四帧 explosion，加入能通过 QA 的原创回归样例。明确 squash 是相对 base 的变换，避免误以为后一帧变换累积在前一帧上；实施前以 CLI/ops 测试核实该语义。
- default 单帧冲突是确定问题：设计允许 bullet fly 1 帧以及若干 idle 1 帧，但 `anim_diff` 必败。推荐显式声明 static 动画意图，静态模式允许 1 帧且不做相邻帧差；动态模式保留最少帧数和防重复检查。声明应从冻结需求派生，并写进 recipe/sheet，build 与独立 validate 一致，不能任由模型关闭检查。
- 如渐进四帧不满足爆炸视觉语义，再引入 effect/one-shot 的专门 QA policy，要求非循环、正确帧数、像素合法、无越界、有效生命周期，并保留合适的连续性约束。阈值经 fixtures 校准，不通过资产名等于 explosion 或只看 loop=false 自动放宽。
- 当前 RecipeValidate 只有 rules 字符串；新增 policy 时必须贯穿格式 schema、CLI build、sheet metadata、CLI validate 和 Harness 允许值。设计 preflight 提前报告需求与规则不兼容，需修改设计时走新 revision/审批。
- `tank` rules 还要求同 sheet 至少四种地面 tile，不适用于每个坦克独立 recipe。不能简单把默认规则替换为 tank；独立素材方向/动画完整性由需求门禁检查，未来再拆 tank-sprite 与整包规则。

验收：合法静态 bullet/idle/UI 单帧通过；声明动态却只有 1 帧失败；循环大幅闪烁、相同帧重复、越界/非法 palette 仍失败；原创四帧爆炸通过并人工确认 1x/8x 动画，build 与 validate 结果一致。

### P1-B：能力信息与低成本上下文

- `pxl list macros` 当前仅列名字；`PxlAdapter::schema` 已存在，但 artist 没有 schema 工具。开放受限 schema 查询，并为 macros 增加参数、默认值、输出帧数、示例等元数据；复用宏校验签名来源，避免两套硬编码漂移。
- `pulse_glow(ramp=water)` 可以作为签名修复示例，不是保证 QA 或“水波”语义达标的万能配方；不要添加 steps alias 掩盖错误。water 应根据设计生成两帧局部波纹，并使用正确 tile metadata/接缝约束。
- `assets.read_requirements` 补齐 anims、fps、looped、seamless、semantics、priority 和批准 QA policy；当前只给 ID/kind/strategy/size/directions，信息不足。
- 更新 `harness/roles/artist.md` 的静态/动态、宏签名与 bounded repair 说明；将 palette、需求摘要、当前资产 specs、最新错误作为结构化 checkpoint。压缩较早的长 specs 与失败记录，保留 digest 和可读取路径，避免第一轮 prompt 从 3,634 涨到 33,143 token。
- 队列/失败额度先由 Harness 强制，不能只依赖 prompt 中现有“three identical failures”文字约束。

### P0-D：预算崩溃一致性（可与诊断并行）

修改 `agent/runner.rs`、`policy/budget.rs` 和 `pipeline/runner.rs`。

**范围界定（复核后修订）**：当前代码没有跨进程 resume——`PipelineRunner::start` 每次新建 run id，`BudgetLedger::load` 仅被测试调用。因此本批次交付是「逐回合持久化 + 崩溃后事件对账」，跨进程 resume 的 run 续接是**显式新增范围**，只在持久化与对账落地后另行立项，不与本批次验收混淆。验收中「重启后一致」指对账逻辑读事件重建账本，不指 CLI resume。

1. 每个 finished 模型/工具事件后保存账本，而不是等阶段返回；snapshot 使用临时文件加 rename 的原子写入，绑定已结算 seq。
2. 事件作为完成调用用量的依据；恢复时重放 snapshot seq 之后的 finished，确保不会漏算或重复算，不能累计 candidate_written 的 token。
3. 记录实际模型和工具 elapsed，自动构建也计量；真正 wall-clock 预算采用运行起点/恢复规则计算，不只相加已完成 provider 延时。
4. 已 started 无 finished 标识为 interrupted/usage_unknown，不能假装已计费为零或猜 completion；报告已知用量与未知调用。若 provider 支持查询用量，再单独对账。
5. token 限额在一次调用内可能超出，因为当前只在调用前 check。请求最大 completion 上限与剩余预算联动；若不能准确估算 prompt，清楚标注可超额边界，不承诺严格零超额。

验收：在模型 finished 后、工具 finished 后以及 snapshot 更新前模拟终止；重启后已完成调用数/token 与事件一致。旧的零预算快照能重建 **78 次 / 1,545,488 token**，同时报告末次 unknown；旧事件缺少精确 elapsed 时不编造耗时。

## 4. 回归与交付拆分

| 提交批次 | 交付 | 关键回归 |
|---|---|---|
| A | 统一诊断、日志兼容、逐回合预算 | E_SCHEMA hint/path；E_QA 数值；旧 events 读取；中断重建 |
| B | dirty 输入记录、自动检查、状态查询、门禁修正 | w 漏调用；同轮多写依赖；重复 build 去重；palette 失效与防抖；残留输出不能发布。**调度回归 fixture 用多帧动画资产**（bullet 单帧 fly 在批次 D 前必然 QA 失败，不得作为 B/C 的调度测试载体） |
| C | bounded repair、coverage-first、真实 manifest | explosion 死循环不饿死其他资产；方向/尺寸/动画/帧数缺项拒绝 |
| D | 静态/动态 QA 契约、能力元数据、原创 fixtures | 单帧冲突；动态闪烁不退化；effect 样例；build/validate 一致 |
| E | 上下文 checkpoint 与受控整轮复跑 | 成本对比、视觉检查、完整包门禁与审批 |

优先写 deterministic scripted provider 测试，无真实模型、无 API key、无新增依赖。现有 `harness/src/agent/runner.rs` 与 `harness/tests/cli_e2e.rs` 可承载闭环回归；`pxl-qa`、`pxl-ops` 与 CLI 测试承载规则和签名回归。留存 w/explosion 的 palette/spec/recipe 应复制成最小测试 fixture，不让测试覆写 `games/battle-city/runs/` 或依赖可变运行目录。

实施验证命令：

```sh
cargo test -p pxl-harness
cargo test -p pxl-qa -p pxl-ops -p pxl-format -p pxl-cli
cargo test --workspace
```

真实模型复跑放最后，另建 run 并设置明确预算，沿用批准设计或明确审批过的新 revision。记录每项首次尝试是否发生、build 成败/错误分布、未构建 specs、blocked 项、prompt/completion/请求/耗时、完整性门禁和视觉结果。以本轮 78 次 / 1,545,488 token 为基线；不预先承诺某个 token 降幅，不把 PNG 存在或 CLI exit 0 当作视觉质量通过。

## 5. 本次工作边界

- 已直接重算原始事件，并检查 Harness 反馈、调度、门禁、pxl 宏和 QA 实现；未执行真实模型请求或重新构建历史配方。
- 本计划不将历史审查文档静默改成新结论；后续批次应明确勘误 `failures.md` 和 `missing-assets.md` 中的旧统计及 w 归因。
- 当前 `games/tank-slice/runs/r1792635399555/` 的 `budget.json`、`events.jsonl`、`run.json` 三个未提交改动保持不动。
