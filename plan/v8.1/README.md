# v8.1 计划索引：Aseprite 对齐缺口审计与增补

> 日期：2026-11-02（Asia/Shanghai）  
> 主题：**对照 `thirdparty/aseprite` 全量功能面，审计 [v8.0](../v8.0/README.md) 的覆盖完整性，把遗漏项整理为可执行的增补任务**  
> 方法：静态阅读（本地源码树 + 公开规格 `thirdparty/aseprite/docs/ase-file-specs.md` + `src/app/cli/app_options.h` 全旗标清单）与 `libs/pxl-ase-doc` / `libs/pxl-ase-io` / `libs/pxl-ase` / `libs/pxl-cli/src/cmd/ase/` 实现核对；未编译、未跑测试；任务状态一律「未开始 / NOT_ASSESSED」。  
> 边界：与 v8.0 相同——clean-room、无 UI、不复制 `thirdparty/aseprite` 源码/资源进产品；本版只新增计划文档，不改代码、不改 `games/`。

## 文档入口

| 文档 | 作用 |
|---|---|
| [01-缺口分析与增补总表.md](./01-缺口分析与增补总表.md) | 全部缺口：IO chunk / 渲染 / CLI 矩阵 / 模型编辑 / tilemap / 滤镜脚本 / 处置确认，每项带证据与增补动作 |

## 一句话结论

v8.0 的**分层取舍是完整的**（L0–L5 与旁路模块处置均已成文，核心域 doc/dio/filters/render/CLI 都有分册）；但实现与规格对照存在 **9 类遗漏**：

1. **规格 chunk 级缺口**：User Data(0x2020) / Color Profile(0x2007) / Cel Extra(0x2006) / External Files(0x2008) / Tileset(0x2023) 未读取——读改写 `.aseprite` 会**静默丢数据**（含 tag 颜色、ICC、cel 缩放）；
2. **渲染正确性**：cel z-index 读入但渲染未按规格 NOTE.5 排序；19 种 blend 模型枚举已齐但只实现 NORMAL；组层独立合成（header bit2）未做；
3. **Playback 导出语义未规格化**：`doc/playback.h` 的 PlayAll / repeat 计数 / ping-pong reverse 展开算法无文档；
4. **CLI 矩阵缺行**：`cli-flag-matrix.md` 对照 `app_options.h` 全集缺 14+ 行（`--split-layers/--split-tags/--split-grid/--slice/--ignore-layer/--shrink-to/--format json-array/--tagname-format/--power-of-two-size/--trim-sprite/--border|shape|inner-padding/--sheet-{type,pack,width,height,columns,rows}/--list-layer-hierarchy/--export-tileset` 等），且 08 §6 写「做」的项与矩阵不同步；
5. **文档编辑写回命令未规划**：帧/tag/层 CRUD、`set-duration`、`flatten` 等（对应 `src/app/cmd/` 可无头化命令）；
6. **tilemap/tileset 只有「后续」标签**，无范围冻结；
7. **滤镜 P1/P2 延期项未落成任务卡**（convolution/median/color curve/target 通道/tiled mode）；
8. **模型细节**：`sort_palette` 有证据路径无任务；帧级 palette 动画；pixel ratio 读取后丢弃；header/layer flags 位语义不完整；
9. **脚本对照小误**：`src/README.md` 称 scripting 为 JavaScript engine、`app/script/luacpp.h` 为 Lua 绑定，v8.0 10 只写 Lua（Rhai 选型不受影响）。

## 里程碑

| 代号 | 主题 | 优先级 | 依赖 |
|---|---|---|---|
| GA | IO 保真：User Data / Color Profile 透传 / Cel Extra / header+layer 位语义 / pixel ratio / External Files / 写回丢失警告 | P0–P2 | v8.0 M2 |
| GB | 渲染正确性：z-index 排序（NOTE.5）/ blend 全集分期 / 组层合成 / Playback 导出语义 / `--shrink-to` | P0–P2 | v8.0 M5 |
| GC | CLI 矩阵补全：补行（纯文档，P0）→ split-* / slice / ignore-layer / json-array / sheet 高级旗标实现 | P0(矩阵) / P1(实现) | v8.0 M7 |
| GD | 文档编辑命令（扩展子命令）：帧/tag/层/doc CRUD 写回 `.aseprite`；`palette sort` | P1 | GA |
| GE | 收尾：tilemap/tileset 最小闭环、滤镜 convolve/median/curve、帧级 palette、GIF 选项 | P2 | GA / GB |

建议顺序：**GC 矩阵补行先行**（纯文档，恢复诚实基线）→ GA / GB 的 P0 项 → GD → 其余 P2。

## 验收总则

- 与 v8.0 相同：未跑检查不得写 PASS；矩阵状态与实现同步是 GC 的第一交付物。
- 任何「对齐 Aseprite」的声明必须附：公开规格章节或本地源码路径证据。
- 官方 CLI 无对应 flag 的扩展命令必须在 help 标明「扩展，非官方」。
