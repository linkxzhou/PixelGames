# PixelGames 计划总览（v2：CLI 优先）

> 2026-10-21 15:50（UTC+8）更新：**方向调整，暂不做前端和 WASM UI，也暂不写代码。** 当前只做规划。目标是一组 Rust CLI（主程序名 `pxl`），由 miniagent 通过 `bash` 调用，先做出一个"坦克移动"演示。
> 文档清单：README.md（本文）、cli-design.md（**新增**，CLI 规格）、miniagent-integration.md（**新增**，agent 集成方式与坦克演示调用序列）、asset-pipeline.md（数据模型与变换 op）、architecture.md（crate 布局）、roadmap.md（里程碑）。
> 下文第 1 节之后保留 v1 内容；凡是写到"Web 编辑器、WASM、浏览器"的地方，**都已推迟（deferred）**。

## 0. v2 关键变化
| 项 | v1 | v2 |
|---|---|---|
| 交付形态 | Web 编辑器 + WASM | **Rust CLI `pxl`**（core 库 + cli），WASM 以后复用 core |
| 调用方 | Claude（tool use） | **miniagent**（`/tmp/miniagent` → `/Volumes/my/github/miniagent`），当前默认模型是 **GLM（`glm-5.3-flash-external`）而不是 Claude**，通过 `bash` 调用 CLI，或者以后通过 MCP 调用 |
| 第一个演示 | 一组敌人 | **坦克**：生成坦克、4 方向（rotate90/flip）、履带动画（row_shift + palette cycle）、移动帧、地面 tile、输出 spritesheet PNG + JSON |
| QA | 以 Claude vision 为主 | **以程序 QA（`pxl validate`）为主**；GLM 是否支持看图【未验证】，视觉 QA 作为可选项，可以由人看 `preview` 输出 |

## 1. 现状（代码分析）

`/Volumes/my/github/PixelGames` 目前**只有 `thirdparty/`，还没有自有代码**，也不是 git 仓库（根目录没有 .git）。体积约 145MB。

| 第三方项目 | 是什么 | 许可 | 规模 / 最后提交 | 与我们相关的部分 |
|---|---|---|---|---|
| `thirdparty/Sprite-Generator`（MaartenGr） | 用 Python 移植的 zfedoran/pixel-sprite-generator，即 **Dave Bollinger 蒙版算法** | **仓库没有 LICENSE 文件**（GitHub API 显示 license 为 null，58★） | 620K，2020-04-29 | `SpriteGenerator/Sprite.py`：`_generate_body` → `_mirror_body` → `_generate_edges` → `_apply_colors` |
| `thirdparty/Pixelorama` | Godot 4 写的像素编辑器（GDScript + gdshader） | MIT（10.6k★） | 114M，2026-10-20 | `src/Shaders/Effects/*`（偏移、描边、调色板化、索引映射、旋转算法）、`src/Classes/Tiles.gd`、`LayerTileMap.gd`、`AnimationTag.gd`、`Palette/Palette.gd` |
| `thirdparty/piskel` | JS 写的网页像素编辑器 | Apache-2.0（12.9k★） | 31M，2026-04-18 | `src/js/tools/transform/TransformUtils.js`（翻转、旋转、平移、居中）、`rendering/OnionSkinRenderer.js`、`tools/drawing/DitheringTool.js`、`Lighten.js` |

### 1.1 Bollinger 蒙版算法要点（`Sprite-Generator/SpriteGenerator/Sprite.py`）
- **蒙版取值**：`0` 永远为空；`1` 以 50% 概率成为身体；`2` 以 50% 概率成为身体，否则成为边框（-1），即"必有像素"。
- **镜像**：`_mirror_body` 对每一行做 `row + row[::-1]`，横向对称，宽度翻倍（中轴列会重复一次）。
- **描边**：`_generate_edges` 把身体像素四邻域中值为 0 的格子设成 -1（边框）。
- **上色**：随机色相；每一行（u）以一个"偏向小值"的随机数决定是否换色相（`color_variations`）。亮度为 `sin(u/ulen·π)·(1-noise)+rand·noise`，边框乘以 `edge_brightness`。
- **代码缺陷**：`is_vertical_gradient` 的两个分支完全相同，纵向和横向渐变实际没有区别，原 JS 版有这个区别。另外上色时直接输出 RGB，**不是调色板索引**。所以我们的移植要改成：输出索引，从卡带调色板里的 ramp（明暗梯度）取色。
- **许可**：MaartenGr 版没有许可证，**不复制它的代码**。改为参考 zfedoran 原版（MIT，618★，已核实）和 Bollinger 的公开算法描述，在 Rust 里重新实现。

### 1.2 Pixelorama 可借鉴的点（都是 MIT，可以移植思路或代码）
- `Shaders/Effects/OffsetPixels.gdshaderinc`：按整数像素偏移，可选环绕。对应我们的 `shift` op。
- `Shaders/Effects/OutlineInline.gdshaderinc`：外描边和内描边，提供菱形、圆形、方形三种笔刷和宽度参数。对应 `outline` op。
- `Shaders/Effects/Rotation/`：`cleanEdge.gdshader`（MIT，© torcado 2022）、`OmniScale`、`SmearRotxel`、`NearestNeighbour` 是**像素画友好的旋转算法**。对应 `rotate` op，要限制在小角度和挥砍类效果。
- `Shaders/Effects/Palettize`、`IndexMap.gdshader`、`ColorReplace.gdshader`、`Shaders/SetIndices.gdshader`、`FindPaletteColorIndex.gdshaderinc`：**索引色和调色板映射**。对应 `palette_swap`、`quantize`。
- `FlatToIsometric.gdshaderinc`：平面转等距视角，可用于等距 tile。
- `DropShadow`、`GradientMap`、`Posterize`、`Pixelize`：投影、梯度映射、色阶化。
- `Classes/Tiles.gd`、`Layers/LayerTileMap.gd`、`Cels/CelTileMap.gd`、`TileSetCustom.gd`：tile 模式和 tilemap 图层的数据模型。
- `Classes/AnimationTag.gd`、`Frame.gd`、`Project.gd`：帧、图层、cel 和动画标签的组织方式，**可参考卡带的 sprite 数据模型**。

### 1.3 Piskel 可借鉴的点（Apache-2.0，需要保留 NOTICE 和署名）
- `tools/transform/TransformUtils.js`：`flip`（水平或垂直）、`rotate`（先补成正方形再旋转 90°，再裁回）、`moveFramePixels`（平移，越界的地方填透明）、`center`（按包围盒居中）、`getBoundaries`。**逻辑很简单，适合直接写成 Rust 单元测试的"金标准"**。
- `rendering/OnionSkinRenderer.js`：洋葱皮显示，编辑器调试动画时用。
- `tools/drawing/DitheringTool.js`、`Lighten.js`：棋盘格抖动、提亮和压暗，对应 `dither_fill`、`shade` op。

### 1.4 你在新方向里提到的项目：核实结果
- **GAlbanese09/spritebrew**：✅ 存在，67★，AGPL-3.0，TypeScript（Next.js 16 + PixiJS 8），2026-10-21 仍在提交，线上产品是 spritebrew.com。**它的 AI 生成能力直接调用 Retro Diffusion 的付费 API**（README 原文："Powered by Retro Diffusion's pixel art models via direct API"），没有自研的确定性变换动画。可借鉴的是外围工具：切片、预览、导出到 TexturePacker / Aseprite / GameMaker / RPG Maker / Godot。**AGPL 许可，不能把代码合入我们的项目。**
- **Candle 本地 LLM**：已放弃。LLM 用 Claude API，浏览器里只跑确定性的 Rust/WASM 代码。

## 2. 新方向：可行性评估

**方向**：静态帧用 PCG 生成（Bollinger 蒙版移植到 Rust），动画用确定性的像素矩阵变换生成，Claude 作为"导演"通过结构化工具调用发出变换指令，全部用 Rust+WASM 在浏览器里跑。

### 2.1 优点
- **稳定**：相同的 seed 和 op 序列，结果逐像素一致，可以缓存、做 diff、做回归测试。
- **零推理成本**：浏览器本地计算，毫秒级，可以离线。
- **天然用调色板**：输出就是索引矩阵，和卡带格式一致。
- **Claude 擅长这件事**：选模板、调参数、编排 op 序列都是结构化 JSON，比让它逐像素画稳得多。

### 2.2 局限（必须正视）
1. **Bollinger 蒙版的表现力有限**：适合飞船、机器人、史莱姆、怪物、道具这类**对称、轮廓随机也可接受**的东西；**不适合需要可辨认语义的人形角色**（脸、发型、服装），也不适合立绘。
2. **纯变换动画只能做"小幅度"动作**：
   - 能做好的：idle/呼吸（行平移、上下 1px 起伏）、受击闪白（换调色板）、死亡溶解（有序或随机掩码擦除）、闪烁、抖动、挤压拉伸（squash/stretch）、漂浮、旋转的投射物、简单的翅膀拍动（按部件旋转）。
   - 勉强能做的：行走和跑步（腿部区域错切或交替位移，配合身体上下起伏）。在 8–16px 角色上可以接受，32px 以上会显得像"纸片滑动"。
   - 做不了的：转身或改变朝向（4/8 方向需要不同视角的画面）、挥砍时手臂姿态变化、表情变化、Dead Cells 那种流畅打击动作、攀爬这类拓扑变化大的动作。
3. **方向问题**：左右可以镜像得到；上下（背面、正面）没法靠变换得到，必须另有一套素材。

### 2.3 降级策略（按顺序尝试）
1. **部件化（LPC 风格纸娃娃）**：角色拆成头、躯干、手臂、腿、武器等**部件**，每个部件有锚点（pivot），动画通过**变换部件**完成，例如腿以髋部为轴旋转或偏移。这是"变换动画"能做到较大动作的关键，相当于 2D 骨骼的像素版，**应该从 M2 起作为主路线**。
2. **模板部件库**：为每个视角和动作手工或半自动准备少量部件模板（比如腿的 4 帧行走模板），PCG 只负责配色和细节变化。
3. **LLM 编辑网格**：Claude 对单帧做小范围的"补丁编辑"（`patch_pixels`），用来修正变换产生的瑕疵或补齐关键帧。限制在 ≤16×16 的区域，编辑后必须过 QA。
4. **外部生成（可选、付费、非确定）**：复杂角色或立绘交给扩散流水线或 PixelLab，再量化回卡带调色板（见 pixel-art-gen-opensource.md）。**不放进 MVP。**

### 2.4 关于长期目标
要做到 Stardew / Terraria / Celeste / Undertale / Dead Cells / Octopath 这类游戏，**素材层**需要：autotile、4/8 方向角色、立绘、弹幕、法线贴图和光照。roadmap.md 按游戏列出了需求。实话说：
- Celeste、Terraria、Undertale 风格**最容易**用这套方案逼近（小角色、tile 为主、弹幕是几何图形）。
- Stardew 需要大量可辨认的物品、作物和 NPC 立绘，**PCG 只能覆盖其中一部分**，需要部件化加 LLM 编辑。
- Dead Cells 的流畅动作和 Octopath 的 HD-2D 光照**距离最远**。HD-2D 的法线可以从高度图或索引矩阵自动推导，但质量有限【未验证】。

## 3. 关键决策（建议）

| # | 决策 | 建议 |
|---|---|---|
| D1 | 素材的唯一真实来源 | **调色板索引矩阵 + 生成配方（recipe：模板、seed、op 列表）**。素材可以随时从配方重新生成，配方本身也可以编辑 |
| D2 | 动画表达 | 单张矩阵上的帧级 op 序列，加上部件和锚点上的 op（M2 起） |
| D3 | Agent（Claude 或 miniagent/GLM）的角色 | 只做导演：选模板、写 recipe、调参数、看渲染图做 QA。**不在热路径上**，运行时不依赖 API |
| D4 | 确定性 | 自带 PRNG（SplitMix64 或 PCG32），**禁止使用平台随机数和浮点超越函数**（sin 等改用定点查表），保证 WASM 和 native 结果逐位一致 |
| D5 | 第三方代码 | Bollinger 用 clean-room 方式按 zfedoran 版（MIT）的算法重写；Piskel 的变换只参考思路，用 Rust 重写；Pixelorama 和 cleanEdge 可以移植（保留 MIT 声明）；**不使用 MaartenGr 的代码（无许可）和 spritebrew 的代码（AGPL）** |
| D6 | 编辑器 | **推迟**。先做 CLI；以后的 Web/WASM 前端复用 core |
| D7 | Agent 接口 | **CLI + `--json` 输出 + 稳定的退出码**，配一份 miniagent skill（Markdown）说明用法；M2 再加 `pxl mcp`（stdio MCP server） |

## 4. 未验证项
- 变换动画在 16–32px 角色上的观感：需要用 M1 原型实测。
- Bollinger 在 4 方向人形角色上是否可用：预计不行，需要部件化。
- 从索引矩阵推导法线贴图的质量（HD-2D）。
- cleanEdge 和 OmniScale 用 CPU 整数实现后与 GPU 着色器版本是否一致。
- 我只看了第三方目录的结构和少量关键文件，**没有逐个文件审计**。
