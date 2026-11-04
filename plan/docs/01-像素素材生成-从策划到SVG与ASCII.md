# 像素素材生成：从策划到 SVG / ASCII

> 像素游戏生成管线系列第 1 篇，只讲图。第 2 篇：ASCII → 合成音乐。

## 1. 目的

用语言模型生成整款像素游戏的素材：像素图、动画、音乐。不接图片生成模型，不接音乐生成模型，也不用任何扩散模型 API。

像素素材恰好是语言模型的舒适区：画布小、调色板有限、造型高度概括。模型不需要"画"得多好，只需要把造型、配色、动画节奏描述准。但这有个前提——模型的描述不能直接等于结果，中间要隔一层确定性的东西。

## 2. 当前阶段：如何稳定生成像素素材

让模型直接吐 PNG 是行不通的：每次结果都不同，改一个字全图重画，没法验收，没法回归，也没法协作。所以第一步是把"画图"这个动作从模型手里拿走，变成一次编译。

参照物是 Aseprite。我按它的公开规格和源码行为，用 Rust 干净重写了一个 headless 版本——不拷贝它的代码（仓库里 `thirdparty/aseprite` 只作行为对照），不依赖 Aseprite 本体，无 GUI，最后收敛成一个独立二进制 `pxl-asset-gen`（仓库内叫 `step-300`，来自 `libs/pxl-cli`）。

目前实现的功能，按块说：

- 图像与文档模型：调色板索引像素（0 号固定透明）；Sprite / Layer / Cel / Frame / Tag / Slice；`.aseprite` 读写；PNG / GIF / spritesheet（PNG + JSON meta）导出。
- 中间表示编译：ASCII 网格、SVG、shapes、sprite 规格、recipe，全部编译成索引像素。
- ase 家族：convert / sheet / filter / op / import / script（Rhai 脚本）/ frame·tag·layer·cel 编辑 / export-tileset，对齐 Aseprite CLI 的一个子集语义。
- 质检与工具：validate（程序化 QA，exit 5 即不合格）/ inspect / render / pixelize（PNG 转索引帧）/ enhance（2–4x 放大）/ tile 生成 / schema。
- 工程化：recipe 批量 build / plan；scene / prototype 编译；stdio MCP server；audio（离线渲染 WAV，下篇讲）。

"稳定"在这里有明确定义：锁定调色板 + 中间表示 + `seed` + 工具版本（`--pxl-version-lock`），同样的输入重跑两遍，像素级一致。整个二进制里没有语言模型、不读网络，它只做一件事：把文本确定性地编译成像素。

<!-- 图：同 seed 跑两遍，像素一致 -->
![确定性重建](./images/01-deterministic-rebuild.png)

## 3. 为什么是 SVG / ASCII：流程与架构

模型对 SVG 和 ASCII 的表达力，远好于对像素坐标的掌控力。让它写"第 37 行第 12 列放一个深灰"，它经常数错；让它写一段 SVG path、或者画一个 ASCII 网格，准确率高一个量级。所以我不让模型碰像素：模型写中间表示，编译交给工具。

流程一条线：

```
策划案 brief → SVG / ASCII / shapes / recipe（中间表示）→ step-300 编译 → PNG / GIF / sheet
```

中间表示各有分工：

- ASCII：一格一字符，改起来最直观，单精灵迭代的主力；
- SVG：先定外形和比例，再栅格化到目标尺寸，适合曲线造型；
- shapes / recipe：管几何变换、调色板映射、动画帧和批量构建，`build` 一步跑完生成、动画展开、打包、预览、QA。

架构上三层：

| 层 | 位置 | 干什么 |
|---|---|---|
| skill | `scripts/skills/pxl-asset-gen` | 工艺文档 + 预编译二进制 + 出包脚本，开箱即用 |
| CLI | `libs/pxl-cli` → `step-300` | 所有像素操作，确定性编译 |
| 资产包 | `examples/<slug>/` | brief、中间表示、产物，全部落盘可回溯 |

skill 里没有源码、没有编译步骤，就是一个二进制加一批 markdown 工艺文档（调色板、剪影、光影、动画规范）。模型照着文档干活，用二进制落地，产出就是仓库里的 examples 包。

<!-- 图：mid-rep 编译成 PNG -->
![mid-rep 编译](./images/01-midrep-compile.png)

## 4. 不多说，直接上效果

### 4.1 几个经典游戏

输入只有一份策划案（brief），出整个资产包。仓库里已经跑了 50 多个游戏包，挑几个：

**battle_city（坦克大战）** — 坦克、砖墙、爆炸。
<!-- 图：坦克 idle -->
![battle_city 坦克](./images/01-battle-city-tank.png)
<!-- 可用 examples/battle_city/previews/player_tank_idle_N_x8.png、brick_idle_x8.png、explosion_burst.gif -->

**stardew（星露谷）** — library 型整包，几百个资产，交付带 `index.html` 的可浏览素材库。
<!-- 图：library 首页 / 角色 -->
![stardew library](./images/01-stardew-library.png)
<!-- 可用 examples/stardew/index.html、previews/characters_01.png -->

**dead_cells** — 敌人、地形。
<!-- 图：enemies / biomes -->
![dead_cells](./images/01-dead-cells.png)
<!-- 可用 examples/dead_cells/previews/enemies_01.png、biomes_01.png -->

**metal_slug** — 载具、爆炸。
<!-- 图：载具 / 爆炸 -->
![metal_slug](./images/01-metal-slug.png)
<!-- 可用 examples/metal_slug/previews/metal_slug_idle_E_x8.png、explosion_burst.gif -->

**asteroids** — 飞船、岩石、UFO。
<!-- 图：飞船 / 岩石 UFO -->
![asteroids](./images/01-asteroids.png)
<!-- 可用 examples/asteroids/previews/ship_idle_x8.png、rock_big_idle_x8.png、ufo_idle_x8.png -->

### 4.2 skill 怎么用

skill 在 `scripts/skills/pxl-asset-gen`，自带各平台预编译二进制，挂给任何一个 coding agent 就能用。两种用法。

给 agent 用，一句话：

> 用 pxl-asset-gen，按 stardew 的样子给我出一套 XX 游戏的素材包。

agent 会按 SKILL.md 的流程自己走完：brief → 资产清单 → 调色板 → 铺块 → 上色 → 动画帧 → 全量构建 → 程序化校验 → 截图评审 → 交付 `index.html`。每一步有 "Done when" 卡验收，不合格就回退到对应步骤修。

想自己动手，直接用二进制：

```bash
SKILL=<repo>/scripts/skills/pxl-asset-gen
PXL=$SKILL/bin/pxl-asset-gen-darwin-arm64   # Linux: pxl-asset-gen-linux

$PXL ascii render hero.ascii.json --palette p16 --scale 8 -o preview.png   # ASCII 快速预览
$PXL build ship.recipe.json -o out/                                        # recipe 出全产物
$PXL validate out/ship.pxl.json --json                                     # 程序化 QA
$PXL ase convert hero.aseprite -o hero.png                                 # Aseprite 导入导出
```

skill 内置三条工作流：A 整包（默认，终点是 `index.html` 素材库）、B 单图（brief → 画格 → 上色 → build → validate → render → 导出）、C 改图（对现有 PNG / `.pxl.json` / `.aseprite` 做小步修改）。

<!-- 图：CLI 会话 -->
![CLI](./images/01-skill-cli.png)

### 4.3 输出长什么样

recipe 型（asteroids / battle_city / metal_slug 这一票街机包）：

```
examples/<slug>/
  brief.md            # 策划案，唯一的输入
  palette.json        # 调色板
  <id>.recipe.json / <id>.shapes.json（或 ASCII）   # 中间表示
  <id>.pxl.json       # 编译产物（文档）
  <id>.sheet.png / previews/   # sheet 和预览图
```

library 型（stardew / dead_cells 这类几百资产的包）：

```
examples/<slug>/
  palette.json  sources/  tools/project.py    # 真相源
  atlases/  assets/  previews/                # 产物
  manifest.json  validation.json              # 清单和校验记录
  index.html                                  # 可浏览素材库
```

brief、palette、中间表示是真相源，previews 只用来验收。包内自带 `validation.json` 和确定性构建脚本，删掉全部产物重跑一遍，所有图原样回来——这也是第 2 节说的"稳定"在包层面的落地。

---

下一篇：ASCII → 音乐。文本序列离线渲染成 WAV，同样不借音乐生成模型。
