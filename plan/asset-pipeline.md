# 素材流水线设计（asset-pipeline）

> **状态（2026-10-22）：已实现。** 数据模型、op DSL、recipe、parts 纸娃娃均落地并有测试；v2 新增 op（palette_cycle/texture/noop）与宏（tread_cycle/idle_rumble）已可用。注意实际实现与本文早期示意有出入处以代码为准（如 tank shapes 最终用镜像 rect 表达）。

> v2 说明：数据模型和 op DSL 仍然有效，CLI 中对应 `*.pxl.json` 和 `*.ops.json`（见 cli-design.md）。第 4 节的"Claude 工具 API"在 v2 中由 `pxl` 子命令实现（M1 通过 bash 调用，M2 通过 MCP）。v2 新增 op：`palette_cycle`（按调色板 cycles 定义轮换索引）、`texture`（在 region 内调用生成器填充纹理）、`noop`（复制帧），以及宏 `tread_cycle`、`idle_rumble`。

## 1. 数据模型

### 1.1 调色板
```json
{ "palette": { "id": "p16", "colors": ["#00000000","#1a1c2c","#5d275d", "... 共 16 色"],
  "ramps": { "skin": [7,8,9,10], "metal": [1,12,13,14], "slime": [5,6,7] },
  "special": { "transparent": 0, "outline": 1, "flash": 15 } } }
```
- `ramps`（明暗梯度）是上色的基础：PCG 和明暗类 op 只在一个 ramp 内移动索引，不会跑出调色板。
- 索引 0 固定为透明。

### 1.2 索引矩阵（Sprite Frame）
- 内部表示：`width × height` 的 `u8` 数组，按行存储。
- 文本表示（给 Claude 和做 diff 用）：每行一个字符串，`0-9a-z` 对应索引 0–35，`.` 表示透明（等同 0）。

### 1.3 部件与图层（M2 起）
```json
{ "parts": [
  { "id": "torso", "frame": "<matrix>", "pivot": [4,6], "z": 1 },
  { "id": "leg_l", "frame": "<matrix>", "pivot": [1,0], "parent": "torso", "attach": [2,8], "z": 0 },
  { "id": "head",  "frame": "<matrix>", "pivot": [3,6], "parent": "torso", "attach": [4,0], "z": 2 } ] }
```
合成规则：按 z 排序绘制；子部件位置 = 父部件位置 + attach − pivot；所有坐标都是整数。

### 1.4 蒙版模板（Bollinger）
```json
{ "template": { "id": "ship_8x12", "mirror": "x", "cells": [
  "000000", "000011", "000012", "000112", "0011122", "..." ],
  "legend": { "0": "empty", "1": "maybe_body", "2": "body_or_border" },
  "color": { "ramp_pool": ["metal","slime"], "row_hue_change": 0.2, "edge": "outline" } } }
```
- 扩展取值（我们自己加的）：`3` = 必为身体，`4` = 眼睛或高光槽（固定用 `special.flash` 或 ramp 最亮色），`5` = 部件锚点标记。
- 预置模板库：飞船、机器人、史莱姆、虫子、龙（侧视和俯视）、宝石或道具、蘑菇、树、石块。

### 1.5 配方（Recipe）：素材的唯一真实来源
```json
{ "asset": "slime_green", "kind": "sprite",
  "base": { "gen": "bollinger", "template": "slime_8x8", "seed": 42, "palette": "p16", "ramp": "slime" },
  "post": [ {"op":"outline","color":"outline","mode":"outer"}, {"op":"shade","dir":"top_left","amount":1} ],
  "anims": {
    "idle":  { "fps": 6, "loop": true,  "frames": [ [], [{"op":"squash","sy":-1}], [], [{"op":"shift","dy":-1}] ] },
    "hurt":  { "fps": 12, "loop": false, "frames": [ [{"op":"palette_swap","map":"all->flash"}], [] ] },
    "death": { "fps": 10, "loop": false, "frames": "dissolve(seed=7, steps=6, pattern=bayer4)" } } }
```
- **确定性约定**：`seed` 加上 op 序列，再加上引擎版本号（`gen_version`），就完全决定输出像素。修改算法时提升 `gen_version`，旧配方可以选择锁定旧版本。

## 2. 变换 op DSL（JSON Schema 摘要）

全部是**整数、无插值或仅最近邻**的操作，作用于帧矩阵，也可以通过 `target` 指定作用于某个部件、某个区域（rect 或 mask）。

| op | 参数 | 用途 | 参考 |
|---|---|---|---|
| `shift` | dx, dy, wrap?, region? | 平移、上下起伏 | Piskel `moveFramePixels`、Pixelorama `OffsetPixels` |
| `row_shift` | rows:[y0,y1], dx, pattern:"sine4" | 呼吸、飘动（行错位） | 自研 |
| `shear` | axis, k（整数分母）, region, pivot_row | 行走时腿部错切、风吹 | 自研 |
| `squash` | sx, sy（±像素）, anchor:"bottom" | 挤压拉伸 | 自研（最近邻缩放） |
| `flip` | axis | 左右镜像、朝向 | Piskel `flip` |
| `rotate90` | dir | 投射物、tile 变体 | Piskel `rotate` |
| `rotate` | angle（限制 ≤45°）, pivot, algo:"nearest" 或 "cleanedge" | 挥砍、旋转 | Pixelorama `Rotation/*`（cleanEdge 为 MIT） |
| `palette_swap` | map（如 `"3->7"`、`"all->flash"`、`"ramp:skin+1"`） | 受击闪白、变色、阵营色 | Pixelorama `ColorReplace` / `Palettize` |
| `shade` | dir, amount | 光照方向、明暗 | Piskel `Lighten` |
| `outline` | color, mode:"outer" 或 "inner", brush | 描边 | Pixelorama `OutlineInline` |
| `dissolve` | seed, steps, pattern:"bayer4" 或 "random" | 死亡、消失 | Piskel `DitheringTool`（棋盘格思路） |
| `mask_erase` / `mask_fill` | mask, color | 局部擦除或填充 | — |
| `patch_pixels` | x, y, rows[] | **Claude 局部编辑**（≤16×16） | — |
| `part_transform` | part, dx, dy, rot90?, flip? | 部件动画（M2） | — |
| `compose` | parts[] | 部件合成 | — |

**JSON Schema 规则**：每个 op 都是 `{ "op": <enum>, ...参数 }`，用 `oneOf` 区分；参数全部为整数或枚举；数组长度设上限；frames 最多 16 帧、矩阵最大 64×64（M1 为 32×32）。

### 2.1 预设动画宏（Claude 优先使用这些，而不是手写 op）
- `idle_breathe(amp=1, period=4)`
- `bob(amp=1)`
- `walk_side(stride=1..2, bob=1, leg_region=auto)`
- `run_side`
- `hurt_flash(frames=2)`
- `death_dissolve(steps=6)`
- `blink(eye_cells)`
- `spin_projectile(steps=4|8)`
- `pulse_glow(ramp)`

宏在 Rust 端展开成 op 序列，展开结果可以导出，方便审阅。

## 3. 确定性
- PRNG：SplitMix64 生成种子，PCG32 负责流式随机；每个 op 从 `hash(seed, op_index)` 派生自己的子种子。
- 禁止浮点：Bollinger 原版的 `sin` 亮度曲线改成整数查表（只对 ramp 索引做加减偏移）。
- 黄金测试：每个模板和宏都存一份"配方 → 期望矩阵哈希"，CI 中 native 和 wasm32 两端都跑（用 wasmtime 或 node）。

## 4. Claude 工具 API（后端代理调用；工具本身由浏览器或服务器端的 WASM 执行）

| 工具 | 输入 | 输出 |
|---|---|---|
| `list_templates(kind?)` | — | 模板 id、尺寸、适用类别、预览图 URL |
| `generate_sprite(recipe.base)` | 模板、seed、ramp | 矩阵文本 + 4× 放大的 PNG |
| `batch_variants(base, seeds[], n≤32)` | — | 拼接图（contact sheet）PNG + 每格的 seed，让 Claude 挑选 |
| `apply_ops(asset, frame, ops[])` | op 列表 | 新矩阵 + PNG + 警告（如越界、出现孤立像素） |
| `make_anim(asset, name, macro或frames)` | — | 动画 GIF 或条带 PNG + 每帧差异统计 |
| `patch_pixels(asset, frame, x, y, rows)` | — | 新帧 |
| `qa_report(asset)` | — | 程序检查结果（见第 5 节） |
| `save_recipe(asset)` | — | 写入卡带 |

Claude 的标准流程：先用 `batch_variants` 出 16–32 个候选，看拼接图选 1–3 个，再加 post op，再做动画宏，然后跑 `qa_report`，最后看 GIF 做视觉 QA，不合格就调参数或用 `patch_pixels` 修。

## 5. 校验与 QA
**程序检查（硬性）**：
- 所有像素都在调色板内，透明色只用索引 0。
- 包围盒占画布比例在 20%–95% 之间。
- 4 连通分量数 ≤ N（防止碎片）。
- 孤立像素 ≤ k。
- 每帧都有轮廓色闭合。
- 动画相邻帧的像素差占比在 1%–35%：太小说明没在动，太大说明在闪烁跳变。
- 循环动画首尾帧差异小。
- 动画过程中脚底基线稳定（行走类除外，允许上下起伏）。
- 不出现半透明像素。

**视觉检查（Claude vision，软性）**：
- 输入：4×–8× 放大的 contact sheet、GIF 或帧条，配上**一张 16 色色板图**。
- 输出 JSON：`{readable:0-5, silhouette:0-5, motion:0-5, issues:[{frame,x,y,desc}], fix_suggestion}`。
- 阈值：readable ≥ 3 且 motion ≥ 3 才通过；最多重试 2 轮，仍不通过就降级（换模板或改用部件化）或交给人工。

**回归**：配方 + `gen_version` → 哈希快照。
