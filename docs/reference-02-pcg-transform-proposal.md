# 参考资料 02：Sprite-Generator 掩码算法 + 像素编辑器变换算子生成动作（原文存档）

> 由 linkxzhou 于 2026-10-21 15:35 提供，作为方向调整的依据，原文保留。核实结果：Sprite-Generator 仓库无许可证，需按 MIT 上游 zfedoran/pixel-sprite-generator 重写；SpriteBrew 真实存在（AGPL-3.0），但其 AI 生成调用 Retro Diffusion 付费 API，并无确定性变换动画；Candle 本地模型已从方案中移除。详见 `plan/`。

你的直觉非常敏锐，这个思路直接切中了轻量级、确定性像素游戏引擎的核心破局点！

MaartenGr/Sprite-Generator (https://github.com/MaartenGr/Sprite-Generator) 是一个基于 Dave Bollinger 掩码算法（Mask Algorithm） 的经典程序化生成（PCG）方案。它通过在一个二维矩阵上进行随机填色、随机底噪和轴对称，能够秒级生成数万个完美的像素飞船或小怪物，但正如你所说，它生成的只是“静态单帧死图”，无法产生走、跑、跳、攻击等连续动作。 [1 (https://github.com/MaartenGr/Sprite-Generator)]

如果你提到的 “got pixel 编辑器” 指的是 Godot 生态中大名鼎鼎、完全开源的像素全能工具 Pixelorama (https://godotengine.org/showcase/pixelorama/)（由 Godot 引擎编写，在开源社区常被简写或误打为 got pixel），或者是类似 Piskel (https://www.piskelapp.com/) 这种纯浏览器端的开源像素编辑器，那么将 MaartenGr 的“单帧矩阵”与“像素编辑器的核心算法”进行融合，不仅完全可行，而且是目前实现原生 WASM 像素动作生成代价最小、效果最可控的硬核方案。 [1 (https://www.piskelapp.com/), 2 (https://godotengine.org/showcase/pixelorama/)]

💡 为什么这个结合能生成动作？（底层逻辑）

传统的 3D 游戏靠“骨骼”动，而 2D 像素游戏靠的是“帧动画（Frame-by-Frame Animation）”。像素动作的本质，就是对一张静态图的像素行和列进行有规律的位移、拉伸、旋转或局部重绘。 [1 (https://www.seeles.ai/resources/blogs/how-to-animate-sprite-sheets-ai-game-development)]

Pixelorama 等现代像素编辑器之所以强大，是因为它们内置了大量的网格数学变换算子（Image Manipulation / Sprite FX）。通过引入这些算子，可以让 Agent 操纵算法，对 MaartenGr 生成的初始帧进行自动“揉捏”，从而变成一整张 Sprite Sheet（精灵图序列）。 [1 (https://godotengine.org/showcase/pixelorama/), 2 (https://pixler.dev/pixel-art-animation-generator), 3 (https://ludo.ai/features/sprite-generator)]

🛠️ 具体的融合实现方案：利用 Rust 复刻并升级

在你的 Rust + WASM 引擎构想中，你不需要把整个 Pixelorama 几万行前端 UI 搬进来，而是可以用 Rust 提取它们核心的“像素矩阵变换算法”。具体动作生成流程如下：

动作 1：呼吸/待机动作（Idle / Breathe Animation）

- 算法逻辑（来自 Pixelorama 的层位移）： 保持角色的头部、主体像素完全不动（Bit-identical），只在垂直方向上将中间的像素行向上或向下整体平移 1 像素，同时让腹部或两侧的像素向外拓宽 1 像素。 [1 (https://medium.com/@creativeaininja/sprite-gen-turns-ai-images-into-sprites-your-game-can-actually-use-f32b7091a0e4)]
- Rust 实现： 直接用 Rust 的 Vec<u8> 数组切片对矩阵行进行 row_shift。通过控制循环，3 帧就能做出来一个极其复古、完美的 2D 像素呼吸循环。 [1 (https://medium.com/@creativeaininja/sprite-gen-turns-ai-images-into-sprites-your-game-can-actually-use-f32b7091a0e4)]

动作 2：行走/奔跑循环（Walk / Run Cycle）

- 算法逻辑（来自像素编辑器的剪切变换 Shear FX）： 行走需要身体前倾。你可以对 MaartenGr 生成的对称网格引入“仿射变换（Affine Transformation）中的剪切（Shear）”。
- 具体重绘：
  - Frame 1（左脚迈出）： 将下半身左侧的像素列向下延伸 1-2 像素，右侧缩短。
  - Frame 2（交替）： 像素归位。
  - Frame 3（右脚迈出）： 将下半身右侧的像素列向下延伸。
- Agent 的角色： LLM Agent（如 Candle 本地跑的轻量模型）不需要亲自动手画图，它只需要作为“总指挥”，通过 Rhai 脚本向引擎发送指令：“对当前 Sprite 执行底层剪切算子，X轴倾斜 0.1，左下角像素向下偏移 1 像素”。

动作 3：受击与死亡（Hurt / Death）

- 算法逻辑（调色板交换 Palette Swapping 与颗粒化）：
  - 受击： 像素编辑器中常用的“全局变色”。Rust 接收静态矩阵，直接将其中的 RGB 颜色表瞬间替换为强烈的纯白或纯红（闪烁 2 帧）。
  - 死亡： 引入像素级沙化算法（Particlerization）。逐帧随机把矩阵里 20%、50%、80% 的像素点的 RGB 值的 Alpha 通道设为 0，并让其余像素向上飘散。 [1 (https://godotengine.org/showcase/pixelorama/)]

🎨 进阶技术替代：如果你不想手写变换算法

如果你觉得完全靠数字几何去“硬搓”动作手感不够自然，在 2026 年的开源节点上，有两个专门结合了 AI + 像素编辑器 的方案可以直接为你所用：

1. SpriteBrew (https://github.com/GAlbanese09/spritebrew)（全网页端开源）
   - 这是近期的明星开源项目，它把传统的像素编辑器（Pixel Editor）和轻量级 AI 动作管线整合在了一个 Next.js/Web 环境里。它支持输入单张静态像素（比如 MaartenGr 生成的图），然后直接输出标准的 Godot/Unity 动作序列图。你可以直接去扒它的纯前端动作生成管线逻辑。 [1 (https://github.com/GAlbanese09/spritebrew)]
2. PixelLab AI Engine (https://www.pixellab.ai/) (集成 Aseprite/Pixelorama 的 Inpainting)
   - 利用了局部重绘（Inpainting）技术。MaartenGr 生成第一帧，Agent 自动擦除局部（比如擦掉手部），然后命令轻量级 Diffusion 模型在像素编辑器的网格限制下，只在手部重新生成“挥剑”动作的像素点。 [1 (https://www.pixellab.ai/), 2 (https://www.spritefusion.com/ai-pixel-art-editor)]

🚀 结论

极其可行。 用 MaartenGr/Sprite-Generator 的算法思想用 Rust 快速低成本地生成静态像素根基（解决“无中生有”），再编写一套类似 Pixelorama / Piskel 的底层像素矩阵变换函数库（解决“让它动起来”），最后交给后台的 WASM Agent 去调用。

这套方案完美的避开了商业 2D AI 生成模型常有的“多帧不连贯”、“画风前后不一致（Consistency 闪烁）”的致命痛点，因为你的每一帧都是基于第一帧在内存里进行严丝合缝的数学计算和位移得来的，是非常纯正的像素级硬核骨骼动画。 [1 (https://medium.com/@creativeaininja/sprite-gen-turns-ai-images-into-sprites-your-game-can-actually-use-f32b7091a0e4)]
