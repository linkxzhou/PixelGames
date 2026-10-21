# 参考资料 01：Rust + WASM 像素引擎方案与业界像素游戏技术栈（原文存档）

> 由 linkxzhou 于 2026-10-21 提供，原文保留。其中部分结论后续已修正，见 `plan/`。

从“3D 工业流”降维到“2D 像素游戏引擎”是一个极其聪明的商业与技术卡位。
由于像素游戏的数据结构（Tilemap 瓦片地图、Spritesheet 精灵图、RGB 像素矩阵）极其规律且体量微小，你完全可以绕过庞大的 3D 资产生成和显存瓶颈。整个管线（代码、关卡、像素画、数值）可以实现100% 由本地轻量级 Agent 实时生成并即时运行，真正做到“一人一世界”的无限生成游戏。
以下是针对这个垂直领域的可行性分析、业界开源现状以及推荐的 Rust + WASM 开源组件方案：

一、 方案合理性分析（Feasibility & Strategy）

优势（💡 为什么这个方案极具竞争力？）

1. 内存与传输零压力： 一张 3D 贴图动辄数 MB，而一张 \(16\times16\) 的像素角色图片只有几百个字节。这意味着你的 SharedArrayBuffer 零拷贝设计可以快到极致，Agent 生成的像素矩阵可以直接以 [u8; 4]（RGBA）的形式秒级推送到渲染层。
2. 本地 Agent 推理成为可能： 生成 2D 像素图或 Tilemap 关卡，不需要几十 B 参数的巨型模型。在浏览器本地（通过 WASM + WebGPU）跑一个百 M 级别的 Smollm 或 Llama-3.2-1B 编代码/关卡，再跑一个轻量级 Pixel-Diffusion 甚至纯符号规则（PCG 过程式生成算法），即可实现全离线闭环，算力成本为 0。

核心挑战（⚠️ 需要解决的难点）

• 像素对齐与伪影（Artifacts）： 2D 像素游戏在浏览器中放大时，如果渲染器的抗锯齿（Anti-aliasing）没有关闭，或者相机移动不是整数像素，画面会变得模糊。Rust 渲染层必须支持严格的 Pixel-Perfect（像素完美） 渲染。

二、 业界/开源像素游戏引擎现状

目前市面上结合了 Rust、WASM 或 AI 的 2D/像素引擎主要有以下几类，它们可以作为你的锚点：
1. Pixengine / PixelGameEngine 类（纯像素流）： 传统像素引擎，强调对每一个像素点的控制（draw_pixel(x, y, color)），但对现代大模型 Agent 的接入和高性能 Web 渲染优化不足。
2. Macroquad / Miniquad： 极简、超快、编译体积极小的 Rust 2D 引擎。它对 WASM 的支持比 Bevy 更轻量（编译后只有几百 KB），非常适合做网页小游戏，但缺乏高级的 AI 调度框架。
3. Fantasy Consoles (虚拟幻想机，如 PICO-8 / TIC-80)： 这是你最值得借鉴的产品形态。PICO-8 将代码、像素画、地图、音效全部限制在一个微小的格式里。你的引擎本质上可以做成一个 “AI 驱动的下一代 PICO-8”——用户只需输入一句自然语言，Agent 自动在这四个标准槽位里填满二进制像素和脚本代码。

三、 推荐的 Rust + WASM 开源组件生态矩阵

为了实现“Agent 实时生成 -> WASM 内存中转 -> 网页高效像素渲染”的闭环，建议采用以下开源组件搭建你的引擎：

1. 核心渲染与游戏框架：Macroquad 或 Bevy (开启 2D 特性)

• 推荐选择：Macroquad
    • 理由： 它的核心设计就是“开箱即用的 2D 游戏”。它对像素画支持极好（默认易于关闭纹理过滤以保持硬边缘），最重要的是编译速度和 WASM 体积远小于 Bevy。非常适合需要频繁冷启动、动态加载 Agent 代码的 Web 场景。
• 备选（如果你需要复杂的 ECS 架构）：Bevy Engine。禁用所有 3D 插件，只保留 bevy_sprite 和 bevy_render。它有现成的 bevy_ecs_tilemap 库，极其适合处理庞大的像素复古关卡。

2. UI 与 编辑器交互层：egui

• 理由： 纯 Rust 编写的即时模式 UI。它可以直接内嵌在 Macroquad 或 Bevy 中。你可以用它在浏览器里绘制“对话框”、“生成进度条”或者“像素微调面板”。它与 WASM/WGPU 的集成度是目前 Rust 生态中最好的。

3. 浏览器本地 AI 推理层（Agent 核心）：Candle

• 理由： Hugging Face 官方的 Rust 极简深度学习框架。
    • Text Agent： 编译到 WASM 后，通过 WebGPU 调用本地量化版的 Qwen-1.5-B 或 Llama-3.2-1B。它专门负责接收用户的指令，将其转化为游戏关卡的 JSON 描述（如：{ "tile": "lava", "x": 10, "y": 5 }）或者生成角色的属性行为脚本。
    • Pixel Agent： 如果需要动态生成前所未有的像素鬼怪，可以用 Candle 运行超轻量级的 Pixel-VAE 或 Conditional-GAN，直接在 WASM 内存里输出一个 Vec<u8> 的像素颜色数组，甚至不需要走外部网络请求。

4. 脚本动态执行层（热重载）：Rhai 或 Rua (Lua in Rust)

• 痛点： Agent 实时生成的“游戏逻辑”（比如：怪物 AI、子弹轨迹）不能直接写进 Rust 里重新编译（因为是在用户的浏览器里运行）。
• 解法： 引入 Rhai（一个完全用 Rust 编写的轻量级嵌入式脚本语言）。Agent 生成 Rhai 文本脚本，引擎在 WASM 运行时（Runtime）中动态解析并绑定到游戏实体上。它对 WASM 原生友好，安全且极快。

🗺️ 技术流水线架构设计

```
[ 用户输入: "创造一个岩浆主题的横版硬核关卡" ]
                     │
                     ▼
[ 1. AI 线程 (Candle WASM + WebGPU) ] 
   ├── Text Agent ──> 生成 Rhai 怪物行为脚本 & 关卡 JSON 矩阵
   └── Pixel Agent ─> 生成 16x16 RGBA 像素资产 (Raw Buffer)
                     │
                     ▼ (通过 SharedArrayBuffer 零拷贝传递指针)
[ 2. 引擎主线程 (Macroquad + egui WASM) ]
   ├── 加载 Raw Buffer 变为 2D Texture (设置纹理过滤为 Nearest 确保像素硬边缘)
   ├── 解析 Rhai 脚本绑定到游戏对象循环 (Tick)
   └── 渲染到浏览器 Canvas 
```

💡 商业与产品切入点建议

你可以将这个垂直引擎定位为 “The Generative Fantasy Console (生成式幻想游戏机)”：
1. 彻底去掉文件系统： 游戏不再有 .png、.mp3，一切资产在内存中都是结构化的代码和可编程矩阵（PCG + AI）。
2. 无限漫游模式： 当玩家控制小王子走到地图边缘时，WASM 后台的 Agent 动态以当前的种子（Seed）和上下文，本地实时推理并“生出”下一块像素地图和新怪物。
这个“全栈 Rust+WASM 像素 AI 引擎”的方向极其敏捷且避开了重资产的坑。接下来你需要我：
• 为你搭建一个 Macroquad + egui 编译到 WebAssembly 的最小可行性脚手架 (MVP) 工程结构与 Cargo.toml 依赖？
• 深入探讨如何设计 Rhai 脚本接口，好让大模型 Agent 生成的代码能够无缝控制像素人物的移动和碰撞？

---

在像素游戏的历史和现代独立游戏浪潮中，诞生了无数跨时代的神作。有趣的是，许多最顶尖的像素游戏并没有使用 Unity 或 Unreal 等重型商业引擎，而是选择了更轻量、对 2D 极其友好的专门引擎，甚至是开发者纯手速撸的自研引擎 / 框架。
以下是业界最著名的像素游戏及其背后所使用的引擎与技术栈：

1. 独狼与奇迹：C# / XNA 与 MonoGame 阵营

这一派系的特点是“不直接用现成引擎，而是基于底层框架自己写引擎”。这与你构想的用 Rust 自建像素引擎的思路高度契合。
• 《星露谷物语》（Stardew Valley）
    • 开发引擎/技术： 自研引擎（基于 Microsoft XNA 框架，后迁移至 MonoGame）
    • 技术内幕： 作者 ConcernedApe 独自一人用了 4 年多时间，用 C# 在微软早就停止维护的 XNA 框架上硬生生拼出了一个完整的游戏引擎。后来为了兼容现代 64 位系统和更庞大的 MOD 生态，游戏在 2021 年整体迁移到了 XNA 的开源继承者 MonoGame。 [1] (https://www.reddit.com/r/gamedev/comments/16mnf91/how_long_would_it_have_taken_concernedape_to_make/), [2] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/), [3] (https://community.playstarbound.com/threads/some-of-my-questions-for-concerned-ape.142087/), [4] (https://en.wikipedia.org/wiki/Stardew_Valley)
• 《泰拉瑞亚》（Terraria）
    • 开发引擎/技术： 自研引擎（同样基于 C# + XNA 框架）
    • 技术内幕： 作为一个拥有海量瓦片（Tilemap）和光源计算的沙盒游戏，《泰拉瑞亚》的引擎是百分之百纯手写的。其特殊的网格挖掘、流体物理以及完全硬编码的资产管理方式，都证明了专为像素细分领域定制的底层引擎具有极高的性能优势。 [1] (https://forums.terraria.org/index.php?threads/what-is-the-game-engine-and-why-do-people-say-that-it-is-limiting.122886/), [2] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/), [3] (https://www.bisecthosting.com/blog/terraria-engine-specs)
• 《蔚蓝》（Celeste）
    • 开发引擎/技术： Monocle 引擎（基于 XNA / FNA / MonoGame）
    • 技术内幕： 开发者 Noel Berry 基于 C# 开发了一套专门应对精密跳跃平台的 2D 框架 Monocle。《蔚蓝》那堪称丝滑、无死角的像素级碰撞检测（Pixel-Perfect Collision），正是得益于这种完全可控的底层代码结构。 [1] (https://www.reddit.com/r/gamedev/comments/7vjobd/whats_a_game_like_celeste_made_in/), [2] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/), [3] (https://celestegame.tumblr.com/tools)

2. 独立游戏大户：GameMaker 阵营

GameMaker 是像素游戏界“yyds（永远的神）”，许多现象级作品都诞生于此。它开箱即用的像素完美渲染和专门的脚本语言（GML），让其长盛不衰。
• 《传说之下》（Undertale）
    • 开发引擎： GameMaker Studio
    • 技术内幕： 狐狸哥（Toby Fox）几乎只靠 GameMaker 默认的功能和基础的 2D 房间编辑器（Room Editor），就创造出了这款剧情和反传统机制逆天的神作，证明了“机制与剧情上限取决于脑洞，而非引擎”。 [1] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/)
• 《武士零》（Katana ZERO）
    • 开发引擎： GameMaker Studio
    • 技术内幕： 该作将 GameMaker 的 2D 物理和粒子特效发挥到了极致。极其华丽的刀光、子弹时间下的像素碎屑、以及严丝合缝的打击判定，都是在 GameMaker 的 2D 环境下深度定制的成果。 [1] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/), [2] (https://www.sprite-ai.art/blog/best-game-engines-pixel-art)
• 《热线迈阿密》（Hotline Miami）
    • 开发引擎： GameMaker (早期版本)
    • 技术内幕： 纯粹的暴力美学与俯视角像素射击的完美结合，早期完全由 GameMaker 快速原型迭代而来。

3. 硬核底层流：Haxe / Heaps 阵营

• 《死亡细胞》（Dead Cells）
    • 开发引擎： Heaps.io (基于 Haxe 语言)
    • 技术内幕： 这一款是真正的底层技术流。开发商 Motion Twin 的主力程序员 Nicolas Cannasse 正是 Haxe 语言和 Heaps 引擎的亲生父亲。由于《死亡细胞》其实是用 3D 模型降维渲染为 2D 像素帧（把 3D 骨骼动画实时转为像素图），普通的 2D 引擎很难承受这种计算，于是他们用自研的 GPU 加速引擎 Heaps，在底层把性能榨干到了极限。 [1] (https://www.reddit.com/r/gamedev/comments/6mi5ns/can_we_talk_about_how_amazingly_responsive_dead/), [2] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/), [3] (https://heaps.io/), [4] (https://haxe.org/use-cases/games/)

4. 商业与跨平台巨头：Unity 阵营

虽然 Unity 默认是 3D 引擎，但通过关闭抗锯齿、配置点过滤（Point Filtering）和正交相机（Orthographic Camera），它依然统治了大量的现代像素商业大作。 [1] (https://www.sprite-ai.art/blog/best-game-engines-pixel-art)
• 《八方旅人》（Octopath Traveler）系列
    • 开发引擎： Unreal Engine 4（注：非纯 Unity，它开创了 HD-2D 风格）
    • 技术内幕： 严格来说它是“3D 像素”。用虚幻引擎的 3D 真实动态光影、景深衰减、粒子特效，去照射 2D 的像素纸片人角色，将古典像素与现代现代图形学完美融合。 [1] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/)
• 《东方夜雀食堂》、《光明旅者》（Hyper Light Drifter）
    • 开发引擎： Unity
    • 技术内幕： 借助 Unity 庞大的跨平台编译能力、多语言本地化系统和成熟的粒子/照明管线，方便团队进行工业化协作和主机移植。 [1] (https://soulbound.game/blog/ultimate-list-of-pixel-art-games-25-classics-that-defined-a-genre/)

5. 极简复古与原型圣地：PICO-8 幻想机

• 《蔚蓝》初代原型（Celeste Classic）
    • 开发引擎： PICO-8
    • 技术内幕： 《蔚蓝》在诞生为大作之前，作者是在一个名为 PICO-8 的虚拟机里，用几百行 Lua 代码和 \(128\times128\) 的像素限制，在 4 天内搓出了最初的初版原型。 [1] (https://celestegame.tumblr.com/tools), [2] (https://www.reddit.com/r/gamedev/comments/7vjobd/whats_a_game_like_celeste_made_in/)

📊 直观对比表

| 游戏名称 | 游戏类型 | 核心引擎 / 技术栈 | 核心技术特点 |
| --- | --- | --- | --- |
| 《星露谷物语》 | 模拟经营 RPG | C# + MonoGame (自研) | 数据量巨大，纯内存逻辑流，无引擎包袱 |
| 《泰拉瑞亚》 | 2D 沙盒生存 | C# + XNA (自研) | 极其强悍的 Tilemap 动态刷新与挖掘性能 |
| 《死亡细胞》 | 动作 Roguelike | Haxe + Heaps.io (开源) | 3D 骨骼动画实时转 2D 像素，极致的动作响应 |
| 《蔚蓝》 | 平台跳跃 | C# + Monocle (自研框架) | 严格到像素点（Pixel-Perfect）的碰撞检测与手感 |
| 《传说之下》 | 剧情向 RPG | GameMaker Studio | 快速原型开发，极高的数据与脚本自由度 |
| 《八方旅人》 | HD-2D JRPG | Unreal Engine 4 | 3D 光影材质与 2D 像素的降维打击结合 |

💡 对你自研 Rust 像素引擎的启示

从上述神作可以看出，做像素游戏引擎，“重型可视化”不是核心，对底层内存和像素网格（Tilemap/Grid）的绝对控制才是关键。
《星露谷》和《泰拉瑞亚》的成功证明，基于底层框架（如 C# 的 XNA）自研的专有引擎，在应对像素级逻辑时，性能和自由度远超商业通用引擎。这为你选择 Rust (Macroquad / wgpu) + WASM 提供了极强的行业先例信心——你正在走一条当年 ConcernedApe 走过的路，只是你手里拿的是更现代、更适合浏览器的 Rust 武器。 [1] (https://forums.terraria.org/index.php?threads/what-is-the-game-engine-and-why-do-people-say-that-it-is-limiting.122886/), [2] (https://www.reddit.com/r/gamedev/comments/16mnf91/how_long_would_it_have_taken_concernedape_to_make/)
