# v3.0 Plan v4：第三方像素画项目算法测绘与 PixelGames 能力对比

> 日期：2026-10-26
> 前置：`pxl-native-resolution-quality-plan.md`（原生分辨率）、`pxl-pixelart-pipeline-v2-plan.md`（`pxl-art` 增强管线）、`pxl-multi-size-ascii-svg-plan.md`（多尺寸 + SVG 栅格化）。
> 分析对象：`thirdparty/pixel-scaler`、`thirdparty/piskel`、`thirdparty/Pixelorama`、`thirdparty/Sprite-Generator`，以及本报工作区 `crates/`（11 个 crate / 146 个 `.rs`）。
> 范围：本文件是**测绘 + 差距诊断 + 排期**，不改代码。后续实施仍只改 `crates/`。
> 方法：逐项目通读 `src/`（跳过 `node_modules`、`tests`、`assets` 二进制），以**符号名 + 行号**定位；行号是检索起点，重构后以符号名为准。

---

## 0. 结论摘要

1. **四个第三方项目的算法总量约 110 项，其中真正"像素画专用"的只有 23 项**；其余是通用 RGBA 图像编辑（模糊、卷积、混合模式、色彩曲线），与我们的索引帧模型存在**原理性冲突**，不应移植。
2. **Pixelorama 是唯一在算法深度上全面超过我们的项目**（19 个层效果、22+ 混合模式、7 种旋转算法、Allegro 扫描线 flood fill、SmartSlicer 图集切分）。piskel 与 pixel-scaler 的算法密度远低于我们，Sprite-Generator 的全部生成逻辑我们已在 `pxl-gen/src/bollinger.rs` 完整重实现并修掉了它的两个 bug。
3. **我们有三处是"独有且更优"的**：SVG 扫描线 + 多数投票光栅化（全行业没有第二家把"不产生新色"做到这个程度）、selout 描边（Pixelorama 只有平描边）、12 项 QA 度量体系（其余四家零 QA）。
4. **真实缺口只有 6 项，集中在"交互式编辑"而非"渲染"**：
   - **选区与填充完全缺失**（flood fill / 魔棒 / 套索 / 椭圆选区 / 形态学选区）——这是最大空白，`pxl-format::Region` 只有矩形。
   - **旋转后处理缺失**（cleanEdge / Rotxel / 补洞），我们的 RotSprite 是 EPX×3 + 最近邻，斜线保形弱于 Pixelorama。
   - **抖动矩阵只有 4×4**（缺 2×2 / 8×8 / 16×16 / 蓝噪声）。
   - **调色板无排序**（Pixelorama 9 种、piskel 按色相分桶）。
   - **GIF 无帧差压缩**（pixel-scaler 用"颜色距离平方 ≤16 视为未变"做到，我们不压缩）。
   - **图集无法反向切分**（只有打包，没有 sheet → frames）。
5. **一条架构级建议**：引入**仅在导入/导出边界存在**的 RGBA 工作缓冲。内部永远索引帧；`import: RGBA → quantize → index`、`export: index → RGBA`。这样既不破坏"绝不产生调色板外颜色"的不变量，又能吃掉第三方项目里那些"必须先有连续值"的能力（网格检测、边缘检测、帧差、AA 残留判定）。

---

## 1. 四个项目的技术画像

| 项目 | 语言 / 渲染后端 | 数据模型 | 算法密度 | 算法特征 |
|---|---|---|---|---|
| **pixel-scaler** | Vue 3 + TS，Canvas 2D（**Rust 侧零图像代码**，`src-tauri` 只有 Tauri 壳 18 行） | `ImageData` / `Uint32Array`（RGBA） | **极低**：真正只有 xBR（外部库 `xbr-js`）+ Canvas 原生最近邻 | 全仓库 0 个 shader、0 个 Worker、0 个 wasm。算法价值在**GIF 容器处理**（帧差、disposal、透明索引） |
| **piskel** | 纯 JS（无 TS/WebGL），Canvas 2D | `Uint32Array`，小端 `a<<24\|b<<16\|g<<8\|r`，透明归零 | **低**：核心是 `drawImage` + `imageSmoothingEnabled` | 无量化、无描边、无 AA、无混合模式。价值在**交互工具几何**（均匀步长直线、像素完美圆、镜像笔、套索内外判定）与**调色板排序** |
| **Pixelorama** | Godot 4.7 GDScript，`gl_compatibility` + NEAREST 过滤 | **双图模型**：`Image`（RGB）+ `indices_image`（索引存在 R 通道，0=透明） | **高**：19 个层效果、27 个 shader、7 种旋转、扫描线 flood fill | 绝大多数效果是 **GPU `texture_blit` shader**（`ShaderLoader.generate_texture_blit_shader` 自动包装 `apply_effect()`）；算法广度最大，但质量参差（作者本人标注多处"来自 godotshaders"） |
| **Sprite-Generator** | Python 3 + Pillow 7.1.2，**无 numpy** | `list[list[int]]` 符号掩码（0/1/2/-1）→ `putdata` | **极低**：单文件 157 行 | 纯 PCG（模板掩码 + Bernoulli + 镜像 + 4 邻域膨胀 + 正弦光照），**不是图像处理，也没有遗传算法** |

### 1.1 数据模型决定算法上限（关键判断）

这是本次分析最重要的一条结构性结论：

| 模型 | 代表 | 能力上限 | 代价 |
|---|---|---|---|
| **RGBA 连续值** | pixel-scaler / piskel / Godot `Image` | 什么都能做：混合放大、模糊、任意插值、半透明 | 必然产生调色板外颜色 → 必须事后量化；透明边缘易出现 AA 脏边 |
| **索引 + 调色板（我们）** | `pxl-core::IndexFrame` | 恒定色数、零 AA、比特级可复现 | **无法表达连续值**：不能模糊、不能混合、不能亚像素采样 |
| **双图（RGB + 索引图）** | Pixelorama | 绘制时 O(N) 最近色锁定调色板，shader 做显示 | 双份内存 + 索引/RGB 同步成本；效果一旦走 shader 就脱离调色板 |

我们的定位是第二种，这是**优势**（确定性、可 QA、文件小、无脏边），但也解释了两个"为什么做不到"：

- 为什么不做混合型 xBR / OmniScale / 双线性放大？——输出必然落在调色板外，需要再做一次量化，等于"先污染再净化"，比直接 MMPX/contour 更差。
- 为什么不做 GaussianBlur / ConvolutionMatrix？——即使做了，也必须立即 `map_to_palette`，中间结果无意义，最终只剩下"抖动后的色块"，不如直接用 ramp 分带 + Bayer。

**因此第三方项目里的连续值滤镜族，整体判定为"不移植"，而不是"待实现"。**

---

## 2. 算法测绘（按类别）

> 图例：`✔` 完整实现 / `◐` 简化、间接或仅作辅助 / `✘` 无。
> "我方"列的路径相对 workspace 根。

### 2.1 放大 / 重采样

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 | 说明 |
|---|---|---|---|---|---|---|
| Nearest / k×k 块复制 | ◐ Canvas | ◐ Canvas | ✔ | ✔ PIL | **✔** `crates/pxl-core/src/scale.rs:20 scale_copy` | 我方透明像素跳过 |
| EPX / Scale2x | ✘ | ✘ | ◐ Scale3x 内含 | ✘ | **✔** `scale.rs:55 epx2x`（仅 2×） | 越界视作透明，透明永不填充 |
| Scale3x | ✘ | ✘ | **✔** `DrawingAlgos.gd:329`（`tol=0.196` 边缘判定） | ✘ | **✔** `crates/pxl-art/src/upscale.rs:147 scale3x` | 逐位等价的可移植规则 |
| xBR | **✔** 外部库 `xbr-js` 2×/3×/4×，`blendColors=true` | ✘ | ✘ | ✘ | **◐** `upscale.rs:207 xbr_index` | 我们是**索引变体**：角规则同 EPX，"同色"放宽为 OKLab 亮度差 ≤ 调色板跨度/4；**不混合、仅 2×** |
| xBR 级联拆解（超 400% 递归） | **✔** `xBR.ts:149 calcScalePercents` | ✘ | ✘ | ✘ | **✘** | 见 P1-5 |
| MMPX (McGuire & Gagiu 2021) | ✘ | ✘ | ✘ | ✘ | **✔** `crates/pxl-art/src/mmpx.rs:28 mmpx2x` / `:173 mmpx(k)` | 完整 4 组规则（1:1 斜线、交点、三角尖、2:1 斜坡），2×/4×；亮度平局用 OKLab L |
| 自研矢量轮廓重建（Contour） | ✘ | ✘ | ✘ | ✘ | **✔** `upscale.rs:270` → `contour.rs:52 reconstruct` | 闭环追踪 → RDP 简化 → 阶梯理想化 → 二次曲线 → 扫描线偶奇填充；2/3/4× |
| Hybrid 按材质逐像素选路 | ✘ | ✘ | ✘ | ✘ | **✔** `upscale.rs:111 hybrid` | Metal/Glass 走 xbr/scale3x，其余 contour |
| HQ2x/3x/4x | ✘ | ✘ | ✘ | ✘ | ✘ | **不移植**（见 §4.4） |
| Eagle / SuperEagle / 2xSaI / Super2xSaI | ✘ | ✘ | ✘ | ✘ | ✘ | 同上 |
| OmniScale（RetroArch） | ✘ | ✘ | **✔** `Shaders/Effects/Rotation/OmniScale.gdshader` | ✘ | ✘ | 插值型，产生新色 → 不移植 |
| cleanEdge（torcado, MIT） | ✘ | ✘ | **✔** `Shaders/Effects/Rotation/cleanEdge.gdshader` | ✘ | **✘** | **索引安全的纯重排**，值得补 → P0-2 |
| Rotxel（3× 子格 9 邻居投票） | ✘ | ✘ | **✔** `DrawingAlgos.gd:528` | ✘ | **✘** | 索引安全 → P0-2 |
| URD（放-转-缩） | ✘ | ✘ | **✔** `DrawingAlgos.gd:715` | ✘ | ◐ `rotsprite` 结构类似 | — |
| Smear 补洞（角度扫描） | ✘ | ✘ | **✔** `SmearRotxel.gdshader:29`（每 0.0174 rad 试角） | ✘ | **✘** | 索引安全 → P0-2 |
| 任意倍率（5×/1.5×） | ✔ 百分比 | ✔ 任意 | ✔ 任意 | ◐ 2× | **◐** 仅 2/3/4 | → P1-5 级联组合器 |
| 覆盖率加权混合降采样 | ◐ 浏览器 | ◐ 浏览器 | ✔ 双线性/三次 | ◐ PIL | **✘** 且**刻意不做** | 我们改用众数/k-centroid/极值（`pixelize.rs:358`） |

### 2.2 色彩量化与调色板

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 |
|---|---|---|---|---|---|
| Median cut | ◐ `gifenc`（`rgba4444`） | ✘ | **✔** `addons/gdgifexporter/quantization/median_cut.gd:32`（8 轮 → 256，选最长轴，桶内取均值） | ✘ | **✔** `crates/pxl-art/src/quantize.rs:122 median_cut`（**OKLab + 亮度权重 ×2**） |
| Lloyd k-means 精修 | ✘ | ✘ | ✘ | ✘ | **✔** `quantize.rs:193`（嵌在 median cut 内，轮数可配） |
| NeuQuant / PAM | ✘ | **✔** `lib/gif/gif.worker.js` | ✘ | ✘ | ✘（不需要：样本少、色数少） |
| Uniform 量化 | ✘ | ✘ | ✔ `quantization/uniform.gd` | ✘ | ✘ |
| Octree / Wu | ✘ | ✘ | ✘ | ✘ | ✘ |
| 最近色映射 | ◐ | ✘ | **✔** `ImageExtended.gd:156`（绘制时 O(N) 扫描，优先当前色板项） | ✘ | **✔** `quantize.rs:61 map_to_palette`（OKLab 欧氏平方，Q16 定点） |
| 从图提取调色板 | ✘ | **✔** 唯一色哈希去重（`ImageProcessorWorker.js:61`，alpha<125 透明，上限 256） | ✔ `Palettes.gd:275`（可选 add_alpha_colors） | ✘ | **✔** 等价于 median_cut |
| **调色板排序** | ✘ | **✔** `ColorSorter.js:19`（暗/亮/低饱和三分 + 36° 十桶） | **✔** `Palette.gd:303`（Hue/Sat/Value/**OKHSL 亮度**/R/G/B/Alpha + Reverse，共 9 种） | ✘ | **✘** → P0-4 |
| OKLab / OKHSL 亮度 | ✘ | ✘ | ✔（`bottosson` 移植，含 toe 曲线） | ✘ | **✔** `crates/pxl-core/src/color.rs:173`（全整数 icbrt） |
| 色差度量 | ◐ RGB 欧氏平方 | ◐ 精确相等 | ◐ **RGBA 欧氏距离** `ImageExtended.gd:193` | ✘ | **✔** OKLab 欧氏平方 |
| 容差匹配（逐通道 ∞-范数） | ✘ | ✘ | **✔** `DrawingAlgos.gd:754 similar_colors`（默认 tol≈100/255） | ✘ | **✘** → P0-1（交互语义，见 §4.2） |
| 色阶/色带生成 | ✘ | ✘ | ✘ | ◐ 逐行 HLS `Sprite.py:108` | **✔** `crates/pxl-art/src/palette.rs:83 build_shades`（OKLab 明度步进 + 阴影偏蓝/高光偏暖） |
| 调色板色阶位移 / cycle | ✘ | ✘ | ◐ `Shading.gd:19`（colors_right 左移 N 格） | ✘ | **✔** `pxl-core/src/palette.rs:175 shift_in_ramp` / `:245 cycle_map` |
| 调色板 harmony（互补/三角） | ✘ | ✘ | ✘ | ✘ | ✘（低价值，暂不做） |
| 调色板文件解析（GPL/PAL/HEX） | ✘ | **✔** `PaletteGplReader.js` / `PalettePalReader.js` | ✔ GPL / JASC-PAL / PNG 网格 | ✘ | ✘ → P2-7 |

### 2.3 抖动

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 |
|---|---|---|---|---|---|
| Bayer 2×2 | ✘ | ◐ 棋盘笔 `(col+row)%2` | **✔** `assets/dither-matrices/` | ◐ `colored=False` 黑白 | ✘ |
| Bayer 4×4 | ✘ | ◐ 同上 | ✔ 同上 | ✘ | **✔** `quantize.rs:17 BAYER4`（三处复用：量化 `:79 pick`、`finish.rs:419 ramp_dither`、`pixels.rs:8 dissolve`） |
| Bayer 8×8 / 16×16 | ✘ | ✘ | **✔** | ✘ | **✘** → P0-3 |
| 误差扩散（FS/JJN/Atkinson/Stucki/Riemersma） | ✘ | ✘ | ✘ | ✘ | ✘（明确不做，见 §4.4） |
| 蓝噪声 / void-and-cluster | ✘ | ✘ | ✘ | ✘ | **✘** → P0-3（低成本、高观感收益） |
| 渐变阈值抖动 | ✘ | ✘ | **✔** `Gradient.gdshaderinc:77`（N×1 双纹理查阈值） | ✘ | ◐ `finish.rs:419`（按带高 1px/2px 用 Bayer） |
| Posterize + 2×2 棋盘 | ✘ | ✘ | **✔** `Posterize.gdshaderinc:6`（`round(c*colors ± dither)/colors`） | ✘ | ◐ ramp 分带等价 |

### 2.4 几何变换与旋转

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 |
|---|---|---|---|---|---|
| 任意角最近邻旋转 | ✘ | ✘ | **✔** `DrawingAlgos.gd:732` + `NearestNeighbour.gdshader:7`（UV 先量化到像素中心） | ✘ | **✔** `crates/pxl-ops/src/apply/transform.rs:291 rotate_nearest` |
| RotSprite | ✘ | ✘ | ✘ | ✘ | **◐** `crates/pxl-art/src/rotsprite.rs:59`（EPX ×3 = 8× 放大 + 反向 1/8 像素采样，**不混合、无补洞**） |
| 三次斜切（three-shear） | ✘ | ✘ | ✘ | ✘ | **◐** 仅单次 `transform.rs:99 shear`，未用于旋转 |
| 90°/180° 旋转（补方→转置→裁回） | ✘ | **✔** `TransformUtils.js:26` | ✘ | ✘ | **✔** `transform.rs:234 rotate90` |
| 四点自由变换 / 透视贴合 | ✘ | ✘ | **✔** `CornerPin.gdshaderinc:16`（单位方格双线性反解） | ✘ | **✘** → P2-2 |
| 斜切 / 挤压拉伸（锚点 9 宫格） | ✘ | ◐ | ◐ 仿射 | ✘ | **✔** `transform.rs:99 shear` / `:144 squash`（5 锚点） |
| 环绕平移（wrap） | ✘ | **✔** `Move.js:49`（`(x+w)%w`） | **✔** `OffsetPixels.gdshaderinc:1` | ✘ | **✔** `transform.rs:5 translate(wrap)` / `:61 shift_rows` |
| 正弦行位移（sine4 查表） | ✘ | ✘ | ✘ | ✘ | **✔** `transform.rs:39 scroll_rect`（0,1,0,-1 LUT） |
| crop / 非透明包围盒 / 居中 | ✘ | **✔** `TransformUtils.js:67/113` + `Crop.js:45` | **✔** `crop_to_content` `DrawingAlgos.gd:1023` | ✘ | **◐** QA 有 bbox（`pxl-qa/src/pixels.rs:141`），**无 crop op** |
| 画布 resize（内容不动） | ✘ | **✔** `ResizeUtils.js:45`（9 宫格 origin） | **✔** `resize_canvas` `:1047` | ✘ | ✘ |
| 等距 / 六边形网格生成 | ✘ | ✘ | **✔** `DrawingAlgos.gd:767/845/873`（Bresenham 画 2:1 菱形边） | ✘ | ✘ → P2-1 |
| Flat → 等距投影 | ✘ | ✘ | **✔** `FlatToIsometric.gdshaderinc:1` | ✘ | ✘ |
| 4 轴对称（含两条对角） | ✘ | ◐ 2/4 象限 `VerticalMirrorPen.js:22` | **✔** `Tools.gd:642`（对角用 `Vector2.reflect` + ±0.5 偏置） | ◐ 仅水平镜像 | **◐** `crates/pxl-gen/src/shapes.rs:69 mirror_axis`（仅轴对齐） |

### 2.5 描边 / 形态学

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 |
|---|---|---|---|---|---|
| 外/内描边 + 笔刷形状（□/◇/○） | ✘ | ✘（`Stroke` 是画线笔） | **✔** `OutlineInline.gdshaderinc:12`：radius 内按 3 种像素笔刷扫邻居 alpha；`inside` 切内外；`keep_border_only` 出边框 | ✘ | **✔** `crates/pxl-core/src/outline.rs:47 paint_outline`（Outer/Inner × Square/Diamond/Circle，宽度 **1..=4**） |
| 描边宽度 > 4 | ✘ | ✘ | ✔ 0..10 | ✘ | ✘（上限 4）→ P1-3 |
| 选区膨胀/腐蚀/取边框 | ✘ | ✘ | **✔** `SelectionMap.gd:210/220/241`（复用描边 shader） | ✘ | **✘** → P0-1 |
| **通用形态学 dilate/erode/open/close** | ✘ | ✘ | ◐ 仅选区场景 | ✘ | **✘** → P1-3 |
| 剥离外描边（保留内部轮廓线） | ✘ | ✘ | ✘ | ✘ | **✔** `crates/pxl-art/src/outline.rs:79 strip_outer` |
| **Selout / SeloutLit（按邻近色阶自发光描边）** | ✘ | ✘ | ✘ | ✘ | **✔** `outline.rs:100 apply` / `:167 ring_color`（取 4 邻域**最暗**填充色走 `ShadeMap.deep`，受光侧升一级） |
| 描边 1px 断口修补 | ✘ | ✘ | ✘ | ✘ | **✔** `outline.rs:130`（对角缝隙补 1 像素） |
| Drop Shadow | ✘ | ✘ | **✔** `DropShadow.gdshaderinc:1`（整数偏移采样 alpha） | ✘ | **✘** → P1-4（索引安全） |
| AA 去边（despeckle anti-alias removal） | ✘ | ✘ | ✘ | ✘ | **◐** `pixelize.rs:358` 的 kcentroid 会剔抗锯齿边 |
| 孤立点/拐角/缺口/轮廓节奏/对称清理 | ✘ | ✘ | ✘ | ✘ | **✔** `crates/pxl-art/src/cleanup.rs:91/116/148/192/344` + `analyze.rs:105` 保护掩码 |

### 2.6 选区与填充

| 算法 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | 我方 |
|---|---|---|---|---|---|
| **扫描线 flood fill** | ✘ | ✘（注释写了 scanline，实现是 `queue.pop()` DFS，`PixelUtils.js:118`） | **✔** `FloodFillObject.gd:29`（Allegro 4.2.1 `flood.c` 移植，按行 Segment 链表，只检查上下行未覆盖段） | ✘ | **✘** |
| 魔棒（同色连通选择） | ✘ | **✔** `PixelUtils.js:43` + `ShapeSelect.js:36` | **✔** `MagicWand.gd:51`（tolerance 滑杆 0..255 → `/255`） | ✘ | **✘** |
| 全局同色选择（非连续） | ✘ | ✘ | **✔** `ColorSelect.gdshader:19`（GPU 全图比较，add/sub/intersect） | ✘ | **✘** |
| 套索（多边形内外判定） | ✘ | **✔** `LassoSelection.js:46`（连通扩散 + 边框可达性判内外） | **✔** `Lasso.gd:115` + `Geometry2D.is_point_in_polygon` | ✘ | **✘** |
| 矩形 / 椭圆选区 | ✘ | ◐ 伪帧技巧 `Crop.js:113` | **✔** `RectSelect.gd` / `EllipseSelect.gd` + `DrawingAlgos.gd:234 get_ellipse_points` | ✘ | **◐** `pxl-format::Region` 仅矩形 |
| 选区布尔运算（add/sub/intersect） | ✘ | ✘ | **✔** `operation` 0/1/2 | ✘ | **✘** |
| 图案/纹理填充（tile + 混合） | ✘ | ✘ | **✔** `PatternFill.gdshader:14` | ✘ | **◐** `pxl-gen/src/shapes.rs:316 pattern_on`（仅 checker/stripe_x/stripe_y） |
| 颜色替换（带容差） | ✘ | ◐ `ColorSwap.js:83` 精确 `==` | **✔** `ColorReplace.gdshader` + `Bucket.gd:249` | ✘ | **◐** `pxl-ops/src/apply/color.rs:25 palette_swap`（按索引/ramp，无容差） |

### 2.7 连续值滤镜与混合（RGBA 域，整体判定不移植）

| 算法 | pixel-scaler | piskel | Pixelorama | 我方 | 判定 |
|---|---|---|---|---|---|
| Gaussian Blur（4 变体） | ✘ | ✘ | ✔ `GaussianBlur.gdshaderinc:4` | ✘ | **不做**：必然产生调色板外颜色 |
| Convolution 3×3 | ✘ | ✘ | ✔ `ConvolutionMatrix.gdshaderinc:5` | ✘ | **不做** |
| Brightness/Contrast/Saturation/Tint | ✘ | ◐ `Lighten.js:55`（tinycolor ±3） | ✔ `BrightnessContrast.gdshaderinc:4` | ✘ | **不做**（改走调色板重映射） |
| HSV 调整 / Invert / Desaturate / Luma | ✘ | ✘ | ✔ 各自 shader | ✘ | **不做**（同上） |
| Color Curves（8 条 1D LUT） | ✘ | ✘ | ✔ `ColorCurves.gdshaderinc:3` | ✘ | **不做** |
| Gradient Map | ✘ | ✘ | ✔ `GradientMap.gdshaderinc:13` | **◐** `finish.rs` ramp 分带 | 已覆盖语义，不做 |
| Posterize | ✘ | ✘ | ✔ `Posterize.gdshaderinc:6` | **◐** ramp 分带 | 已覆盖语义 |
| Palettize（最近色） | ◐ gifenc | ✘ | ✔ `Palettize.gdshaderinc:6` | **✔** `map_to_palette`（更强：OKLab） | 已覆盖 |
| Index Map（R→X, G→Y 查映射图） | ✘ | ✘ | ✔ `IndexMap.gdshader:32` | ✘ | **P2-4**：纯索引、零新色，但用途窄 |
| 混合模式 22+ | ✘ | ◐ 仅 `globalAlpha` | ✔ `BlendLayers.gdshader:56`（sampler2DArray + 每层 metadata） | ✘ | **不做**（索引帧无 alpha 混合语义） |
| 自动反色（按屏幕背景亮度） | ✘ | ✘ | ✔ `AutoInvertColors.gdshader:7` | **◐** `crates/pxl-export/src/background.rs:70 auto_for`（OKLab 亮度差选灰） | 已覆盖语义 |

### 2.8 程序化生成

| 算法 | 来源 | 我方 |
|---|---|---|
| 模板符号掩码（0 空 / 1 半数实体 / 2 必现 / -1 预设暗色） | Sprite-Gen `Sprite.py:56 _generate_body` | **✔** `crates/pxl-gen/src/bollinger.rs:126 sample_body`（扩展到 6 种符号） |
| 水平镜像（中缝 2px 宽，原列复制两次） | `Sprite.py:71 _mirror_body` | **✔** `bollinger.rs:164 mirror_grid`（X/Y/XY，中心列复制） |
| 4 邻域膨胀描边 | `Sprite.py:80 _generate_edges` | **✔** `bollinger.rs:189 add_edges` |
| 逐行色相跳变（3×uniform 之和取绝对值 → 近似半正态） | `Sprite.py:108 _apply_colors` | **✔** `bollinger.rs:247 colorize`（保留 `biased-small`） |
| 纵向正弦光照（`sin(u/H·π)`） | `Sprite.py:127` | **✔** `bollinger.rs`（`SIN_HALF_Q8` 整数 LUT） |
| 横向渐变分支 | **死代码**（`Sprite.py:100-105` 两分支完全相同，移植时丢失转置） | **✔ 已修复**（保留 vertical/horizontal 两分支差异） |
| 可复现随机 | **✘ 无 seed**（全仓 grep `seed` = 0） | **✔** `crates/pxl-core/src/prng.rs`（SplitMix64） |
| 透明背景 | **✘** 白色不透明（`Image.new('RGB', …)`） | **✔** 索引 0 = 透明 |
| 周期 Value noise（可平铺） | ✘ | **✔** `crates/pxl-gen/src/noise.rs:173 value_noise`（`rem_euclid(period)` 保证无缝） |
| 矿脉成簇 / 接缝强制对齐 | ✘ | **✔** `noise.rs:88 paint_ore_veins` / `:140 enforce_edges` |
| Perlin / Simplex / Worley / FBM / domain warp | ✘ | ✘（Pixelorama 用引擎 FastNoiseLite，非自研）→ 不做 |
| 形状程序（Bresenham / 粗线 / 椭圆 / 多边形 / 图案） | ✘ | **✔** `shapes.rs:350/282/189/225/316` |
| ASCII 布局编译（legend → cell 展开 → 锚点摆放） | ✘ | **✔** `crates/pxl-gen/src/ascii/mod.rs:103 compile_ascii` |

> **结论**：Sprite-Generator 已被 **100% 覆盖且质量更高**。唯一可借鉴的是 notebook 里 4 组"参数配方"（龙 `saturation=0.2`、robot `colored=True + mirror`、spaceship `color_variations=0.9`）——可作为 `pxl-gen` 的 preset 候选，但价值低。

### 2.9 光栅化 / SVG

| 算法 | 来源 | 我方 |
|---|---|---|
| 扫描线超采样填充（非零/偶奇，采样点 `(i+0.5)/N`） | ✘（Pixelorama 反向：`image_to_svg` 逐像素 `<rect>`，**不是矢量化**） | **✔** `crates/pxl-svg/src/raster.rs:66 fill` |
| 多数投票降采样（非覆盖混合） | ✘ | **✔** `raster.rs:236 resolve`（恰好半数按右/下规则，同票取后画者） |
| 描边展开（分段四边形 + 圆盘/斜接/斜切接头） | ✘ | **✔** `geom.rs:563 stroke` |
| 曲线展平 | ✘ | **◐** `geom.rs:230 cubic_to`（按弦长定段数 2..64，**非**平坦度自适应）→ P2-8 |
| SVG 弧 → 三次贝塞尔 | ✘ | **✔** `geom.rs:265 arc_to` |
| 渐变分带（OKLab 插值 → 逐带 snap） | ✘ | **✔** `palette_map.rs:188 bands`（默认 4 带） |
| 调色板映射三档（strict/nearest/extend） | ✘ | **✔** `palette_map.rs:111 index_of`（strict 报错时给最近 3 色） |
| 细节保护（采样不足强制落 1px + protect 标记） | ✘ | **✔** `scene.rs:900 render_frame` / `:968` |

### 2.10 编码 / 导出 / 打包

| 算法 | pixel-scaler | piskel | Pixelorama | 我方 |
|---|---|---|---|---|
| GIF 编码 | ✔ `gifenc`（`rgba4444`，gifenc 内部量化） | ✔ `gif.js`（NeuQuant，5 workers） | ✔ `gdgifexporter`（median-cut + **手写 LZW**） | **✔** `crates/pxl-export/src/gif.rs:29 encode_gif_timed`（全局调色板 2^n + 每帧全帧写入 + `Background` disposal） |
| **GIF 帧差压缩** | **✔** `gif.ts:114 createDiffFrame`（RGB 距离平方 **≤16** 视为未变 → 置透明） | ✘ | ✘ | **✘** → P1-1 |
| GIF 透明索引固定到 0 | **✔** `gif.ts:239`（规避 dispose=2 背景色问题） | ◐ 未用色查找 `ColorUtils.js:5` | ✘ | **✘** → P1-1 |
| GIF alpha 二值化 | ✔ 阈值 128 | ✔ `FrameUtils.js:184` `round(a/255)*255` | ✘ | **◐** 索引帧天然只有 0/255 |
| GIF disposal 合成（解码方向） | ✔ `gif.ts:31`（type 2/3） | ✔ `libgif.js:559` | ✘ | **✘**（我们不读 GIF）→ P1-2 附带 |
| GIF 帧延迟防漂移（按累计边界四舍五入） | ✘ | ✘ | ✘ | **✔** `gif.rs:19 gif_delays_cs`（12fps → [8,9,8]） |
| APNG | ✘ | ✘ | **✔** `addons/aimg_io/apng_exporter.gd`（手写 acTL/fcTL/fdAT + CRC32） | **✘** → P2-3 |
| Aseprite 二进制写出 | ✘ | ✘ | **✔** `AsepriteExporter.gd:14` | ✘（不做：已有 JSON 生态） |
| 视频（ffmpeg） | ✘ | ✘ | ✔ `Export.gd:712` | ✘ 不做 |
| Sheet 打包：固定网格 | ✘ | ✔ `FramesheetRenderer.js:16` | **✔** `Export.gd:219`（ROWS / COLUMNS / **TAGS_BY_ROW / TAGS_BY_COLUMN**） | **✔** `sheet.rs:31 pack_sheet`（rows-by-anim / grid(≈√n) + pad + 行高对齐 + 2048 上限） |
| 图集列数最优化（`round(√(n/ratio))`） | ✘ | **✔** `PngExportController.js:115` | ✘ | **◐** `pack_sheet` 的 grid 用 `chunks(√n)` |
| **Sheet 反向切分** | ✘ | ◐ 固定网格 + `toDataURL()` 判空 `CanvasUtils.js:55` | **✔** `addons/SmartSlicer/Classes/RegionUnpacker.gd:64`（种子像素向右下扩张矩形 + `fill_rect` 占位 + 空行跳跃 + 矩形合并 + 多线程） | **✘** → P1-2 |
| Atlas JSON | ✘ | ✔ Pixi/TexPack `PngExportController.js:200` | ✘ | **✔** `sheet.rs:141 sheet_json`（Aseprite frames/meta/frameTags，相邻同 tag 合并） |

### 2.11 质量分析 QA（其余四家全部为零）

| 指标 | 我方位置 |
|---|---|
| 调色板越界像素 / 可见像素 / 覆盖率 bps / bbox | `crates/pxl-qa/src/pixels.rs:14/118/126/141` |
| 4-连通域 / 孤立单像素 / 内孔 / 最大连通域占比 | `pixels.rs:169` → `:269 topology` |
| 8 邻域同色单点（color specks） | `pixels.rs:65` |
| 棋盘噪点占比 bps（2×2 交替） | `pixels.rs:106/208` |
| 相邻色对比不足计数 | `pixels.rs:90` |
| 描边缺口（暴露的非 outline 实体像素） | `pixels.rs:81` |
| 帧数 / 尺寸一致 / 相邻帧 diff + 3×3 平移搜索残差 | `animation.rs:60` |
| 帧面积变化 bps / 质心位移（millipixel） / 调色板漂移（ramp 归一化直方图 L1/2） | `animation.rs:135/151/166` |
| 统一色面积占比 / 2×2 块角数 / **copy2x_bps（放大指纹）** | `resolution.rs:15/48/56/63` |
| 瓦片满血边 / 接缝（首末列配对：alpha → 同色 → ramp 距离 ≤N → 通道差 ≤32） | `tile.rs:7/37` |
| 剪影轮廓 run-length 不规则度（jaggy） | `crates/pxl-art/src/metrics.rs:171/203` |

**可补的 QA 项**（来自本次对比）：
- **抖动强度检测**：识别 4×4 Bayer 周期性并把"抖动占比 > X%"报 Warn。现有 `pixels.rs:106` 只识别 2×2 棋盘。
- **AA 残留检测**：当输入来自 PNG 时，统计"仅出现在边缘、且是两邻色中点"的颜色数。索引帧内无意义，但 `pixelize` 之后有意义 → 配合 P2-9 的 RGBA 边界。
- **未使用调色板项占比**：`palette.validate` 只校验结构，不报"8 色里只用了 3 色"。

---

## 3. 交叉对比矩阵（关键算法只看覆盖率）

| 能力域 | pixel-scaler | piskel | Pixelorama | Sprite-Gen | **PixelGames** |
|---|---|---|---|---|---|
| 边缘导向放大 | 1（xBR 外部） | 0 | 3（Scale3x/OmniScale/cleanEdge） | 0 | **6**（EPX/Scale3x/xBR-index/MMPX/contour/hybrid） |
| 旋转质量 | 0 | 1 | **7** | 0 | **2**（RotSprite/Nearest） |
| 色彩量化 | 1（库） | 1（库） | 2（median-cut/uniform） | 0 | **2**（OKLab median-cut + k-means） |
| 调色板管理 | 0 | 2 | **5** | 0 | **3**（色阶/位移/校验） |
| 抖动 | 0 | 1 | **3** | 0 | **2**（Bayer4 三处复用） |
| 描边 | 0 | 0 | 1 | 1（膨胀式） | **5**（内外×3 笔刷 + strip + selout + 断口 + 保护掩码） |
| 选区/填充 | 0 | **3** | **5** | 0 | **0** |
| 几何/网格 | 0 | 3 | **6** | 1 | **4** |
| 滤镜（RGBA） | 0 | 1 | **19** | 0 | 0（刻意） |
| 生成/PCG | 0 | 0 | 0 | **4** | **6**（Bollinger 全覆盖 + 噪声 + ASCII 编译） |
| 光栅化 | 0 | 0 | 0 | 0 | **7**（唯一） |
| 编码/打包 | 2（GIF 帧差） | 2 | **5**（GIF/APNG/Aseprite/ffmpeg/sheet） | 0 | **3**（PNG/GIF/sheet+atlas） |
| QA | 0 | 0 | 0 | 0 | **12** |

**读法**：绿色不均衡（任何单列极值）就是缺口。我们唯一的 0 是"选区/填充"与"连续值滤镜"，前者必须补，后者刻意不做。

---

## 4. 差距诊断（按性质分五类，避免误判）

### 4.1 真实缺口（必须补，共 6 项）

| # | 缺口 | 证据 | 为什么必须补 |
|---|---|---|---|
| G1 | 无像素级选区与 flood fill | `crates/pxl-ops/src/apply/region.rs` 52 行只有 `Region` 矩形 + `resolve_region`；`pxl-format::Region` 无像素语义 | 一切"局部编辑"的前提。没有选区就没有"只清理角色不清理背景"这类操作，`cleanup/finish/outline` 只能全图跑 |
| G2 | 旋转缺后处理 | `rotsprite.rs:59` 只有 EPX×3 + 最近邻，无 cleanEdge/补洞/Rotxel 投票 | 45° 与 30° 旋转后斜线出现 1px 锯齿与洞，是像素画最刺眼的缺陷 |
| G3 | 抖动矩阵只有 4×4 | `quantize.rs:17 BAYER4` 是唯一矩阵 | 大色域渐变（天空、金属）里 4×4 会产生明显方块；8×8/蓝噪声是行业标准配置 |
| G4 | 调色板无排序 | `pxl-core/src/palette.rs` 只有 `validate` / `shift_in_ramp` / `cycle_map` | 调色板顺序直接决定 UI 可读性与 ramp 推导正确性；`build_shades` 的输出顺序目前不受控 |
| G5 | GIF 无帧差 | `pxl-export/src/gif.rs` 每帧全帧写入 | 8 帧 32×32 动画里 80% 像素静止，帧差可省 60–80% 体积，且 pixel-scaler 已证明实现只需 ~40 行 |
| G6 | 图集不能反向切分 | `pxl-export/src/sheet.rs` 只有 `pack_sheet` / `sheet_json` | 无法导入现成素材（Aseprite/TexturePacker 的产物），阻塞"外部素材 → 我们的管线"这条链路 |

### 4.2 名似实异（名字相同、语义不同，不能直接对照）

| 概念 | 第三方做法 | 我方做法 | 判断 |
|---|---|---|---|
| "xBR" | `xbr-js` 真 xBR：y/w/y2/w2 加权**混合**，产生中间色 | `upscale.rs:207` 索引变体：角规则同 EPX，"同色"放宽为 OKLab 亮度差阈值，**纯复制** | 我们的 xbr 更接近 "EPX + 感知容差"。**命名需在文档标注为 `xbr-index`**，避免使用者误期待混合效果 |
| "RotSprite" | 经典：hq 系放大 + 三次斜切 + 采样 | `rotsprite.rs:59`：EPX×3 = 8× + 反向 1/8 定点最近邻 | 保形弱于原版，但**绝不产生新色**。属合理取舍，需在 doc 注释写明 |
| "容差" | Pixelorama `similar_colors`：逐通道 **∞-范数**（tol≈100/255） | 我方全局用 OKLab **欧氏平方** | 两者都要保留：OKLab 用于量化/映射（更符合感知），∞-范数用于**用户滑杆**（可解释、可复现、与 Aseprite 行为一致） |
| "多数投票降采样" | 常规：覆盖率加权得到 AA 边缘 | `raster.rs:236 resolve`：>半数才保留，恰好半数按覆盖半区在右/下决定 | 这是**刻意为像素画设计**的规则，不是 AA。文档必须强调 |
| "pixelize" | Pixelorama：UV 量化后最近邻采样（纯视觉缩小） | `pixelize.rs:435`：网格检测 → 裁齐 → 描边加粗 → 4 种降采样 → 量化 | 我们的 pixelize 是**栅格→像素画**的完整重建管线，比 "pixel_size 缩小" 强一个数量级 |

### 4.3 我方更强（不要为了"对齐"而退化）

| 能力 | 证据 |
|---|---|
| SVG 光栅化 | 扫描线 + N×N 超采样 + 多数投票 + 描边展开 + 渐变分带 + 三档调色板映射 + 细节保护。**四家全无**。Pixelorama 的 `image_to_svg` 是逐像素 `<rect>`，不是矢量化 |
| 描边体系 | selout / selout_lit 按邻近色阶取色，Pixelorama 只有平色描边 |
| HTML/索引 PNG | `png.rs:7 encode_indexed_png` PLTE+tRNS + NoFilter + 无时间戳（比特级可复现），四家都用 RGBA PNG |
| 确定性 | 全整数/定点（OKLab Q16、`trig.rs` LUT、`prng` SplitMix64），跨平台位一致。pixel-scaler/piskel 依赖浏览器 canvas 实现 |
| QA | 12 项指标 + bps 阈值 + tier 分级。四家零 QA |
| 放大算法宽度 | 6 种（含自研 contour）。pixel-scaler 1 种、Pixelorama 3 种 |

### 4.4 原理性冲突（明确不做，避免后续被反复提议）

| 类别 | 具体项 | 不做的硬理由 |
|---|---|---|
| 混合型放大 | HQ2x/3x/4x、Eagle/SuperEagle、2xSaI/Super2xSaI、OmniScale、SuperXBR、xBRZ | 输出含调色板外颜色。若接一次 `map_to_palette`，"先污染再净化"的净效果不如直接 MMPX/contour。MMPX 已覆盖它们的核心场景（线条、文字、2:1 斜坡） |
| 连续值放大 | 双线性、双三次、Lanczos | 同上，且必然模糊边缘 |
| 连续值滤镜 | GaussianBlur、ConvolutionMatrix、BrightnessContrast、HSV、ColorCurves、Luma/Desaturate/Invert | 中间结果无法用索引表达；等价语义已由 ramp 分带 + `palette_swap` + `material` 覆盖 |
| 混合模式 | BlendLayers 22+（Multiply/Screen/Overlay/Hue/Sat/…） | 索引帧无 alpha 混合语义。若将来做图层，只支持 Normal/Erase |
| 误差扩散抖动 | Floyd–Steinberg、JJN、Stucki、Atkinson、Riemersma | 产生大量随机孤立像素，直接触发我们自己 `pxl-qa` 的 specks / 棋盘噪点告警；有序抖动才是像素画惯例 |
| 其他量化器 | NeuQuant、Octree、Wu | 我们的 median-cut + OKLab 加权 + k-means 精修在**小样本、低色数**场景（我们的全部场景）优于它们 |
| 噪声族 | Perlin、Simplex、Worley/Voronoi、FBM、domain warp | 我们需要的是**可平铺**纹理，周期 Value noise + `enforce_edges` 正为此设计；FBM 会产生非整周期 |
| 导出格式 | MP4/AVI/WebM（ffmpeg）、Aseprite 二进制 | 超出工具链边界 |

### 4.5 结构性差异（需要架构决策，不是单点补丁）

| 差异 | 现状 | 建议 |
|---|---|---|
| **无 RGBA 中间态** | 全链路 `IndexFrame`（u8 index + Palette） | 新增**仅存在于边界**的 RGBA 缓冲：`import_png → Rgba8 → pixelize/quantize → IndexFrame`；`IndexFrame → Rgba8 → png/gif`。内部 API 不得接收 RGBA。收益：可做边缘检测、AA 残留判定、帧差、任意输入源。风险：要防止 RGBA 泄漏进 `pxl-art` 的算法签名 |
| **无图层** | 单帧单层 | 不引入。多图层会破坏"索引帧 + 全局调色板"的简单性；`pixelize` 的洋葱皮/参考图需求由 QA 与预览解决 |
| **无交互状态** | 引擎无状态、无撤销 | 保持。选区以**数据**形式（`Mask`）进入 op，不做工具会话 |
| **裁剪 op 缺失** | QA 有 bbox，无 crop | 低风险补齐：`crop` op（按 bbox / 按区域 / 按板）→ P2-5 |

---

## 5. 实施计划

排序原则：**先补"索引安全 + 阻塞其他能力"的**，再补"体积/IO"，最后补"锦上添花"。
每项都必须是整数/定点、确定性、只输出索引、有单测。

### P0（第一批，4 项，解锁局部编辑与旋转质量）

#### P0-1 选区与扫描线填充（G1，最高优先）

**新增** `crates/pxl-core/src/select.rs`（只依赖 `IndexFrame` + `Palette`，无 IO）：

```rust
pub enum Connectivity { Four, Eight }

pub enum Tolerance {
    Exact,
    /// Per-channel ∞-norm, matching Aseprite / Pixelorama slider semantics.
    Channel(u8),
    /// OKLab squared distance (Q16), same metric as `color::dist2`.
    Oklab(u32),
}

pub struct SelectOptions {
    pub seed: (u16, u16),
    pub connectivity: Connectivity,
    pub tolerance: Tolerance,
    /// Restrict the scan to a rect, like Allegro's `_area_start_idx`.
    pub bounds: Option<[i32; 4]>,
}

/// 1 bit per pixel, row-major, w*h bits.
pub struct Mask { /* w, h, bits: Vec<u64> */ }

pub fn flood_select(frame: &IndexFrame, palette: &Palette, opts: &SelectOptions) -> Mask;
pub fn magic_wand(frame: &IndexFrame, palette: &Palette, opts: &SelectOptions) -> Mask;
pub fn rect_mask(w: usize, h: usize, rect: [i32; 4]) -> Mask;
pub fn ellipse_mask(w: usize, h: usize, rect: [i32; 4], filled: bool) -> Mask;
pub fn lasso_mask(w: usize, h: usize, points: &[(i32, i32)]) -> Mask;
pub fn combine(a: &Mask, b: &Mask, op: BoolOp) -> Mask;   // Add | Subtract | Intersect
pub fn morph(mask: &Mask, radius: u8, brush: Brush, op: MorphOp) -> Mask; // Dilate | Erode | Border
```

**算法要求（对齐 Allegro `flood.c`，不是逐像素 BFS）**：

1. 按行维护 `(x1, x2, y, dy)` 段栈；弹出后吞掉整行连续段；
2. 只检查上/下行的**未覆盖**列区间，避免重复进栈；
3. 复杂度接近 O(像素数)，对 128×128 上限来说无性能压力，但保证不会因递归爆栈；
4. 容差判定复用 `color::dist2`（OKLab），`Tolerance::Channel` 走 `max(|Δr|,|Δg|,|Δb|) <= tol`；
5. 透明像素（index 0）单独成域，不参与颜色距离。

**接线**：
- `pxl-format::Region` 扩展为 `Region::Rect | Region::Mask { rle }`（RLE 编码，保持 JSON 可读），而不是新增并行字段；
- `pxl-ops` 所有 op 的 `region` 字段自动支持 Mask（现有 `resolve_region` 返回 `[i32;4]`，改为 `Selection::Rect | Selection::Mask`）；
- 新增 op：`select_fill`（用某色/某 ramp 步填充选区）、`select_clear`。

**验收**：
- 单测：棋盘 8×8 中 seed 在左上、`Connectivity::Four` 得到 32 像素；`Eight` 得到 32；对角连通的两块在 `Eight` 下合并；
- 容差：`Channel(0)` 严格等于同索引；`Channel(16)` 在 8 级灰阶上合并相邻 1 级；
- 性能：128×128 全图 flood 在 release 下 < 1ms（用 `cargo bench` 或 `#[test]` 内计时）；
- 黄金：`harness` 增加 `select.flood.checker` 用例。

#### P0-2 旋转质量三件套（G2）

**(a) cleanEdge 后处理**（新增 `crates/pxl-art/src/rotate_clean.rs`）

参考 `Pixelorama/src/Shaders/Effects/Rotation/cleanEdge.gdshader`（torcado, MIT）的判定：旋转后在"未知区"沿 down-forward 方向找最近的不透明像素补色，并把相邻的 1px 阶梯压平成 2:1 斜率。

索引安全实现（不产生新色）：
```
for each transparent pixel adjacent to opaque:
    look for the opaque neighbour along (down, forward) within 2 px
    if found and the resulting edge would create a 1:1 stair:
        copy that neighbour's index
```
要求：**只在旋转路径上启用**（`rotate_any` 的 `rotsprite` 分支），全图清理另开 `clean_edges` op。

**(b) RotSprite 补洞（smear）**

参考 `SmearRotxel.gdshader:29`：从 `initial_angle` 到目标角度以 ~0.0174 rad（1°）为步长试采样，直到取到不透明像素。我们的版本用**整数度数扫描**：
```
for hole pixel:
    for delta in 1..=max_scan (say 8):
        try deg ± delta (先正后负)
        if opaque: take it; break
```
需要保证 `rotsprite` 的角度约定（顺时针、整度）不变。

**(c) Rotxel（3× 子格 9 邻居投票）**

新增 `crates/pxl-art/src/rotxel.rs`，参考 `Pixelorama/src/Autoload/DrawingAlgos.gd:528`：
1. 目标坐标 ×3，对 9 个子像素做反向旋转映射；
2. 按子格 `index` 落在 3×3 的哪个位置，取 9 邻居中"相似度最高"的那个索引（相似度 = 同索引 或 同 ramp 且步差 ≤1）；
3. 90°/180°/270° 特判走 `rotate90`，避免无意义计算。

**验收**：
- 24×24 角色分别旋转 15°/30°/45°，三种算法各出一张，人工对照 + `metrics::jaggy_irregular` 单调下降；
- 断言：输出调色板 ⊆ 输入调色板（逐像素 `index` 必须存在于输入集合）；
- 断言：不产生洞（对不透明区域的旋转包围盒内，透明像素数不高于 `nearest` 的 1.05 倍）。

#### P0-3 抖动矩阵扩展（G3）

`crates/pxl-art/src/quantize.rs`：
- 把 `BAYER4` 改为 `BAYER2 / BAYER4 / BAYER8 / BAYER16` 四张常量表（Bayer 递归公式生成，写成 `const fn` 或直接常量，便于审计）；
- 新增 64×64 **蓝噪声掩膜**（void-and-cluster 离线生成一次，作为 `const` 字节数组提交，不进构建期随机）；
- `pick(...)` 增加 `matrix: DitherMatrix` 参数；`map_to_palette` 的 `dither: u8` 语义保持（0..100 强度），矩阵通过新参数或 `DitherMatrix::from_str` 选择；
- `finish.rs:419 ramp_dither` 同步支持矩阵选择。

**验收**：
- 4 张 Bayer 表满足 `Bayer(2n)[y][x] = 4*Bayer(n)[y%n][x%n] + [[0,2],[3,1]][y/n][x/n]`；
- 蓝噪声掩膜的 FFT 低频能量低于 Bayer8（用简单方差代理即可，写进测试注释）；
- 渐变 16 级 → 4 色的视觉对比图（`label.rs` 的 contact_sheet）加入 `examples/`。

#### P0-4 调色板排序（G4）

`crates/pxl-core/src/palette.rs` 新增：

```rust
pub enum SortKey { Index, Hue, Saturation, Value, Lightness, Luma, Alpha }

pub fn sort(&mut self, key: SortKey, reverse: bool);
/// Group by ramp first, then sort inside each ramp by lightness.
pub fn sort_by_ramp(&mut self);
```

- `Lightness` 用 OKLab L（我们已有），不用 HSL L（Pixelorama 特意移植了 OKHSL toe 曲线，我们直接用 OKLab L 更简单且更正确）；
- **透明索引 0 必须固定在位置 0**，排序只作用于 1..n（否则整个索引语义崩塌）；
- `sort` 必须重写所有引用该调色板的帧的索引（新增 `PaletteRemap: Vec<u8>`，由 `sort` 返回，调用方决定是否应用）。

**验收**：
- 排序后 `validate` 仍通过；
- `sort` + 应用 remap 后，帧的**视觉像素完全不变**（逐像素 RGBA 对比）；
- 排序幂等（连续两次同 key 排序结果一致）。

---

### P1（第二批，5 项，体积与素材进出）

#### P1-1 GIF 帧差 + 透明索引固定

`crates/pxl-export/src/gif.rs`：
- 新增 `diff_frames(prev, cur, palette, threshold_sq: u32) -> Option<Vec<u8>>`：逐像素 RGB 距离平方 ≤ 阈值 → 写透明索引；全帧无变化返回 `None`（整帧跳过）；
- 默认 `threshold_sq = 16`（沿用 pixel-scaler 的经验值），可配 0..64；
- 透明索引用**调色板里索引 0**，并要求 `disposal = Background`（我们已是）；
- 首帧仍写全帧（GIF 规范要求）；
- 新增 `--gif-diff` CLI 开关，默认 **on**（我们是确定性管线，退化风险低），并加 `--gif-diff-threshold`。

**验收**：
- 8 帧 32×32 角色行走动画：输出体积下降 ≥40%；
- 解码回放逐帧与未压缩版本**像素完全一致**（用 `decode_png` 无法验 GIF，改为在测试里自建最小 GIF LZW 解码或对比 `gif` crate 的 decoder 输出）；
- 阈值 0 时与"逐帧全写"字节完全一致。

#### P1-2 图集切分（G6）

**新增** `crates/pxl-export/src/slice.rs`：

```rust
pub enum SliceMode {
    /// Fixed grid: cell size + offset, skip empty cells.
    Grid { cell: (u32, u32), offset: (u32, u32), skip_empty: bool },
    /// Auto: seed-based rectangle expansion (SmartSlicer style).
    Auto { min_size: (u32, u32), merge_distance: u32 },
    /// Read frame rects from an atlas JSON.
    Atlas { json: String },
}

pub fn slice(img: &Rgba8, mode: &SliceMode) -> Vec<[u32; 4]>;
```

`Auto` 算法（对齐 `RegionUnpacker.gd:64` 的思路，但**不用 flood fill**，因为我们不需要连通性，只需要"不相交的最大矩形"）：
1. 扫描找到第一个非透明像素作为种子；
2. 从种子向右扩张到透明列，向下扩张到透明行（`_estimate_rect`）；
3. 试收紧缩小到实际内容 bbox；
4. 用 `fill_rect(rect, occupied)` 占位，继续扫描下一个未占位非透明像素；
5. 相邻矩形若间距 ≤ `merge_distance` 则合并（处理角色分部件）；
6. 支持 `min_size` 过滤噪点。

**验收**：
- 对 `pack_sheet` 的产物做 `Auto` 切分，得到与输入**完全一致**的 frame rect 列表（round-trip 测试）；
- 对 Aseprite 导出的 atlas JSON（`sheet_json` 产物）做 `Atlas` 切分，rect 一致；
- 固定网格 + `skip_empty`：空帧被跳过，非空帧全保留。

#### P1-3 通用形态学算子

新增 `crates/pxl-core/src/morph.rs`（纯索引，不产生新色）：

```rust
pub enum MorphOp { Dilate, Erode, Open, Close, Border }

/// Values are treated as opaque/non-opaque for shape, then the *nearest*
/// original index is written (so a dilate of a 3-colour blob keeps its 3 colours).
pub fn morph_palette_aware(frame: &IndexFrame, radius: u8, brush: Brush, op: MorphOp) -> IndexFrame;
```

关键语义：**膨胀写入的是"最近源像素的索引"**，不是新色。这解决了"描边宽度只能 1..4"的限制（描边 = dilate 后按 `ring_color` 上色）。

**验收**：
- `Erode(Dilate(x))` 与 `Open(x)` 一致；
- 半径 1..8 的描边与 `paint_outline` 在 radius ≤4 时**逐像素一致**（回归保护）；
- 不变量：输出索引 ⊆ 输入索引。

#### P1-4 DropShadow + 纹理图案填充

- `crates/pxl-art/src/shadow.rs`：`drop_shadow(frame, offset: (i8,i8), color_index: u8, opacity_steps: u8)`。索引安全：阴影是纯色剪影，`opacity_steps` 用 Bayer 抖动模拟半透明（例如 50% → 棋盘）。
- `shapes.rs:316 pattern_on` 扩展：`Pattern::Texture { tile: &IndexFrame, offset, interpolation: Nearest }`，对齐 `PatternFill.gdshader:14` 的 UV 平铺语义（但只做 nearest，不做 mix）。

**验收**：偏移 ±(1..8) 全组合无越界；阴影不覆盖已有实体像素（仅写透明处）。

#### P1-5 级联放大组合器（任意 k）

`crates/pxl-art/src/upscale.rs` 增加：

```rust
/// Compose integer steps so that product == k, e.g. 6 => [2,3].
/// Prefers larger, quality-first steps: 8 => [2,2,2] (mmpx) before [4,2].
pub fn plan_steps(mode: Mode, k: u16) -> Option<Vec<u16>>;
pub fn upscale_any(frame: &IndexFrame, palette: &Palette, mode: Mode, k: u16) -> PxlResult<IndexFrame>;
```

规则：`k` 分解为 2/3/4 的因子组合（优先 4→3→2），每步调用对应 `supports` 的实现；`k=1` 直通；`k` 含 >4 的素因子（如 5、7）→ 返回 `None` 并提示"用 pixelize 的任意尺寸路径"。

**验收**：`k=2..=16` 全部 24 种分解可复现；`k=6` 输出尺寸精确 6×；幂等性不适用（放大不幂等），但需断言**调色板不扩张**。

---

### P2（第三批，按需）

| # | 项 | 落点 | 说明 |
|---|---|---|---|
| P2-1 | 等距/六边形网格生成 | `crates/pxl-gen/src/shapes.rs` 新增 `iso_tile(w,h,step)` / `hex_tile(radius)` | 参考 `DrawingAlgos.gd:767/845`，用 Bresenham 画 2:1 菱形边再镜像填行；用于 tile 集生产 |
| P2-2 | CornerPin 四点自由变换 | `crates/pxl-svg/src/geom.rs` 复用 `Affine` + 新增 `QuadWarp` | 单位方格双线性反解（`CornerPin.gdshaderinc:16`）。用途：修正扫描/截图素材的透视 |
| P2-3 | APNG 编码 | 新增 `crates/pxl-export/src/apng.rs` | 手写 IHDR + acTL + fcTL/IDAT/fdAT + CRC32（参考 `aimg_io`，Unlicense）。我们已有 `png.rs` 的 chunk 能力，成本低 |
| P2-4 | IndexMap | `crates/pxl-ops/src/apply/color.rs` | R→X / G→Y 两通道查表重映射（`IndexMap.gdshader:32`）。纯索引、零新色，适合做"程序化材质重映射" |
| P2-5 | crop op | `crates/pxl-ops` + `pxl-format::Op` | 按 bbox / 区域 / 板裁剪并同步画布尺寸；复用 `pxl-qa/src/pixels.rs:141` 的 bbox |
| P2-6 | 均匀步长直线 + 椭圆对称修正 | `crates/pxl-gen/src/shapes.rs` | piskel `getUniformLinePixels`（`PixelUtils.js:295`）与 `Circle.getCirclePixels_`（极坐标 + 4 象限）；我们现有 Bresenham 斜率不均、椭圆依赖隐式方程 |
| P2-7 | 调色板文件 IO（GPL/PAL/HEX） | `crates/pxl-format` | piskel 有三种 reader；价值低（我们以 JSON 为主），但便于导入社区色板 |
| P2-8 | 曲线展平改为平坦度自适应 | `crates/pxl-svg/src/geom.rs:230 cubic_to` | 现按弦长估段数 2..64，改为递归平坦度（ε=1/4 px），可减少大曲线段的锯齿 |
| P2-9 | 用户给定 cell 的网格校验 | `crates/pxl-art/src/pixelize.rs` | pixel-scaler `xBR.ts:180 validateImageSize` 思路：`w % cell == 0`，否则提示最近可用 cell；补充 `detect_grid` 的显式输入路径 |

### P3（观察项，不排期）

- **RGBA 边界缓冲**（§4.5）：需要一次设计评审。若实施，`pxl-core` 新增 `Rgba8 { w, h, px: Vec<[u8;4]> }`，且**只允许出现在 `pxl-export` / `pxl-cli::import` / `pxl-art::pixelize` 的入参**。
- 对角轴对称（`x=y` / `x=-y`）：Pixelorama 用 `Vector2.reflect` + ±0.5 偏置。我们只有轴对齐镜像；若做 iso 素材，对角对称有用。
- QA 的"抖动强度检测""未使用调色板项占比"。

---

## 6. 验收标准（整批）

1. `cargo test --workspace --no-fail-fast` 全绿；新增单测 ≥ 每项 4 条（1 正例、1 反例、1 边界、1 不变量）。
2. **不变量（每项都断言）**：输出调色板 ⊆ 输入调色板；索引 0 恒为透明；`MAX_FRAME_PIXELS` 与 `MAX_DIM` 不被绕过。
3. 确定性：同一输入跑两次字节一致；跨 `x86_64` / `aarch64` 一致（全整数，无浮点）。
4. 黄金回归：`harness` 的 `golden/hashes.json` 为新增 op 添加条目；不删除既有条目。
5. 文档：`docs/` 下补一份 `algorithms/upscale-rotate.md`，明确 §4.2 的五个"名似实异"。
6. 视觉验收：`examples/` 新增 `thirdparty-bench/` 目录，每个新算法一张 `@4×` 对比图（用 `label.rs` 的 contact_sheet 排版，标注算法名与参数）。

## 7. 不做清单（写入 `docs/algorithms/`，防止反复提议）

- **放大**：HQ2x/3x/4x、Eagle/SuperEagle、2xSaI/Super2xSaI、OmniScale、SuperXBR、xBRZ、双线性/双三次/Lanczos
- **滤镜**：GaussianBlur、ConvolutionMatrix、BrightnessContrast、HSV、ColorCurves、Luma/Desaturate/Invert
- **混合**：BlendLayers 22+ 模式（图层仅 Normal/Erase）
- **抖动**：Floyd–Steinberg / JJN / Stucki / Atkinson / Riemersma（误差扩散）
- **量化**：NeuQuant、Octree、Wu
- **噪声**：Perlin、Simplex、Worley/Voronoi、FBM、domain warp
- **IO**：MP4/AVI/WebM(ffmpeg)、Aseprite 二进制写出、WebP 动画
- **其他**：位图 auto-trace 矢量化、骨骼/蒙皮动画、帧间补间、撤销/历史栈

> 保留的重新评估触发条件：若将来引入 RGBA 工作缓冲并接受"输出必须再量化一次"，则 HQx / xBRZ / GaussianBlur 可重新进入候选——但需要在 §0 第 5 条架构决策里显式批准。

---

## 8. 许可与复用边界

`thirdparty/` 是只读参考。本计划的 P0–P2 全部要求**独立实现**，只借鉴公开论文与算法描述。

| 项目 | 许可证 | 允许的复用方式 |
|---|---|---|
| pixel-scaler | MIT（© 2025 irokaru） | 可参考实现细节；若直接搬代码需保留版权声明。建议只借鉴 **GIF 帧差阈值（16）与透明索引固定** 这两个经验值 |
| piskel | **Apache-2.0** | 可参考；含专利授权条款。建议只借鉴 **均匀步长直线、ColorSorter 分桶参数、图集列数公式** 三个纯数学结论 |
| Pixelorama | MIT（© 2019-present Orama Interactive） | 可参考。其 shader 目录内**混合了第三方许可**：cleanEdge（torcado, MIT）、OmniScale（Lior Halphon / deakcor / Nobuyuki, MIT）、SmearRotxel（Azagaya，**未标许可**）、ConvolutionMatrix（csblo，**未标许可**）、IndexMap（Variable，**未标许可**）、BlendLayers 的 HSL 转换（unitycoder gist, MIT） |
| Sprite-Generator | **无 LICENSE，README 亦未声明** | **只可阅读，不可复制代码**。我们已有的 `crates/pxl-gen/src/bollinger.rs` 是独立实现，与本仓库无代码关系；后续不得以它为依据搬运 |

由此得出三条硬约束：

1. **P0-2 的 cleanEdge / Rotxel 必须按算法描述重写**（判定规则 + 投票语义），不得逐行翻译 GDScript 或 GLSL。提交信息注明"算法参考自公开描述"。
2. **未标许可的 shader（SmearRotxel / ConvolutionMatrix / IndexMap）只作为行为观察对象**，不进入实现依据清单。P0-2(b) 的补洞与 P2-4 的 IndexMap 都按本计划给出的规则重写。
3. `Sprite-Generator` 的任何代码不进入 `crates/`；§2.8 的"已覆盖"结论已经封住了这个需求。

## 9. 附录：证据索引

| 主题 | 关键坐标 |
|---|---|
| pixel-scaler xBR 包装与级联 | `thirdparty/pixel-scaler/src/core/algorithm/xBR.ts:8,54,76,92,127,149,180` |
| pixel-scaler Canvas 重采样 | `.../src/core/utils/imageUtils.ts:40-64` |
| pixel-scaler GIF 帧差/透明/量化 | `.../src/core/utils/gif.ts:14,31,92,114,183,201,233,239,265` |
| pixel-scaler 参数 | `.../src/constants/form.ts:9-33` |
| piskel 帧模型 | `thirdparty/piskel/src/js/model/Frame.js:58-72`、`src/js/utils/core.js:172-205` |
| piskel 连通遍历/填充 | `.../src/js/utils/PixelUtils.js:43,78,118,186,256,295` |
| piskel 调色板排序 | `.../src/js/service/color/ColorSorter.js:19-78` |
| piskel 变换 | `.../src/js/tools/transform/TransformUtils.js:7,26,67,113`、`Crop.js:45`、`ResizeUtils.js:45,88` |
| piskel 工具几何 | `.../src/js/tools/drawing/Circle.js:34`、`DitheringTool.js:44`、`VerticalMirrorPen.js:22`、`Stroke.js:98` |
| piskel 图集切分 | `.../src/js/utils/CanvasUtils.js:55-102`、`FrameUtils.js:282` |
| Pixelorama 效果注册表 | `thirdparty/Pixelorama/src/UI/Timeline/LayerEffects/LayerEffectsSettings.gd:6-58` |
| Pixelorama 描边 shader | `.../src/Shaders/Effects/OutlineInline.gdshaderinc:12` |
| Pixelorama 旋转族 | `.../src/Autoload/DrawingAlgos.gd:329,528,715,732` + `src/Shaders/Effects/Rotation/*.gdshader` |
| Pixelorama 填充/选区 | `.../src/Classes/FloodFillObject.gd:29,44,89,157`、`Tools/SelectionTools/MagicWand.gd:51`、`Classes/SelectionMap.gd:210` |
| Pixelorama 量化 | `.../addons/gdgifexporter/quantization/median_cut.gd:32,111`、`uniform.gd` |
| Pixelorama 图集切分 | `.../addons/SmartSlicer/Classes/RegionUnpacker.gd:64` |
| Pixelorama 导出 | `.../src/Autoload/Export.gd:219,712,865,1046`、`addons/aimg_io/apng_exporter.gd:10` |
| Sprite-Generator 全算法 | `thirdparty/Sprite-Generator/SpriteGenerator/Sprite.py:8,56,71,80,100,108,127,134,143` |
| 我方放大 | `crates/pxl-art/src/upscale.rs:21,111,147,207,270`、`mmpx.rs:28,173`、`contour.rs:52` |
| 我方旋转 | `crates/pxl-art/src/rotsprite.rs:59`、`crates/pxl-ops/src/apply/transform.rs:99,144,234,276,291` |
| 我方量化/抖动 | `crates/pxl-art/src/quantize.rs:17,61,79,122,193,240` |
| 我方描边/清理 | `crates/pxl-core/src/outline.rs:12,18,47`、`crates/pxl-art/src/outline.rs:79,100,167`、`cleanup.rs:91,116,148,192,344` |
| 我方 pixelize | `crates/pxl-art/src/pixelize.rs:24,160,209,358,386,435` |
| 我方 SVG | `crates/pxl-svg/src/raster.rs:66,236`、`geom.rs:230,563`、`palette_map.rs:111,188`、`scene.rs:900` |
| 我方导出 | `crates/pxl-export/src/gif.rs:6,19,29`、`sheet.rs:31,141`、`png.rs:7,99,130` |
| 我方 QA | `crates/pxl-qa/src/pixels.rs:14,106,169,269`、`animation.rs:60,135,151,166`、`tile.rs:7,37`、`resolution.rs:15,63` |
| 我方生成 | `crates/pxl-gen/src/bollinger.rs:113,126,164,189,227`、`noise.rs:88,140,173`、`shapes.rs:114,189,225,282,316,350` |
