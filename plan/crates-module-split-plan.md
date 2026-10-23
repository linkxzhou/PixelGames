# crates 长文件模块化拆分计划

> 日期：2026-10-23（重整版）。状态：**已完成**（6/6 批次）。
> 硬性目标：`crates/` 下每个 `.rs` 文件 **低于 600 行**（含测试）。

## 原则

- 按职责拆分，不按行数截断；函数原样搬移，不改算法、阈值、错误信息、check id、输出字节。
- 保持公开 API 路径、CLI 参数、退出码、测试 target 名（`cli`/`golden`）不变。
- 只做搬移、模块声明、必要 import、最小可见性（`pub(super)`/`pub(crate)`），不加依赖、不改 Cargo 配置。
- 不允许旧入口与 `xxx/mod.rs` 并存（E0761）；迁移后删除旧文件。
- 每批独立编译与测试通过后才进入下一批；黄金哈希不得修改。

## 进度总表

| # | 文件 | 原行数 | 状态 | 拆分后最大文件 |
|---|---|---:|---|---:|
| 1 | `pxl-cli/tests/cli.rs` | 1650 | 已完成 | 381 |
| 2 | `pxl-export/src/lib.rs` | 835 | 已完成 | 325 |
| 3 | `pxl-ops/src/apply.rs` | 1379 | 已完成 | 280 |
| 4 | `pxl-ops/src/macros.rs` | 1092 | 已完成 | 310（tests.rs） |
| 5 | `pxl-qa/src/lib.rs` | 1433 | 已完成 | 335（tests/animation.rs） |
| 6 | `pxl-cli/src/cmd.rs` | 2356 | 已完成 | 408（qa_input.rs） |

其余文件均已 < 600 行（最大 `pxl-core/src/palette.rs` 473），本轮不动。

## 已完成批次摘要

1. **CLI 集成测试**：`tests/cli.rs` 为 6 行入口；`support/mod.rs` + `cli_cases/{contract,static_anim,recipes,sheet_input,publish,palette_tiles,qa_assets,render}.rs`。40 个测试全部保留，过滤路径变为 `cli_cases::<模块>::<原名>`。
2. **export**：`lib.rs` 17 行重导出；`png/gif/preview/raster/sheet.rs` + `tests/support.rs`。raster 承载缩放与 composite，消除 png↔preview 双向依赖。17 个公开项、10 个单测保留。
3. **apply**：`apply/mod.rs`（ApplyCx + 调度）+ `region/transform/color/pixels.rs` + `tests/{mod,transform,color,pixels}.rs`。`crate::apply::rotate90` 以 `pub(crate)` 保留给宏测试。

## 待实施批次

### 4. 动画宏 `pxl-ops/src/macros/`

`pub mod macros` 保留；`lib.rs` 根级重导出不变。

| 文件 | 内容 |
|---|---|
| `mod.rs` | 公共项重导出、`expand_macro` 调度 |
| `catalog.rs` | `MACRO_NAMES`、allowed/default/required、`macro_frames`、`MacroArgInfo`/`MacroInfo`、`macro_info`、`example_call`、`macro_catalog` |
| `args.rs` | `parse_macro_call`、`check_args`、`arg_i32`、`arg_str`、`steps_of` |
| `tank.rs` | `tread_cycle`、tread 检测/区域 helper、`idle_rumble` |
| `character.rs` | `idle_breathe`、`bob`、`walk_side`、`blink` |
| `effects.rs` | `hurt_flash`、`death_dissolve`、`ring_color`、`spin_projectile`、`pulse_glow` |
| `doll.rs` | `doll` |
| `tests.rs` | 原单测整体迁移（约 290 行） |

### 5. QA 规则 `pxl-qa/src/`

| 文件 | 内容 |
|---|---|
| `lib.rs` | 模块声明与原公共项重导出 |
| `model.rs` | `Check`、`Report`、`AnimSeq`、`ValidateInput`（`ramp_for` 改 `pub(crate)`） |
| `validate.rs` | `validate` 调度（check 顺序不变） |
| `common.rs` | palette/frame 基础检查、terminal-empty、bbox/百分比/fail |
| `animation.rs` | `scoped`、`anim_diff`、`shift_residual_bps` |
| `tank.rs` | tread/hull/facing/counts |
| `tile.rs` | `tile_checks`、`tile_seams` |
| `character.rs` | 角色检查与镜像/查找/脚线 helper |
| `tests/` | `mod.rs`（夹具）+ `animation.rs`、`rules.rs` 按行为分组，各 < 600 |

### 6. CLI 命令 `pxl-cli/src/cmd/`

`cmd.rs` → `cmd/mod.rs`；`lib.rs` 的 `cmd::dispatch` 与参数类型路径不变。

| 文件 | 内容 |
|---|---|
| `mod.rs` | 模块声明、参数类型重导出、`dispatch` |
| `args.rs` | `GenArgs`…`BuildArgs` |
| `generate.rs` | `gen` 与 palette/mask/spec helper |
| `document.rs` | `doc_from_frames`、`collect_ops`、`apply_with`、`load_parts`、`frames_vec` |
| `operations.rs` | `op_cmd`、`tile_cmd` |
| `animation.rs` | `anim_cmd`、`default_fps`、`check_anim_timing` |
| `palette.rs` | `palette_cmd`、`apply_palette_op` |
| `sheet.rs` | `sheet_cmd`、`Loaded`、`pack_loaded`、行/单元布局 helper |
| `render.rs` | `render_cmd`、`looks_like_png` |
| `inspect.rs` | `inspect_cmd`、`validate_cmd`、`checks_json`、`load_report` |
| `qa_input.rs` | `doc_input`、`sheet_input` 及 palette/crop/json helper |
| `catalog.rs` | `schema_cmd`、`list_cmd` |
| `build.rs` | staging 常量、`build_cmd`、`publish_tree`、`build_into` |
| `sprite.rs` | `overwrite_ctx`、`redraws_canvas`、`build_sprite` |

若某文件实际超过 600 行，再按上表中的子职责（如 sheet 布局、qa_input 的 sheet 解析）进一步拆分。

## 验证（每批）

```sh
cargo check -p <crate> --all-targets
cargo test -p <crate>
cargo test -p pxl-cli --test cli --test golden
```

最终：`cargo test --workspace`；行数检查 `find crates -name '*.rs' -exec wc -l {} + | awk '$1>=600 && $2!="total"'` 输出为空；变更文件 `rustfmt --check`、`git diff --check` 通过。

## 实施记录

- 批次 1–3：见上文摘要，均通过对应库测试、CLI 40 项、黄金 1 项。
- 批次 4 macros：按计划拆为 8 个文件；`pub mod macros` 与根级重导出不变；30 个 pxl-ops 单测、CLI 40、黄金 1 通过。
- 批次 5 qa：按计划拆为 lib/model/validate/common/animation/tank/tile/character + `tests/{mod,animation,rules}.rs`；`ramp_for` 为 `pub(crate)`；14 个单测通过。
- 批次 6 cmd：迁为 `cmd/` 下 14 个文件（mod 57 行），脚本校验 2330 行有效内容全覆盖且无重复；`sheet.rs` 389、`qa_input.rs` 408 均 < 600，未再细分。
  - 修复：`build.rs` 内 `include_str!` 相对路径因目录加深一级而调整为 `../../../../assets/...`（内嵌内容不变）。
  - 拆分后的多余 import 由 `cargo fix` 清理，`cargo check -p pxl-cli --all-targets` 0 warning。
- 最终验收：`cargo test --workspace` 全绿（含 harness 99 项、CLI 40、黄金 1）；`crates/` 下无 ≥ 600 行的 `.rs`（最大 `pxl-core/src/palette.rs` 473）；`rustfmt --check`、`git diff --check` 通过；无旧入口残留，未改 Cargo 配置与黄金文件。
