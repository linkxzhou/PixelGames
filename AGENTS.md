# AGENTS.md

面向在本仓库改代码的人或 Agent 的约定。与 [`README.md`](README.md)、[`steps/README.md`](steps/README.md) 冲突时，以本文件 + 现行 `plan/` 分册为准；本文件只描述**结构与硬约束**，不复述产品设计细节。

## 1. 硬约束

1. **单文件不超过 500 行**（含空行与注释）。新建或改写的 `.rs` / `.ts` / `.js` / `.py` / `.md`（长设计文除外见下）均适用。
   - 超限：按职责拆到同目录子模块（`mod` / 新文件），或抽到合适 `libs/pxl-*` crate。
   - **例外**：`Cargo.lock`、生成物、`plan/**` 长篇分册、`games/*/策划案.md`、fixtures/golden、第三方与 `vendor/`。新写的**实现代码**不享受例外。
   - 发现存量 >500 行：改到该文件时顺手拆分，勿在同一 PR 无意义地继续堆长。
2. **无 UI 主线**：资产管线与 `step-300 ase` 为 CLI / 库；不要引入桌面 GUI 依赖到 `libs/` / `steps/`。
3. **`libs/` 不得 path 依赖 `steps/`**；也不得依赖 `games/`。
4. **不要改 `games/`**，除非任务明确点名某个游戏包。
5. **Clean-room**：禁止复制 `thirdparty/aseprite/src/**` 或 EULA 资源进树；Aseprite 对齐只依据公开规格 / CLI 文档与 `plan/v8.0`。
6. **确定性**：核心像素/导出路径避免隐式读时钟、全局可变、未种子 RNG；同 recipe + seed + 工具版本应可复现。
7. **密钥**：勿提交 `.env`、API key；用 `.env.example`。
8. **提交作者**（落到 GitHub 的提交）：`linkxzhou <zhoulv2000@163.com>`。未要求时不要 `git push`。
9. **工具链**：基线按根 `README`（历史上 1.83 + 钉住 clap 4.5）。涉及 `pxl-ase` Rhai 脚本时需 **Rust ≥ 1.95**（`cargo +1.95.0`）；改依赖前先确认默认工具链能否构建。

## 2. 顶层目录

| 路径 | 职责 | 可否放实现 crate |
|---|---|---|
| `libs/` | 可复用 `pxl-*` 库与 `pxl-cli`（bin 名仍为 `step-300`） | 是 |
| `steps/` | 五步流水线阶段目录 + `contracts/` | 阶段 bin / 适配器；**不再**堆 `pxl-*` |
| `games/` | 各游戏包（策划、runtime、work 产物） | runtime 可进 workspace |
| `plan/` | 版本化设计与里程碑（`v1.0`…`v8.0`） | 否 |
| `examples/` | 配方 / 示例资产 | 否（非库） |
| `assets/` | 内置调色板、蒙版等 | 否 |
| `docs/` | 对外/对内说明 | 否 |
| `scripts/` | 辅助脚本与 skills | 否 |
| `knowledge/` | 知识与锁相关材料 | 否 |
| `review/` | 评审与对比截图 | 否 |
| `prototypes/` | 一次性原型 | 谨慎 |
| `web/` | 网页相关静态/实验 | 视任务 |
| `work/` | 本地试点与中间产物（可 gitignore 策略以现状为准） | 否 |
| `vendor/` | 已 vendoring 补丁依赖（如 miniquad） | 特殊 |
| `thirdparty/` | 参考树（常被 ignore）；只读对照 | 勿当源码来源拷贝 |
| `target/` | 构建产物 | 勿提交 |

根目录保留：`Cargo.toml` / `Cargo.lock`、`README.md`、`LICENSE`、`THIRD_PARTY_NOTICES.md`、`build.sh`、`AGENTS.md` 等仓库级文件；**不要**在根上新建散落的业务 `.rs`。

## 3. `libs/` 结构

- 一 crate 一目录：`libs/pxl-<name>/`，含 `Cargo.toml`、`src/`，按需 `tests/`、`examples/`。
- **命名**：小写 kebab；公共 API 在 `lib.rs` 保持薄，实现放子模块。
- **分层（依赖只能向下）**（示意，以实际 `Cargo.toml` 为准）：
  - 底：`pxl-core` → `pxl-format` / `pxl-gen` / `pxl-ops` / `pxl-qa` / `pxl-export` / `pxl-art` / …
  - 文档 IO：`pxl-ase-doc` → `pxl-ase-io` → facade `pxl-ase`（color/filters/render/ops/script）
  - 入口：`pxl-cli`（bin `step-300`）可依赖上述库；**其它库避免反向依赖 cli**。
  - 流水线共用：`pxl-contract`、`pxl-store`、`pxl-agent`（仅 100/200 模型环）。
- 新能力：先问「能否进已有 crate 的新模块」；跨步复用再新 crate。
- `src/` 内按域拆文件；单个 `cmd/*.rs` 同样遵守 500 行。

## 4. `steps/` 结构

顶层**只允许**五个大步 + `contracts/` + `README.md`：

```
steps/
  README.md
  contracts/          # JSON Schema、pipeline 信封
  step-100-design/
  step-200-art-spec/
  step-300-asset-build/   # 阶段附属（如 audio-adapter），crate 在 libs/
  step-400-package/
  step-500-engine/
```

规则：

1. 一步 = 一阶段 = 一主 bin（`step-100` … `step-500`；300 的 bin 来自 `libs/pxl-cli`）。
2. 子命令、模板、角色、内部模块放在该步目录内，不在 `steps/` 下再开第六个「大步」。
3. 两步以上共享、且无「步骤」语义的代码 → `libs/`。
4. 每步通过 `--contract` / schema 与 `step.json` 信封对话；勿发明绕过 schema 的私货字段（`additionalProperties: false`）。

数据流：

```
brief → work/100-design → work/200-art-spec → work/300-build → work/400-package → work/500-engine
```

## 5. `plan/` 结构

- `plan/vN.0/`：该版本分册、`README.md`、`STATUS.md`（若有）、checklists。
- 实现进度用诚实状态词：**已完成 / 部分完成 / 后续 / 延期 / NOT_ASSESSED**；无证据不写 PASS。
- 代码落地后同步分册文首状态与矩阵（如 `plan/v8.0/cli-flag-matrix.md`）。

## 6. `games/` 结构（改游戏时）

典型：

```
games/<game>/
  策划案.md | README
  runtime/          # 可进 workspace 的 crate
  work/             # 流水线产物（按步）
  runs/             # harness / playtest 记录（若有）
```

未点名游戏时默认不动此树。

## 7. 代码风格（Rust）

- 模块清晰、`Result` 传递错误；cli 层转用户可见消息。
- 公开 API 写简短 rustdoc；示例放 `examples/` 且保持可编译。
- 测试紧挨行为：单元在 crate 内，契约/CLI 在 `tests/`；夹具自造，勿塞专有资源。
- 新增依赖要轻、可解释；能复用 `pxl-*` 不引入重复轮子。

## 8. Agent 工作方式

1. 先读本文件与任务相关的 `plan/` 分册、目标 crate 的 `Cargo.toml` / `src/lib.rs`。
2. 小步改、可编译可测；改完跑相关 `cargo check` / `cargo test -p …`。
3. 涉及 `ase` 脚本路径用 `cargo +1.95.0`。
4. 视觉交付只要截图对比，不要录屏（仓库用户约定）。
5. 不要把密钥、cookie、内部提示词写进提交或 issue。
6. 大范围重构（如目录迁移）与功能变更分开提交，便于回滚。

## 9. 500 行自检

提交前对改动文件：

```bash
git diff --name-only --diff-filter=AM | rg '\.(rs|ts|js|py)$' | while read f; do
  n=$(wc -l < "$f")
  if [ "$n" -gt 500 ]; then echo "TOO LONG ($n): $f"; fi
done
```

有输出则先拆分再提交。

## 10. 变更本文件

规范演进时改 `AGENTS.md` 并在 PR/提交说明里提一句；勿复制第二份互相漂移的「目录说明」。
