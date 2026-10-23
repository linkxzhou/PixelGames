# Harness v4：Battle City 素材质量与反馈闭环优化计划

日期：2026-10-22。性质：**基于 PR #5 评审的下一轮 Harness v4 规划**，不是回溯修改历史“v4 运行”；本次只制定计划，不修改 `harness/`、历史运行、冻结设计或游戏素材。`harness/Cargo.toml` 的 crate 版本仍为 `0.1.0`，这里的 v4 是能力迭代代号，不是 Cargo 版本号。

## 1. 证据、口径与不可推断项

依据：`review/v5-pr5-battle-city-assets/comparison.md`、`r1792660166849/` 和 `r1792661955007/` 的留存 gate/queue，以及当前 `harness/src/`。前一轮规划参见 `plan/harness-asset-feedback-repair-plan.md`、`plan/harness-v3-agent-capability-plan.md`、`plan/pxl-quality-improvement-plan.md`。**历史运行使用当时二进制**；不能用当前工作区或运行中才提交的修复解释当时已生效的行为。当前工作区有未提交改动，实施时须先确认相应文件基线，不覆盖这些改动。

| 运行 | 可核实结果 | 严格解释 |
|---|---|---|
| 历史 v4：`r1792639385414` | 结束于第 2 轮；`out/<id>` 仅 3/13 通过；78 次完成调用、约 154.55 万 token | 曾经的低覆盖基线，不等于当前代码状态。 |
| v5a：`r1792660166849` | 12/13 通过；修复轮 `running -> running` 报错，自动检查路径错误产生 `E_IO` | 不能将错误 `E_IO` 算作 pxl 的质量检查结论。 |
| v5b：`r1792661955007` | 在第 3 轮第 5 turn 中被 SIGTERM；停止后 `out/<id>` 13/13 通过默认验证；125 次完成调用、4,909,103 token、约 4,705 秒 | **未自然完成、未发布成功、未证明画面好看**；未完成调用的 token/耗时不可补算。 |

v5b 留存的 `assets.gates.json` 是第 2 轮的 gate，**不是 SIGTERM 时重新计算的 gate**：`coverage.queue` 报 13 个 `pending_build`，`pxl.validate` 报两张指纹目录里的旧失败 sheet；`coverage.frames` 与 `coverage.manifest` 当时为通过。当前 `find_sheets()` 递归遍历 attempt 的所有 sheet，不能把旧失败与当前交付混为一谈。指纹相同却被 `mark_dirty` 降级的修复在 `345cc39` 才提交，**v5b 进程没有运行到该版本**。不能从“若使用新代码可能通过”倒推出历史 gate 已通过。

本轮尚**没有独立可复现的图片视觉评审**：评审目录有 `contact-sheet.png`、`v4-vs-v5.png` 及逐项 x8 PNG，但本环境的 `glance`/`pixel_diff` 命令不可用。以下坦克剪影、履带、水波、tile 拼接和 HUD 可读性属于**待验证的视觉假设和验收维度**，不是本轮对图片打出的实测分数。不得引用 `pxl validate` 的 13/13 作为“视觉合格率”。不把不同尺寸、视角或用途的图片相似度当作公平对照。

### 1.1 已实际闭环的部分，不重复立项

- v5b 的 `explosion` 曾有 `anim.burst.N.diff_2_3=38.28% > 35%`，随后重构建并通过；不能称七次历史失败都由同一诊断引起，也不应为此全局放宽帧差阈值。
- 历史 `w` 的 `pulse_glow(steps=2)` 未构建；v5b 改用显式两帧，**没有再次走错误宏签名**，但自动检查对 `b`、`hud_stage_label` 已输出带 id/value/limit 的 `E_QA` 并传到下一修复轮。需用测试单独覆盖“写错宏且不主动 build”的反馈链，不宣称 v5b 已实测这条路径。
- 构建 stdout 不嵌入 `events.jsonl`；`tool_finished` 含 digest/耗时/错误码，模型可见文本在 `tool-results/`，自动报告在 `build-reports/`。保留 artifact 引用与可重放性即可，不应把整段 stdout 重复塞进事件/提示词。
- 静态/动态动画模式、诊断落盘、指纹 dirty 队列、逐回合账本、schema 查询、通用 goal 基础能力已有实现或既有方案；v4 聚焦**交付证据选择、需求契约、真实视觉审阅、受控修复、资源成本**，不重新发明 v3 agent 层。

## 2. 根因树：程序成功、交付可信与视觉合格要分别判定

```
输入（冻结需求 + palette + shapes + recipe）
  -> pxl build / QA：格式、索引、几何与帧差合规？
  -> 当前产物门禁：它是否来自当前指纹、每个要求是否交付？
  -> 视觉审阅：1×画面/动作/地形/UI 是否满足具体美术目标？
  -> 经审阅的发布：证据是否与即将冻结的版本完全一致？
```

| 编号 | 当前证据与症状 | 直接原因 / 边界 | 处理级别 |
|---|---|---|---|
| R1 | v5b 旧 `b`、`hud_stage_label` 的 QA 失败继续令 gate 失败；当前 `out/<id>` 已分别通过 | `assets_stage.rs::find_sheets` 包含指纹目录，`gates` 遍历全部 sheet；`frame_index` / `frame_keys_for` / manifest 也基于目录扫描，成功记录与选中的输出没有统一索引 | P0，纠正当前证据视图；历史失败保留审计，不删除。 |
| R2 | v5b 13 项队列均是 `pending_build`，但此前显式 build 成功 | 相同指纹重写导致降级；`345cc39` 修正此路径，但尚缺组合级复验 | P0，回归并增加启动/发布前不变量校验，不重述为“v5b 已修复”。 |
| R3 | `coverage.frames` 只验证 `{id}_*`，`coverage.manifest` 只验证 delivered + outputs 非空 | 缺按冻结 `AssetRequirement` 的尺寸、方向、动画、帧数、fps/loop、seamless 与真实 sheet 逐项对账；`request_publish` 基于 recipe 和磁盘帧回填，却未独立证明完整 | P0，契约 gate。 |
| R4 | `request_publish` 固定 `visual_review=not_run`；程序 gate 通过可提交 revision | 没有以真实预览为输入的评审与可操作反馈；文本 goal evaluator 不是看图工具 | P1，证据化软审阅；如用户要求视觉签字则成为审批门槛。 |
| R5 | 两轮 60 turn 耗尽；125 次调用约 491 万 token，三次慢调用超过 4.5 分钟 | 包级 agent 长对话、自动检查只在 task 末尾、覆盖优先仅是下轮 prompt；`task_wall_clock_secs` 配置未进入 ledger，且当前累计耗时不等于真实墙钟 | P1，有限轮次/检查点/真实超时和成本可观测性。 |
| R6 | 失败时修复计数可能跨 `E_QA -> E_SCHEMA` 仍视为同类 | `asset_queue.rs::record_result` 先覆盖 `last_error_code`，再调用 `count_repair_and_maybe_block`；直接调用计数函数的单测无法证明端到端正确 | P0，先写真实调用链测试再修正。 |
| R7 | 视觉质量即便通过构建也未被证明；坦克朝向/履带、爆炸动作、砖墙/水拼接、HUD 可读性可能不佳 | pxl 验证检测的是可程序化规则，原创 specs 和语义审美仍需看图/人工判断；x8 单图不能代替 1×、动画及游戏场景 | P1/P2，预览矩阵 + 结构化评审；缺图像能力时明确 `not_run`。 |
| R8 | 现有 `gate.input_digest` 主要取 attempt 路径；`publish-request.json` 的 manifest digest 可能在输入继续写入后失效 | 发布请求、当前输入、gate 报告和最终 revision 未强绑定同一不可变快照 | P0，发布前 TOCTOU 校验与失效机制。 |

**优先级原则：**先修会把真实结果看错的证据/计数缺陷，再收紧确定性契约；在可相信交付对象后才比较视觉，并据视觉反馈做局部修复。不能用“更多 turn、更多色、全局放宽 QA、复制旧 PNG”代替闭环。

## 3. v4 目标、边界和模块归属

1. 对每个 required 资产，任何 gate / manifest / preview / review 都指向**同一份当前成功构建且独立验证的输出**；过时和失败产物可追溯，但不能提供交付证明。
2. 把“构建合规”“需求完整”“视觉审阅”拆成独立结论；同一版本只产生一个可辨认的发布结论，失败或中断不自动变成成功。
3. 在预算范围内让全部 required 项获得首次生成和检查机会，再分配深修资源；视觉修复只改 DSL/shape/recipe，绝不手改导出的 PNG 来蒙混 gate。
4. 无图像 provider 时降级为预览包 + 人工评审，状态 `not_run`/`awaiting_review`；模型不能仅凭文本自报“已看图”，也不能改冻结要求、关闭 QA 规则或篡改 gate。
5. `agent/` 保持通用，不 import `pipeline/` 或 `contracts/`；资产审阅和调度住在 `pipeline/` / `contracts/` / `policy/`。如需通用 image message 扩展，在 `agent/provider.rs` 加媒体承载能力，具体素材量表仍属于 pipeline。
6. 不新增跨进程 run resume 承诺；中断留下可对账报告，未来 resume 单独规划。不改变冻结设计、游戏运行时或 pxl 的全局 QA 阈值；确需改需求时另发 design revision，经审批后再跑。

## 4. 实施批次（每批独立提交、先测试后实现）

### V4-A：当前输出索引与门禁可信化（P0，先决条件）

**触点：**`harness/src/pipeline/asset_queue.rs`、`assets_stage.rs`、`harness/src/adapters/pxl.rs`（若需成功报告元数据）、`harness/tests/cli_e2e.rs`。

1. 定义按 `asset_id + canonical input fingerprint + pxl binary digest + rules/policy version` 键控的成功构建记录：存构建 call_id、输入 digest、输出 sheet JSON/PNG 的相对路径与 digest、独立 validate 结果、报告路径和时间/序列。新字段使用向后兼容反序列化；旧记录缺 digest 时重新验证而非默认为可信。先检查当前 `AssetQueue` 与 `build-reports` 已有字段，复用能证明的内容，避免维护两份相互矛盾的真值。
2. 成功输出采用隔离目录或冻结的产物快照：显式 `out/<id>` 可以作为**工作视图**，但后一构建覆盖同路径时，前一成功指纹不得继续引用当前文件；失败输出只进诊断视图。若短期沿用固定路径，每次 gate 必须复算输出 digest 并匹配当前成功记录；长期首选不可变指纹产物 + 当前输出索引。路径都必须在 attempt 内，不跟随越界 symlink。
3. 将 `gates`、`frame_index`、`frame_keys_for`、`request_publish`、预览构建统一改用 `current_deliverables` 选择器；每个 required ID 恰有一个活动产物，按 asset ID 配对，不用对所有 `*.sheet.json` 递归遍历判交付。`find_sheets` 保留为历史诊断/开发浏览用途；额外 `probe` 不能被算入 required 覆盖或替代要求产物。规则从成功记录/冻结契约取，不能依赖指纹目录 basename 去猜 recipe（现 `rules_for_sheet` 的 fallback 会退化为 `default`）。
4. 若当前输入变更、输出被改写、规则/pxl binary 变化，状态必须失效并要求重新构建/验证；幂等写入同内容保持成功状态。同指纹但曾失败时不得伪装为成功；成功后失败的记录必须按最新尝试/有效版本判定，不依赖 `last_error_code` 是否为空去猜原状态。
5. 合并修复计数步骤：在覆盖 `last_error_code` **前**读取前次失败类；仅同类连续失败累加，成功重置，异类按明确策略重置为 1；对 `E_OTHER` 避免把任意不同错误都归一类。是否允许 blocked 资产因实质变化重新排队需明确并测试，不能让微小改动无限续命。显式与自动 build 走同一计数路径，repair limit 可由 host 配置控制，但不允许模型覆盖。
6. 门禁失败要输出 `asset_id / 当前 fingerprint / 预期输出 / 实际 digest / 失败 check / 报告引用`；旧失败可以出现在审计列表，不能出现在当前 `pxl.validate` failures。每次 gate 消耗资源可统计；避免同一轮重复 `find_sheets` / 重复 validate 无关产物。

**验收：**旧失败 `out/b.<fp>`、`out/hud_stage_label.<fp>` 与新成功 `out/b`、`out/hud_stage_label` 共存时，仅验证当前成功输出；交换输入、覆盖 PNG、覆盖 sheet JSON、改规则/pxl digest、缺当前成功记录均拒绝；相同文件内容写入不降级；`record_failure_and_count` 与 `auto_build` 两条真实路径覆盖 `E_QA→E_SCHEMA`、同类连续 3 次、成功复位、blocked 后行为。利用留存目录作为**只读复现材料**；测试 fixture 用临时目录，不改写历史 run。不宣称历史 v5b gate 自动变绿。

### V4-B：冻结需求逐项核验与原子发布（P0，依赖 A）

**触点：**`harness/src/contracts/design.rs`、`contracts/assets.rs`、`pipeline/assets_stage.rs`、`pipeline/design_stage.rs`（仅设计期校验缺口）、gate/report 序列化。

1. 基于 `AssetRequirement`（`size_px`、`directions`、`anims{name,fps,looped,frames,dirs}`、`semantics`、`seamless`）构建稳定的每资产验收矩阵，按真实 sheet metadata / atlas frame key 交叉核验：`asset_id + anim + dir + frame_index` 每项恰好存在，数量与声明一致、编号无洞/重名；fps 与 loop 不得仅信模型自报的 manifest；frame 画布尺寸与冻结要求一致。不适用的 tile/UI 方向规则明确标记 N/A，而非强行四向。
2. 对 player/enemy tank 分别检查冻结契约要求的四向与 `move`，各自帧数；`w` 的动态/静态动作遵循冻结需求，不通过改名规避；`explosion` 的效果帧与循环语义遵循已批准要求。`kind=tile && seamless=true` 的接缝检查独立做或明确记录未覆盖，不把默认 sprite bbox 规则硬套在地砖上；允许设计明确指定非无缝边界，不靠文件名猜材质。需要 pxl 新 QA policy 时单独变更其 schema/CLI/build/validate 一致性，保持旧 policy 不松绑。
3. 从 A 的成功产物生成 manifest：必须逐项绑定已证实的 recipe/shapes/palette、seed、size、动画元数据和 sheet/PNG digest；`validate_manifest_ids` 加 required 集合去重、路径与 digest/版本匹配的校验。输出路径需位于 revision 可复制的范围，并在 revision 中可重新解析。
4. `request_publish` 生成请求时记录 design revision digest、选中输入集合 digest、manifest digest 与 gate/report digest；每次输入或产物被修改都令发布请求失效。最终 commit 前在同一快照上重新检查引用、需求矩阵、QA 与 policy；报告 `input_digest` 改为规范化内容 digest（包含 design/pxl/gate 版本），不是目录字符串。门禁版本升级并使旧缓存失效。gate 通过**不自动等于**“视觉已审阅”。
5. gate 拆为 `provenance.current_inputs`、`coverage.requirement_matrix`、`coverage.frames`、`coverage.png`、`pxl.validate`、`manifest.binding`、`publish.binding` 等可诊断项；已有 `coverage.frames` 保留兼容语义但不再当唯一充分条件。报告附当前验证的 frame 清单及失败项，不允许“有一个 `{id}_*` 就齐”。禁止无图像证据的人工/模型空口填 `reviewed`。

**验收：**故意缺 E/W/S、`move` 少帧、fps/loop 不符、frame 尺寸不符、重复/跨资产 frame key、只有旧 sheet、空 manifest 动画、输出 digest 被改、请求之后再写 palette，全部失败且能定位到具体要求；有效静态单帧素材仍可通过；合法额外非 required `probe` 不污染交付。需检查输出 copy/commit 后 digest 与路径可复验。

### V4-C：可视化证据与视觉反馈（P1，依赖 A/B）

**触点：**`pipeline/assets_stage.rs`（建议小模块拆分为预览/审阅，不向 `agent/` 泄漏素材语义）、`contracts/assets.rs`、`agent/provider.rs`（仅确实支持 image input 时）、`harness/roles/artist.md`、review artifact/gate。

1. 从**当前成功产物**确定性生成视觉证据包：每项原生 1× 图、统一整数倍最近邻图、二值剪影、深/浅两种背景、带 `asset_id/anim/dir/frame/输入和输出 digest` 标签的 contact sheet；动画生成帧条和按 fps/loop 实际播放的预览（可选 GIF/等效序列）；无缝 tile 做无 padding 的 3×3 重复图和必要的邻接组合；HUD 在游戏目标分辨率中放入示例背景。放大图不替代原生 1× 可读性。
2. 最小可执行路径是**人工审阅 artifact + 结构化记录**，无需增加 VLM 依赖。若 image-capable provider 真实可用，再允许自动软评审：显式传图片字节/URL 和绑定 digest，记录 provider/model/版本/提示词量表/输出及耗时；provider 不支持图片、图像传送失败或只返回文本描述时一律 `not_run`，不得自称看过图。禁止无图像输入的 `GoalEvaluator` 把语言 yes/no 充当视觉判断。
3. 量表按资产角色：坦克分别评估 1×“是坦克/玩家敌人可区分”、N/E/S/W 炮管朝向、主体/履带分区、履带运动与主体稳定；explosion 评估起势→扩张→消散的节奏、非循环终态；`b/x/w/h/g/t` 评估各自冻结语义、tile 重复纹理/接缝与水波动作（禁止从字母名凭空推断材质）；HUD 评估背景上数字/图标可辨性、静态图标与动画意图区分；整包评估 palette、轮廓、光向、像素密度和优先级资产统一性。必要时参照 `miniagent-integration.md` 的坦克观感标准，但不要把样例当作冻结 Battle City 要求。
4. 每条反馈包含：`asset_id`、可选 `anim/dir/frame`、类别、严重度、可观察现象、证据文件+digest、建议改动的源 specs 范围、复核状态；每项可标 `pass/fail/uncertain/not_applicable`，整体不靠单一相似度或未经校准的数值阈值判好坏。无法从图片识别的玩法语义应标“不确定”，由人工确认；不要宣称模型能精确数像素。图片中的文本只作数据处理，不能当指令。
5. 修复只针对最新反馈且局部变动：修改 shape/recipe/palette 后重新 build+validate+需求 gate+预览；同一证据 digest 已审阅过且输入未变可复用记录，变更输入/输出/评审标准即标记 stale。保存 before/after 的对照及解释，允许视觉分歧由人工裁定。避免为一个视觉建议破坏其他方向、动作或 QA。
6. 审批策略明确区分：默认视觉审阅为**非阻断软评审**，如用户要求视觉签字，缺审阅或发现阻断级视觉缺陷时进入 `awaiting_review`、不提交为“视觉已批准”；无需人工签字的流程也必须在 manifest 留 `not_run` 或审阅细节，不能误报通过。`VisualReviewState` 若扩展为 `AwaitingReview/Reviewed` 应保持旧 manifest 的向后兼容并记录审阅主体、digest 与结论，不能只加一个布尔值。

**验收：**替换预览图或改写 specs 会让 review stale；同一素材在两种背景、1×/x8、动画/3×3 中有可追溯命名；无 image provider 时输出可供人看的证据且 `not_run`；给出真实图像输入的 provider 能产出资产级结构化反馈，不能让仅文本 provider 标 `reviewed`。由至少一次人工复核确认问题清单，不预设“v5b 比 v4 好多少分”。

### V4-D：受控调度、及时反馈与墙钟预算（P1，可在 B 后与 C 并行）

**触点：**`pipeline/assets_stage.rs`、`asset_queue.rs`、`agent/runner.rs` 的通用安全回调/检查点（如确需）、`config.rs`、`policy/budget.rs`、`storage/event_log.rs`。

1. 保留冻结要求的优先级，但将“未生成/未检查资产优先”从 `coverage_first_note` 的文字建议升级为 Harness 可强制的**小批次检查点**：优先完成各 required 项第一次 specs+build，再做重复修复；若 provider 仍一次完成 60 turn 而不释放控制，采用有界的 3–8 turn 子任务/按资产任务窗口，自动检查在窗口边界发生，结果作为下个窗口 user 消息注入（绝不产生孤儿 tool message）。具体窗口大小用脚本化及真实跑数据校准，不作为必须新增的配置默认值。
2. 显式 build 失败立即反馈；自动 build 尽量在稳定依赖写完后的最近检查点发生。每资产/窗口 palette 防抖、同指纹去重，防止改公共 palette 后爆发重建。blocked 资产不得继续消费自动重试，给其他资产留首次机会；区分 QA 连续失败、schema 失败、IO/网络/pxl 不可用等基础设施错误，后者先诊断/重试策略，不作为美术失败计数。
3. 每窗口输出结构化 checkpoint：当前有效指纹、队列状态、最近诊断报告/visual feedback digest、未完成需求矩阵、已经消耗与剩余的模型 request/token/真实墙钟、下一资产；`todo.write` 是模型便签，不能覆盖队列真值。长历史仍由 `agent/compress.rs` 的 digest 引用压缩，不能把几十张图片或所有 stdout 重复放进文字上下文。
4. 明确 `task_wall_clock_secs` 为单子任务实际墙钟上限，`wall_clock_budget_secs` 为整 run 上限（如果原字段设计不同须先更新兼容说明）；从单调时钟/持久化 run 起点衡量墙钟，不能只累加 finished 调用 elapsed；请求 timeout、任务 deadline、整 run deadline 取最近值并向 provider/工具传剩余时间。已开始未完成调用记 `interrupted/usage_unknown`，不能编造 token 或把 provider 的慢响应归咎于 harness 卡死。
5. 统一结算模型、工具、自动构建和可选视觉 provider 的资源；设置阶段预算和最低首次覆盖保留配额，token 请求前检查并根据实际能力限制 completion；避免只加大 `max_model_turns_per_task` 而成本上升。若引入新的自动审阅请求，计入同一预算或单独可见子预算，并在预算耗尽时给出未完成列表。
6. 中断时原子落盘最近成功的 gate/queue/checkpoint，gate 报告标明对应 seq、时间与输入 digest，并注明是否 stale；不得在中断后仅看 `out/<id>` 的结果就宣布运行成功。此前 v5b 的 660,505ms、618,747ms 等慢调用可用来测试超时边界，但不反推 provider 一定能真正取消远端请求。

**验收：**FakeProvider 令 explosion 无限 QA 失败，其他 12 项仍先获首次尝试；写坏 `w` 宏不手动 build 能在**下一个检查点**拿到 `E_SCHEMA` hint；同窗口三次 palette 写入按最终指纹构建；任务与运行预算分别生效、超时保留未知用量；预算耗尽时事件、快照、队列和提示引用相互对齐。用 v5b 的耗时/token 仅作成本基线，目标是测出变化，不预先承诺降幅。

### V4-E：Battle City 视觉质量回归与决策（P2，依赖 A–D）

1. 设计冻结不变，在相同尺寸/方向/背景/帧时序下对 v4 和 v5b 的**共同资产**做配对观察；另列 v4 缺而 v5b 有的资产为覆盖提升，不把缺失样本算作同图像“画质退步”。`v4-vs-v5.png` 当前只涵盖 bullet、enemy_tank、explosion、player_tank，不能由此断言其余九项视觉变化。
2. 人工/可选 VLM 对每个资产完成 V4-C 量表，按优先级选 2–3 个质量瓶颈做源配方局部修复；水波与材质需与设计核对，爆炸帧差仅作为 QA 约束，不能凭默认门限把剧情效果都做成一样的两帧；参考 `plan/pxl-quality-improvement-plan.md` 的“先剪影、再配色/面材质、最后动作”单变量对照，避免同时变尺寸/调色/动作后无法归因。
3. 测试产物记录：每项首次完成时间、build/validate 成败、修复次数、有效/旧失败产物、需求矩阵、视觉问题数/严重度、人工结论、prompt/completion token、请求数、wall time、最终提交状态；对跑次结束原因分别用 `completed/failed/budget_exhausted/interrupted`，不能拿一个 SIGTERM run 证明闭环通过。
4. 真实模型复跑只在脚本测试与人工审阅流程就绪后进行，锁定 design revision、pxl binary digest、provider/model、规则版本、配置和预算；另开 run，不改 v5a/v5b 数据或原评审结论。若游戏运行时没有启动，只报告素材层审阅，不报告游戏内实装观感。

**验收：**生成可追溯的逐项证据表和问题归因；程序完整性达到所有 required 项均为当前版本有效且需求矩阵通过；视觉结论必须有已审预览和 reviewer 记录，未审项维持 `not_run`；不得用高 token、单纯复制模板、改变 design revision 或放宽默认 QA 获得表面成功。

## 5. 建议数据契约、状态机与失效规则

最小交付视图（字段示意，**实施前核对现有队列以避免重复**）：

```
AssetDeliverable {
  asset_id, design_revision_digest, input_fingerprint,
  pxl_binary_digest, qa_policy_digest, build_call_id,
  source_paths_and_digests[], sheet_path_and_digest,
  png_path_and_digest, validation_report_and_digest,
  frame_inventory[], status
}
VisualEvidence {
  asset_id, deliverable_digest, preview_kind,
  preview_path_and_digest, renderer_version
}
VisualFinding {
  asset_id, deliverable_digest, evidence_digest,
  anim?, direction?, frames?, severity, observation,
  source_edit_hint?, reviewer, status
}
```

需要统一以下不变量：

- `Current(A)` 当且仅当输入指纹、冻结设计、pxl 二进制、QA 规则和输出文件 digest 全部匹配且 build+validate 成功；有记录却缺文件与缺记录语义不同，但两者都不能交付。
- `Present(A)` 不蕴含 `Complete(A)`；`Complete(A)` 必须覆盖当前 frozen requirement 的各 anim/dir/帧/尺寸等，且 manifest 与实际 atlas 一致。
- `Reviewed(A)` 必须绑定 `Current(A)` 的完整 digest 和实际图片证据；改动任一输入/输出/证据后自动失效。`Reviewed` 仍需区分通过、失败、待人工裁定，单个枚举值不足以表达审阅结果。
- `Publishable(package)` 需要所有 required `Current ∧ Complete`，发布请求未过期；若要求视觉签字，还要所有指定项目有通过的审阅/人工批准。未启用视觉签字时允许程序发布，但清楚标注 `visual_review=not_run`，不可称为“视觉通过”。
- 软性观感诊断与硬性可程序化 gate 分开；VLM 失败/超时不会偷偷改写 `pxl.validate` 的通过值，也不会让模型自行降低冻结需求。

旧 run 的事件/gate 继续可读，新增字段 `serde(default)` 或显式 schema version；旧 `VisualReviewState::Reviewed` 如果无图片/digest 不可自动升级为新版“已批准”。导出 revision 前应检查报告路径是否仍可访问，必要时将审阅证据和 digest 纳入 revision 或稳定审计引用。

## 6. 回归矩阵、执行方式和完成定义

| 批次 | 无网络确定性测试 | 人工/真实模型验证 |
|---|---|---|
| A | 旧失败+新成功共存；当前输出覆盖/缺失；指纹相同幂等；规则变化；`E_QA→E_SCHEMA` 与同类计数；显式/自动 build 等价 | 复算一份 v5b 的只读诊断视图（**新 gate 报告**，不改历史 gate）。 |
| B | 缺方向/少帧/错 fps/loop/size、重复帧、manifest 虚报、过期发布请求、跨目录输出 | 人工抽检 manifest 中每个动画映射是否与 atlas 对应。 |
| C | 固定输入生成预览 digest；修改输入使审阅 stale；纯文本 provider 不能 Review；无 provider 留 `not_run` | 审阅 1×、x8、动作、3×3 拼接和 UI 场景，逐项确认或标不确定。 |
| D | FakeProvider 诱发饿死/慢调用/预算中断；自动 `w` 失败在下一 checkpoint 被读到；palette 防抖 | 限额下复跑，比较首次覆盖、调用数/token 与墙钟。 |
| E | 固定 fixture 的质量回归；事件—报告—manifest digest 链一致 | 锁定配置真实复跑 + 人工签字；报告仅声明实际验证过的维度。 |

建议执行（实现完成后、无需真实 provider）：

```sh
cargo test -p pxl-harness
cargo test -p pxl-qa -p pxl-ops -p pxl-format -p pxl-cli
cargo test --workspace
```

实施前保留工作区未提交改动并按批次审阅 diff；测试只写临时目录，不在 `review/`、现有 `games/*/runs/` 或冻结设计上做改写。本计划尚未执行以上测试，也没有重新构建 v5b 或运行新模型。完成 v4 的判据不是“某张图可以打开”，而是**可重放的当前证据、完整冻结契约、诚实的视觉审阅状态、受控预算和可审计的发布决策**同时成立。
