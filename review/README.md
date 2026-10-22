# Review 对比存档

每轮素材 review 的对比图都存在这里，用来记录 pxl 出图质量的改进过程。v1、v2 每个目录是 10 张单项对比图加一张总览。v3 是 100 张单项对比图，外加按类别的 contact sheet 和一张总览。

| 轮次 | 目录 | 代码基线 | PR | 对比图列 |
|---|---|---|---|---|
| v1 | `v1-pr1/` | `6868c64` | [PR #1](https://github.com/linkxzhou/PixelGames/pull/1) | 参考原图 / pxl 生成 |
| v2 | `v2-pr2/` | `e6d38a7`（用户优化版） | [PR #2](https://github.com/linkxzhou/PixelGames/pull/2) | 参考原图 / v1 / v2 |
| v3 | `v3-pr3/` | `36216ea` | [PR #3](https://github.com/linkxzhou/PixelGames/pull/3) | 参考原图 / v2（仅旧 10 张） / v3 |

## 评分记录（风格 / 辨识度，1–10）

v1、v2 只覆盖原来的 10 张。v3 对这 10 张给的是整张素材一个分数（心和子弹合在 `soul-bullet` 里）。

| 素材 | v1 风格 | v2 风格 | v3 风格 | v1 辨识度 | v2 辨识度 | v3 辨识度 |
|---|---|---|---|---|---|---|
| celeste-run | 6 | 7 | 7 | 7 | 8 | 8 |
| celeste-spikes | 4 | 6 | 6 | 7 | 8 | 8 |
| terra-tiles | 5 | 7 | 6 | 6 | 8 | 7 |
| crop-stages | 5 | 7 | 6 | 7 | 8 | 7 |
| orchard-tree | 4 | 6 | 6 | 7 | 8 | 8 |
| item-chest | 5 | 6 | 6 | 8 | 8 | 8 |
| soul-bullet（心 / 子弹） | 7 / 4 | 8 / 6 | 7 | 8 / 5 | 8 / 7 | 8 |
| ui-dialog | 5 | 6 | 5 | 7 | 8 | 7 |
| meadow-slime | 5 | 7 | 7 | 7 | 8 | 8 |
| shaded-crate | 3 | 5 | 6 | 8 | 8 | 8 |

v3 全部 100 张：风格平均 **5.88**，辨识度平均 **7.01**。

### 最高的 10 张

| 素材 | 风格 | 辨识度 |
|---|---|---|
| celeste-run | 7 | 8 |
| heart-meter | 7 | 8 |
| meadow-slime | 7 | 8 |
| soul-bullet | 7 | 8 |
| celeste-spikes | 6 | 8 |
| dash-crystal | 6 | 8 |
| floating-eye | 6 | 8 |
| item-chest | 6 | 8 |
| orchard-tree | 6 | 8 |
| shaded-crate | 6 | 8 |

同档还有 mushroom、pickaxe、pumpkin、stardew-farmer、terra-grass、torch、wheat（都是 6 / 8）。

### 最低的 10 张

| 素材 | 风格 | 辨识度 | 原因 |
|---|---|---|---|
| deadcells-roll | 4 | 4 | 滚翻做成了一团椭圆 |
| nameplate | 4 | 5 | 只剩一条名字色块 |
| bed | 5 | 6 | 床架和枕头挤在一起 |
| chair | 5 | 6 | 靠背和腿分不太开 |
| earth-worm | 5 | 6 | 一条细虫，剪影弱 |
| item-slot | 5 | 6 | 两层方框 |
| menu-button | 5 | 6 | 按钮上只有一根色条 |
| shade-wisp | 5 | 6 | 一团火，没有角色 |
| shortbow | 5 | 6 | 弓和弦太细 |
| skeleton | 5 | 6 | 肋骨和腿是散点，骨架不完整 |

### 分类平均

| 类别 | 张数 | 风格 | 辨识度 |
|---|---|---|---|
| characters | 16 | 5.88 | 6.88 |
| tiles | 16 | 5.94 | 7.00 |
| props | 12 | 5.83 | 7.00 |
| items | 14 | 5.86 | 7.00 |
| crops | 12 | 6.00 | 7.33 |
| enemies | 12 | 5.83 | 6.92 |
| ui | 10 | 5.60 | 6.70 |
| effects | 8 | 6.13 | 7.38 |

### 100 张明细

| # | 素材 | 游戏 | 类别 | v3 风格 | v3 辨识度 |
|---|---|---|---|---|---|
| 01 | celeste-run | Celeste | characters | 7 | 8 |
| 02 | celeste-dash | Celeste | characters | 6 | 7 |
| 03 | celeste-climb | Celeste | characters | 6 | 7 |
| 04 | undertale-frisk | Undertale | characters | 6 | 7 |
| 05 | undertale-frisk-side | Undertale | characters | 6 | 7 |
| 06 | stardew-farmer | Stardew Valley | characters | 6 | 8 |
| 07 | stardew-water | Stardew Valley | characters | 6 | 7 |
| 08 | terra-miner | Terraria | characters | 6 | 7 |
| 09 | deadcells-idle | Dead Cells | characters | 6 | 7 |
| 10 | deadcells-roll | Dead Cells | characters | 4 | 4 |
| 11 | octopath-hero | Octopath Traveler | characters | 6 | 7 |
| 12 | octopath-dancer | Octopath Traveler | characters | 6 | 7 |
| 13 | stardew-chicken | Stardew Valley | characters | 6 | 7 |
| 14 | terra-guide | Terraria | characters | 6 | 7 |
| 15 | celeste-hair | Celeste | characters | 6 | 7 |
| 16 | undertale-flowey | Undertale | characters | 5 | 6 |
| 17 | terra-tiles | Terraria | tiles | 6 | 7 |
| 18 | terra-dirt | Terraria | tiles | 6 | 7 |
| 19 | terra-grass | Terraria | tiles | 6 | 8 |
| 20 | terra-stone | Terraria | tiles | 6 | 7 |
| 21 | terra-ore | Terraria | tiles | 6 | 7 |
| 22 | terra-sand | Terraria | tiles | 6 | 7 |
| 23 | terra-snow | Terraria | tiles | 6 | 7 |
| 24 | terra-wood | Terraria | tiles | 6 | 7 |
| 25 | stardew-path | Stardew Valley | tiles | 5 | 6 |
| 26 | stardew-hoedirt | Stardew Valley | tiles | 6 | 7 |
| 27 | celeste-snow | Celeste | tiles | 6 | 7 |
| 28 | undertale-ruins | Undertale | tiles | 6 | 7 |
| 29 | deadcells-brick | Dead Cells | tiles | 6 | 7 |
| 30 | octopath-cobble | Octopath Traveler | tiles | 6 | 7 |
| 31 | terra-ice | Terraria | tiles | 6 | 7 |
| 32 | lava-crust | Terraria | tiles | 6 | 7 |
| 33 | shaded-crate | Octopath Traveler | props | 6 | 8 |
| 34 | barrel | Stardew Valley | props | 6 | 7 |
| 35 | signpost | Stardew Valley | props | 6 | 7 |
| 36 | torch | Terraria | props | 6 | 8 |
| 37 | lantern | Octopath Traveler | props | 6 | 7 |
| 38 | clay-pot | Stardew Valley | props | 6 | 7 |
| 39 | fence | Stardew Valley | props | 6 | 7 |
| 40 | wooden-door | Terraria | props | 6 | 7 |
| 41 | bed | Stardew Valley | props | 5 | 6 |
| 42 | table | Stardew Valley | props | 6 | 7 |
| 43 | chair | Stardew Valley | props | 5 | 6 |
| 44 | well | Stardew Valley | props | 6 | 7 |
| 45 | item-chest | Terraria | items | 6 | 8 |
| 46 | copper-sword | Terraria | items | 6 | 7 |
| 47 | pickaxe | Terraria | items | 6 | 8 |
| 48 | hatchet | Terraria | items | 6 | 7 |
| 49 | watering-can | Stardew Valley | items | 5 | 6 |
| 50 | ruby | Terraria | items | 6 | 7 |
| 51 | gold-coin | Stardew Valley | items | 6 | 7 |
| 52 | iron-key | Terraria | items | 6 | 7 |
| 53 | health-potion | Stardew Valley | items | 6 | 7 |
| 54 | berry | Stardew Valley | items | 6 | 7 |
| 55 | fish | Stardew Valley | items | 6 | 7 |
| 56 | spellbook | Octopath Traveler | items | 6 | 7 |
| 57 | shortbow | Terraria | items | 5 | 6 |
| 58 | wooden-shield | Terraria | items | 6 | 7 |
| 59 | crop-stages | Stardew Valley | crops | 6 | 7 |
| 60 | orchard-tree | Stardew Valley | crops | 6 | 8 |
| 61 | wheat | Stardew Valley | crops | 6 | 8 |
| 62 | parsnip | Stardew Valley | crops | 6 | 7 |
| 63 | cauliflower | Stardew Valley | crops | 6 | 7 |
| 64 | sunflower | Stardew Valley | crops | 6 | 7 |
| 65 | sapling | Stardew Valley | crops | 6 | 7 |
| 66 | mushroom | Terraria | crops | 6 | 8 |
| 67 | cactus | Terraria | crops | 6 | 7 |
| 68 | pumpkin | Stardew Valley | crops | 6 | 8 |
| 69 | tulip | Stardew Valley | crops | 6 | 7 |
| 70 | berry-bush | Stardew Valley | crops | 6 | 7 |
| 71 | meadow-slime | Stardew Valley | enemies | 7 | 8 |
| 72 | cave-bat | Terraria | enemies | 6 | 7 |
| 73 | skeleton | Terraria | enemies | 5 | 6 |
| 74 | floating-eye | Terraria | enemies | 6 | 8 |
| 75 | zombie | Terraria | enemies | 6 | 7 |
| 76 | bee | Stardew Valley | enemies | 6 | 7 |
| 77 | hermit-crab | Stardew Valley | enemies | 6 | 7 |
| 78 | ghost | Terraria | enemies | 6 | 7 |
| 79 | earth-worm | Stardew Valley | enemies | 5 | 6 |
| 80 | spike-ball | Dead Cells | enemies | 6 | 7 |
| 81 | shroom-enemy | Terraria | enemies | 6 | 7 |
| 82 | shade-wisp | Dead Cells | enemies | 5 | 6 |
| 83 | ui-dialog | Undertale | ui | 5 | 7 |
| 84 | heart-meter | Undertale | ui | 7 | 8 |
| 85 | menu-button | Terraria | ui | 5 | 6 |
| 86 | item-slot | Stardew Valley | ui | 5 | 6 |
| 87 | cursor-hand | Stardew Valley | ui | 6 | 7 |
| 88 | coin-counter | Stardew Valley | ui | 6 | 7 |
| 89 | nameplate | Octopath Traveler | ui | 4 | 5 |
| 90 | menu-arrow | Undertale | ui | 6 | 7 |
| 91 | stamina-bar | Dead Cells | ui | 6 | 7 |
| 92 | map-marker | Octopath Traveler | ui | 6 | 7 |
| 93 | soul-bullet | Undertale | effects | 7 | 8 |
| 94 | celeste-spikes | Celeste | effects | 6 | 8 |
| 95 | hit-spark | Dead Cells | effects | 6 | 7 |
| 96 | puff-explosion | Terraria | effects | 6 | 7 |
| 97 | bone-bullet | Undertale | effects | 6 | 7 |
| 98 | ring-bullet | Undertale | effects | 6 | 7 |
| 99 | slash-arc | Dead Cells | effects | 6 | 7 |
| 100 | dash-crystal | Celeste | effects | 6 | 8 |

## v2 改进点
- 侧视跑步角色，头发单独色相
- 尖刺改为单峰，用 `rotate90` 生成四个朝向
- 泥土、草、矿石改为手写像素 stamp
- 作物去掉内置土壤，叶片带色阶
- 树冠分瓣，风只摆动叶片
- 宝箱盖抬起时锁扣保持不动
- 心形脉冲用 `ramp:heart+1`，子弹改为 V 形
- 对话框光标用 `blink`
- 史莱姆改为灰色，跳动加眨眼
- 木箱四个面使用独立色阶

## v2 仍存在的问题
- 缺 autotile、9-slice 和字体、头发物理、弹幕图案
- `death_dissolve` 最后一帧为空，过不了 bbox 检查
- Terra 地块只能做成 8×9（铺满的 8×8 过不了 20–95% bbox 规则）
- 8×8 跑步动画因帧差余量只有 22 个像素，没有加明暗
- 木箱仍是 16×16，参考是约 184 色的 48×48 烘焙图

## v3 相对 v2

配方在 `examples/reference-v3/`。每张有自然语言说明（`descriptions.md` 和各自的 `description.json`）、调色板、shapes、recipe，以及裁下来的参考图。像素是 shapes 写的，没有描参考图。100 张 `pxl build` 都过了 `validate --rules default`。引擎侧只改了一处：`harness` 在 `exec` 之前用 `process_group(0)` 建进程组。原先在 `exec` 之后 `setpgid` 会得到 EACCES，超时杀不掉 shell 的孙进程，`timeout_kills_process_group` 会一直等到子进程自己退出。

和 v2 的 10 张比：箱子三个面分开上色，风格 5 → 6。跑步、尖刺、树、箱子、史莱姆、心大体持平。地形和对话框略降：这轮用同一套左上光和棋盘抖动，没有再逐像素抠 v2 那两张手写图章，对话框里的字被明暗混进了纸色。

这轮新做到的：

- 100 张，六个目标游戏，八类都在：角色、地形、道具、物品、作物、敌人、UI、特效。原来的 10 个类别都还在。
- 调色板暗部偏冷、亮部偏暖。明暗写进图章：左上受光沿、右下暗沿，大块面上有棋盘抖动。
- 地形是团块而不是细噪声。草和雪是上下两种材料的层边界，矿脉留在内圈，对边落在同一条 ramp 上。
- 尖刺仍是单峰，四个朝向用 `rotate90`。心的脉冲仍是 `ramp:heart+1`，人字弹只改底部几格，不转整张画布。
- 角色、史莱姆、作物、大部分道具用 `bob(amp=1)`。8×8 跑步仍是四帧手写。作物是四段不循环生长。

参考图的来源记在每张 `description.json` 的 `source_url`：

- Celeste：`NoelFB/Celeste` 的 PICO-8 `atlas.png`（和 v1/v2 同一张）。MIT 只覆盖代码。
- Terraria 风格的地块和少量物品：`Zorz42/Terralistic`（GPL-3.0），取条带的第一格。
- 其余：Kenney CC0 的 Pixel Platformer 和 Roguelike/RPG pack（OpenGameArt）。这是同类像素参考，不是那几款商业游戏的原画。

## v3 仍存在的问题

- 没有 autotile、九宫格、字体、头发物理。任意角度旋转仍然拒绝。
- 人形是同一套骨架换色，躯干有几行相同的色带。`deadcells-roll` 不是滚翻，`undertale-flowey` 更接近蘑菇。
- 动画大多是整张 1px 起伏。走路如果只挪腿，容易把轮廓拆开或者帧差超过 35%。`blink` 的眼睛矩形在放大之后对不齐，这轮没有用。
- 小图标为了达到 20% 包围盒，有的被最近邻放大，笔画变粗。
- UI 的字是色块，不是字形。对话框的字被 ramp 明暗打散了。
- 参考图和目标游戏不是同一张原画时，风格分有上限：放进那个游戏的图集里仍会被认出来。

总览和分类 contact sheet：

- `review/v3-pr3/contact-sheet.png`
- `review/v3-pr3/contact-characters.png`
- `review/v3-pr3/contact-tiles.png`
- `review/v3-pr3/contact-props.png`
- `review/v3-pr3/contact-items.png`
- `review/v3-pr3/contact-crops.png`
- `review/v3-pr3/contact-enemies.png`
- `review/v3-pr3/contact-ui.png`
- `review/v3-pr3/contact-effects.png`

> 对比图里的参考像素来自上面列出的公开仓库和 CC0 素材包，只作 review 存档。配方本身是原创图章。
