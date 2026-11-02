# 08 · CLI 命令面与批处理

> 日期：2026-11-02（Asia/Shanghai）  
> 状态：**部分完成**（2026-11-02 Asia/Shanghai）  
> 证据：`step-300 ase --help` 及 inspect/convert/sheet/list/filter；见 cli-flag-matrix.md
> 对照：公开 https://www.aseprite.org/docs/cli/ ；本地 `src/app/cli/app_options.h`、`app_options.cpp`、`cli_processor.*`  
> 主方案：**扩展 `step-300`（`pxl-cli`）增加 `ase` 子命令组**；与现有子命令共存

## 1. 背景

Aseprite 的 `--batch` 意为「不要启动 UI」。我们的工具天生无 UI，因此：

- 不需要伪造一个必须带 `-b` 才能跑的程序；  
- 但为了**脚本可移植**，`step-300 ase` 应接受并忽略/兼容 `-b/--batch`，并实现同一套导出旗标语义。

现有 `pxl-cli`（`src/lib.rs`）子命令包括：`gen`、`ascii`、`op`、`anim`、`palette`、`tile`、`sheet`、`svg`、`sprite`、`prototype`、`scene`、`audio`、`pixelize`、`profile`、`shapes`、`render`、`enhance`、`inspect`、`validate`、`build`、`plan`、`schema`、`list`、`mcp`(未实现)。**无**文档级 ase 批处理。

## 2. 主方案（冻结）

```text
step-300 ase [全局兼容旗标] <输入...> [动作旗标...]
step-300 ase inspect <file>
step-300 ase filter <name> ...
step-300 ase op --ops ops.json ...
```

- **库编排**：`pxl-ase` facade 解析「类 Aseprite」顺序敏感参数（官方 CLI 对 flag 与文件先后顺序敏感，如 `--split-layers` 须在文件前）。  
- **共存**：禁止修改现有子命令默认行为；文档写清何时用 `sheet` vs `ase --sheet`。  
- **可选**：`pxl-ase` 二进制 = `step-300 ase` 别名（后置）。

### 为何不选「完全独立 `ase` 二进制替代 step-300`」

五大步与 skill 已绑定 `step-300`；独立二进制会分裂入口。子命令组足够隔离。

### 为何不选「把所有公开 flag 挂到根命令」

会与现有 `sheet`/`render` 参数命名冲突、破坏 clap 结构。故放在 `ase` 命名空间。

## 3. 目标

1. 兼容矩阵：公开 Options 一节每个 flag → 支持/部分/不做/扩展。  
2. 顺序敏感解析器（可参考官方行为测试，不抄 C++）。  
3. `--preview`：只打印计划不写盘。  
4. `--verbose`：stderr 日志；JSON 模式与全局 `--json` 协调。  
5. 脚本友好：退出码；`--overwrite` 继承全局。  
6. 扩展子命令：`filter`/`op`/`inspect`（官方无同名 flag 的能力）。

## 4. 非目标

- `--shell` Lua REPL（见 10）。  
- Steam `--noinapp`、WinTab。  
- 启动 UI 的任何路径。  
- 实现 `mcp`（另案）。

## 5. 与 steps 对照表

| 能力 | 标注 | 证据 |
|---|---|---|
| clap 全局 `--json/--seed/--strict/--overwrite` | 【已有·可复用】 | `pxl-cli/src/lib.rs` |
| `sheet`/`render`/`inspect`/`build`/`plan` | 【已有·可复用】 | 各 `cmd/*.rs` |
| `palette`/`op`/`anim` | 【已有·可复用】 | 同上 |
| 类 Aseprite 顺序批处理解析器 | 【缺失】 | — |
| `--filename-format` 占位符全集 | 【缺失】 | sheet 侧有自己命名 |
| `--list-layers` 等 | 【缺失】 | `list` 子命令列的是 generators/ops，不是文档层 |

## 6. 公开 CLI 映射矩阵（初稿）

| Flag | 计划 | 备注 |
|---|---|---|
| `--batch` / `-b` | 兼容忽略 | 默认无 UI |
| `--preview` / `-p` | 做 | 干跑 |
| `--save-as` | 做 | 05+06 |
| `--palette` | 做 | 02 |
| `--scale` | 做 | 复用 `pxl-core::scale_*` / 最近邻 |
| `--shrink-to` | 做 | |
| `--dithering-algorithm/matrix` | 做 | 02 |
| `--color-mode` | 做 | 02 |
| `--data` / `--format` | 做 | 05；补 json-array |
| `--sheet` + sheet-* | 做 | 05；packer 映射 |
| `--split-*` | 做 | 07 |
| `--layer` / `--all-layers` / `--ignore-layer` | 做 | |
| `--tag` / `--frame-range` / `--play-subtags` | 做 / 部分 | play-subtags P1 |
| `--ignore-empty` / `--merge-duplicates` | 做 | |
| `--*-padding` / `--trim*` / `--extrude` / `--crop` / `--slice` | 做 | |
| `--filename-format` / `--tagname-format` | 做 | |
| `--script` / `--script-param` | 后续 | 10 |
| `--list-layers` / hierarchy / tags / slices | 做 | |
| `--oneframe` | 做 | 06 |
| `--export-tileset` | 部分 | 依赖 tilemap 模型进度 |
| `--shell` | 不做（主线） | 10 可选 |
| `--verbose` / `--debug` / `--help` / `--version` | 做 | debug 不必写桌面文件 |
| `--noinapp` | 不做 | Steam |

## 7. 工作包 / 里程碑

### M7.0 — clap 结构与解析实验

- `AseArgs`：原始 `Vec` 顺序解析 vs 结构化；原型选一。  
- **验收**：用例「`--split-layers in.ase --save-as out.png`」参数序测试。

### M7.1 — P0 导出闭环

- 读 ase → 滤层/帧 → save-as png/gif → sheet+data。  
- **验收**：公开文档「Convert」「Export sheet」用例脚本。

### M7.2 — list-* 与 preview

- **验收**：输出格式稳定；`--json` 包装。

### M7.3 — 扩展 filter/op

- **验收**：04/03 接通；help 文本标明「扩展，非官方 flag」。

### M7.4 — 兼容性与文档

- `step-300 ase --help` 指向本矩阵；skill 文档更新属代码阶段。  
- **验收**：help 快照测试。

## 8. 示例（目标形态）

```bash
# 兼容 Aseprite 批处理习惯
step-300 ase -b player.aseprite --scale 2 --save-as player-x2.png

step-300 ase -b anim.aseprite --sheet sheet.png --data sheet.json --format json-hash

step-300 ase -b --layer "Body" hero.aseprite --save-as body.gif

# 扩展
step-300 ase filter outline -i hero.aseprite -o hero-out.aseprite --width 1
```

## 9. 验收

- 矩阵中 P0 行均有自动化测试或明确人工项。  
- 不破坏现有 `pxl-cli` 子命令黄金测试。  
- 未跑：`NOT_ASSESSED`。

## 10. 风险

- 顺序敏感与 clap derive 冲突 → 可能对 `ase` 使用自定义 parser。  
- 用户以为根级 `sheet` 会读 `.aseprite` → help/错误提示引导到 `ase`。

## 11. 依赖

- 依赖：05、06、07、02、03、04。  
- 被依赖：09。
