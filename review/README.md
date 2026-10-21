# Review 对比存档

每轮素材 review 的对比图都存在这里，用来记录 pxl 出图质量的改进过程。每轮一个子目录，里面是 10 张单项对比图加一张总览 `contact-sheet.png`。

| 轮次 | 目录 | 代码基线 | PR | 对比图列 |
|---|---|---|---|---|
| v1 | `v1-pr1/` | `6868c64` | [PR #1](https://github.com/linkxzhou/PixelGames/pull/1) | 参考原图 / pxl 生成 |
| v2 | `v2-pr2/` | `e6d38a7`（用户优化版） | [PR #2](https://github.com/linkxzhou/PixelGames/pull/2) | 参考原图 / v1 / v2 |

## 评分记录（风格 / 辨识度，1–10）

| 素材 | v1 风格 | v2 风格 | v1 辨识度 | v2 辨识度 |
|---|---|---|---|---|
| celeste-run | 6 | 7 | 7 | 8 |
| celeste-spikes | 4 | 6 | 7 | 8 |
| terra-tiles | 5 | 7 | 6 | 8 |
| crop-stages | 5 | 7 | 7 | 8 |
| orchard-tree | 4 | 6 | 7 | 8 |
| item-chest | 5 | 6 | 8 | 8 |
| soul-bullet（心 / 子弹） | 7 / 4 | 8 / 6 | 8 / 5 | 8 / 7 |
| ui-dialog | 5 | 6 | 7 | 8 |
| meadow-slime | 5 | 7 | 7 | 8 |
| shaded-crate | 3 | 5 | 8 | 8 |

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

> 对比图中包含原游戏的参考美术，仅作内部 review 存档用。
