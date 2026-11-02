# 09 · 与 steps 集成与迁移

> 日期：2026-11-02（Asia/Shanghai）  
> 状态：**已完成**（2026-11-02 Asia/Shanghai）  
> 证据：`ase import` → ase-import/v1 manifest + schema-valid Recipe；pilot 含 flood-fill/hue-sat/flags/import；steps/README；未改 games/；§4.3 P0 已关或显式延期

## 1. 背景

五大步：

- `step-100-design` → 视觉意图  
- `step-200-art-spec` → recipe / palette / visual-plan（不构建最终图）  
- `step-300-asset-build` → 本版主战场（`pxl-*` crates + 二进制 `step-300`）  
- `step-400-package` → 打包校验  
- `step-500-engine` → 运行时消费  

v8 提供「外部 Aseprite 文档 ↔ 内部确定性资产」桥梁，而不是替换 recipe 生成哲学。

## 2. 目标

1. Cargo workspace 成员增加 `pxl-ase-*`（路径在 `steps/step-300-asset-build/` 下）。  
2. `pxl-cli` 增加 `ase` 子命令；CI 与 skill 调用约定。  
3. 定义 `AseDoc → PxlDoc` / 帧目录 / palette.json 的导入契约。  
4. 试点：选一个小 `.aseprite`（自绘）→ `ase --sheet` → 可选接入 `plan` 索引。  
5. 缺口清单可勾选关闭。  
6. 明确**不**自动改 `games/`。

## 3. 非目标

- 在 step-100/200 内嵌 Aseprite。  
- 强制美术改用 `.aseprite` 源（recipe/ASCII/SVG 仍一等）。  
- 一次迁移所有历史素材。

## 4. 与 steps 对照：已有 vs 缺口总清单

### 4.1 【已有·可复用】（实施时应直接调用）

| 项 | 位置 |
|---|---|
| 索引帧 / 调色板 / outline / scale / 文本矩阵 | `pxl-core` |
| Op DSL + apply + macros | `pxl-format`、`pxl-ops` |
| PNG/GIF/sheet/preview | `pxl-export` |
| 生成 / shapes / noise / prototype | `pxl-gen` |
| SVG 栅格 | `pxl-svg` |
| 量化 / MMPX / rotsprite / pixelize 管线 | `pxl-art` |
| QA | `pxl-qa` |
| 引擎侧 doc helpers | `pxl-engine` |
| CLI 入口与全局旗标 | `pxl-cli` → `step-300` |
| 音频离线 | `pxl-audio`（与 ase 无强绑） |
| 批构建索引 | `cmd/plan.rs`、`cmd/build.rs` |

### 4.2 【部分】

| 项 | 现状 | 缺口 |
|---|---|---|
| Aseprite JSON sheet | 可写 hash + frameTags | 缺 json-array、layers/slices 完整 meta、从多层渲染 |
| 动画 | Anim/宏/≤16 帧 | 缺文档 Tag/Slice/linked；长动画导入策略 |
| 调色板 | 强 | 缺文档色模状态机、GPL、CLI `--color-mode` 流水线 |
| Outline | core/op | 缺完整 outline_filter 参数面 |
| `list` 命令 | 列 gen/ops | 不列文档层/标签 |

### 4.3 【缺失】→ 关闭状态（M8.3，2026-11-02 Asia/Shanghai）

| # | 项 | 状态 | 证据 / 备注 |
|---|---|---|---|
| 1 | `AseDoc` 多层时间轴模型 | 已关闭 | `pxl-ase-doc`；M1 |
| 2 | `.aseprite` 编解码 | 已关闭 | `pxl-ase-io`；M2 |
| 3 | 层合成 blend | 部分/延期 | flatten 已有 normal 合成；高级 blend mode 延期 |
| 4 | 滤镜集（hue/replace/…） | P0 已关闭 | hue-saturation/replace/outline/invert/brightness-contrast；convolve/median/curve **延期**（P1/P2） |
| 5 | 类官方批处理解析与 flag 矩阵 | 部分/延期 | 矩阵已刷新；顺序敏感 argv **延期**；P0 旗标子集已挂 |
| 6 | Tag/Slice 一等编辑与 split/list | 已关闭 | M6；list/split CLI |
| 7 | 正式 `Ase → recipe/PxlDoc` 导入契约 | 已关闭 | `ase import` + `ase-import/v1` + schema-valid Recipe |

## 5. 契约草案（待 P 任务冻结）

### 5.1 导入产物建议

```text
work/<pilot>/ase-import/
  source.aseprite          # 只读输入（或哈希引用）
  import-manifest.json     # 工具版本、源哈希、色模、帧数
  palette.json             # 若 indexed
  frames/*.png 或 *.pxl.json
  sheet.png + sheet.json   # 可选
```

`import-manifest` 字段（草案）：`schema:"ase-import/v1"`、`source_sha256`、`pxl_version`、`frame_count`、`tags[]`、`warnings[]`。

### 5.2 与 `asset-build` index

- `step-300 plan` 今日写 package 用 `index.json`。导入帧若要进包，应生成合法 recipe 或登记为已构建资产——**不得**塞未知字段躲过 schema（现多处 `additionalProperties: false`，v7 已强调）。  
- 新能力用能力 ID 协商（例：`ase_import_v1`），读者不支持则失败。

### 5.3 step-200

- 可选：规格中允许 `source.kind = "aseprite"` 路径引用；**默认不做**，试点成功后再开。  
- lint 仍禁止 step-200 构建最终图。

### 5.4 step-400/500

- 消费端继续认现有 sheet/palette；只要导入桥产出旧格式，下游零改。  
- 若 JSON meta 增字段，须保持旧解析可忽略。

## 6. 工作包 / 里程碑

### M8.0 — Workspace 接入设计

- 成员列表、feature 开关、`CARGO_TARGET_DIR` 约定（对齐 v7 P00 的 `+1.95.0`/`--locked` 习惯若仍适用）。  
- **验收**：设计进本文件；代码阶段再改 `Cargo.toml`。

### M8.1 — 导入命令

- `step-300 ase import -i x.aseprite -o dir/`  
- **验收**：目录结构符合 §5.1；`--json` 摘要。

### M8.2 — 试点

- 自绘 ≤8 帧、2 层小角色；跑通 import → sheet →（可选）手工挂 examples。  
- **验收**：截图/哈希附证据；**不**改 `games/`。人工视觉 `NOT_ASSESSED` 除非有评审记录。

### M8.3 — 缺口清单关闭流程

- 维护「4.3 列表」勾选；每项链接测试名。  
- **验收**：M8 结束时 P0 缺口全关或显式延期。

### M8.4 — 文档与 skill

- 更新调用说明（代码阶段）；plan 只要求预留章节。  
- **验收**：08 help 与 09 交叉链接。

## 7. 建议实施顺序（给执行者）

1. M0（00）许可与矩阵  
2. 01 文档模型  
3. 06 IO 读  
4. 05 flatten + 复用 export（可先 PNG）  
5. 02 色模 / 08 P0 CLI  
6. 07 tags  
7. 03/04 编辑与滤镜  
8. 06 写 + 09 导入试点  
9. 10 可选脚本  

## 8. 验收总则

- 集成测试不得依赖本机安装的 Aseprite GUI 二进制（可选增强测试可标 `#[ignore]`）。  
- 任何「对齐 Aseprite」声明必须附：公开文档章节或夹具对比记录。  
- 未跑：`NOT_ASSESSED`。

## 9. 风险

- 双源真相（recipe vs ase）→ 试点只读导入，编辑仍回 recipe 或明确「ase 为源」。  
- EULA：试点夹具用自绘文件，不用商业 sprite 资产。

## 10. 依赖

- 依赖：00–08。  
- 交叉：v7 现代规格可消费导入后的 palette family（未来），本版不阻塞 v7。
