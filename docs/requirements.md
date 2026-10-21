# 原始诉求（按时间顺序，原文）

日期：2026-10-21（Asia/Shanghai）

## 1. 项目起点（12:20）

> 我现在想做一款像素的游戏引擎，最好是 RUST+WASM，可以直接在 web 上加载，而且像素游戏引擎的图片可能不需要图片生成，所以整体只需要 agent 生成即可，所以做这个垂直领域，现在分析业界或者开源的像素游戏引擎，分析方案的合理性，需要用哪些开源组件比较合适，先调研，其中 LLM 直接使用 Claude 的模型实现，其中我已经收集了如下参考资料：（见 `reference-01-initial-proposal.md`）
>
> 基于如上的一些思考，现在调研开发一款像素的游戏引擎 + AI 驱动的管线，给出调研资料

## 2. 继续调研（13:27）

> 继续调研，星露谷、泰拉瑞亚、蔚蓝、传说之下、死亡细胞、八方旅人等技术实现博客，分析技术实现方案，或者现在有没有 AI 做像素游戏成熟的产品或者开源的方案

## 3. 开源幻想机（13:35）

> PICO-8 有没有开源的版本

## 4. 美术生成替代（13:37）

> PixelLab 或者 Retro Diffusion 开源替代方案

## 5. 复刻项目（14:09）

> 先 github 查找复刻 星露谷、泰拉瑞亚、蔚蓝、传说之下、死亡细胞、八方旅人等像素游戏的列表

## 6. 素材生成方案（14:16）

> 像素素材AI 生成还有哪些方案

## 7. 方向调整：PCG + 变换算子（15:35）

> 现在整体改为如下方案：（见 `reference-02-pcg-transform-proposal.md`）
>
> 其中代码在 /Volumes/my/github/PixelGames ，相关的开源代码在 /Volumes/my/github/PixelGames/thirdparty，先分析代码，结合上述方案，和调研，最后输出一个 plan 放到 /Volumes/my/github/PixelGames/plan，目标是实现一个像素游戏引擎，能实现 `星露谷、泰拉瑞亚、蔚蓝、传说之下、死亡细胞、八方旅人` 这些游戏，现在这一步是为了稳定的生素材

## 8. 先做 CLI（15:47）

> 先不要实现前端版本，先将Sprite-Generator，Pixelorama，Piskel 等像素生成，像素动作等做成 cli 工具，然后我用 /tmp/miniagent 来调用 cli 实现一个简单坦克移动的功能，先更新 plan ，暂时不实现代码

## 9. 建仓库并云端实现（16:02）

> 先给 PixelGames 创建一个 git 项目，私有的，然后再云端执行写代码，完成后我来 review，再到本地用 miniagent 来调用 pxl 生成 坦克

## 10. 更复杂的角色（16:57）

> 可以制作一个更加复杂的动态图，比如 星露谷 中的人物

## 11. 拉取参考仓库到 thirdparty（17:49）

> 将如下git 拉到本地 thirdparty 中：NoelFB/Celeste、velzie/rustic-mountain、Zorz42/Terralistic、JevonLee/Stardew、TML233/UndertaleEngine（另含 lvk/Unitale）、YouhuaUncle/Pixel-art-3D-to-2D-rendering-in-Godot、thenumbernine/numo9

## 12. 推送 GitHub（17:55）

> 现在已经加了.gitignore，如果没问题可以推送到 git@github.com:linkxzhou/PixelGames.git

## 13. 用 pxl 生成参考美术（18:02）

> 现在分析 Celeste、rustic-mountain、Terralistic、Stardew、UndertaleEngine、Unitale、Pixel-art-3D-to-2D-rendering-in-Godot、numo9 各个美术资源，是否可以用 pxl 来生成，生成其中10 个美术资源，我看看效果

## 14. 存档（18:05）

> 将整个像素游戏我给出一些参考原始信息和原始诉求单独落到 docs 目录中存档
