# pxl 出图质量分析（基于 PR #1 分支 `cursor/reference-art-recipes-874a`）

> **状态（2026-10-22）：已完成，归档。** R1–R10 根因分析为改进计划的输入；其中 R6（调色板）、R10（cleanEdge）等部分缺口已在后续版本修复，确定性缺陷复核见 pxl-quality-improvement-plan.md Q2。本文不再更新。

只读分析，没有改仓库。代码在 `/workspace/PixelGames`，上游参考仓库浅克隆在 `/tmp`。

## 1. 质量差的根本原因

| # | 原因 | 证据（文件/函数） | 影响的素材 |
|---|---|---|---|
| R1 | **几何原语只有矩形/圆/线/矩阵**，配方几乎全是轴对齐矩形拼贴，没有手调剪影 | `pxl-gen/src/shapes.rs::generate_shapes`；`examples/reference/shaded-crate/crate.shapes.json` 9 条 `rect`；木箱的板缝、钉子都是 1px 长条矩形 | shaded-crate(3)、orchard-tree(4)、celeste-spikes(4)、bullet(4) |
| R2 | **明暗只是“剪影边缘 ±1 阶”**，不是光照：`shade` 只看 `(x±lx, y±ly)` 邻格是否透明，内部平面完全不变，没有体积/法线/面的概念，也无法区分顶面和侧面 | `pxl-ops/src/apply.rs::shade`（约 680–720 行） | 箱子、树冠、史莱姆看起来是“扁平色块+描边光” |
| R3 | **材质 = 值噪声**：地块是周期性 value noise + 斑点，没有结构（土块、草叶方向、矿石团簇的高光/阴影），块大小固定 cell=4/8，读起来像迷彩 | `pxl-gen/src/noise.rs::generate_noise_tile`（`"dirt" => (4,17,false)` 等硬编码） | terra-tiles（对比图里 pxl 16×16 比 Terralistic 8×8 更“糊”，矿石散点无团块） |
| R4 | **描边是一刀切**：`paint_outline` 只有 outer/inner + 刷子形状，没有选择性描边（selout：随底色变色、受光面断线）、没有内部轮廓、没有 AA 角点 | `pxl-core/src/outline.rs::paint_outline` | 所有角色/道具都是同色硬黑边 |
| R5 | **没有抖动/AA/渐变工具**：`bayer4` 只用于 `dissolve` 擦像素，不能做两色过渡 | `apply.rs::dissolve` | 大面积单色、带状（banding） |
| R6 | **配色问题**：调色板手写、ramp 短（3–4 阶）、无色相偏移（hue shift），暗部只是变暗不变冷；例如 meadow-slime 用饱和绿而参考是低饱和灰白 | `examples/reference/*/palette.json`；`Palette::shift_on_ramp` 只沿 ramp 移位 | slime、tree、crate |
| R7 | **画布尺寸/比例不对齐参考**：参考多为 8×8（Celeste、Terralistic），pxl 普遍做 16×16，再按 8× 放大对比，像素密度和细节层级不一致；slime 参考是贴地的扁形，pxl 是满格圆形 | 对比图 `attachments/*.png`；各 `recipe.json` 的 `w/h` | 所有“风格分”偏低的项 |
| R8 | **QA 不测审美**：`pxl-qa` 只测调色板索引、bbox 20–95%、连通块 ≤8、孤立像素 ≤4、动画差分，因此“能通过 QA”≠“像参考” | `pxl-qa/src/lib.rs::frame_checks / anim_diff / tile_checks` | 迭代时没有反馈信号 |
| R9 | 动画缺区域形变：只有整数平移/错行/squash，没有局部变形（头发、果冻挤压）、轨迹 | `apply.rs` Op::Shift/RowShift/Squash；`macros.rs` 的宏均是整帧位移 | celeste-run、slime bob、soul-bullet |
| R10 | 任意角度旋转未实现（报错 “cleanEdge angles arrive in M2”）；autotile 未实现 | `apply.rs` Op::Rotate；`pxl-cli/src/cmd.rs` TileCmd::Autotile | spikes 四向、地形拼接、子弹朝向 |

**结论**：低风格分主要来自 R1/R2/R3/R6/R7——“程序化原语 + 边缘明暗 + 噪声”的生成范式本身达不到手绘参考的造型和光影；功能缺口（R9/R10）只解释其中一部分素材。

## 2. 自动校验像素画的方法

### 2a. 可在 pxl-qa 里做成确定性 Rust 检查

| 检查 | 方法 | 参考 |
|---|---|---|
| 调色板一致性 | 已有索引检查；再加 “每个 ramp 实际使用数”“总色数上限”“相邻 ramp 阶的 ΔE(OKLab) 单调、色相偏移方向一致” | Lospec 调色板库 https://lospec.com/palette-list ；Slynyrd 配色教程 https://slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes |
| 孤立像素 / 噪点密度 | 已有 ≤4；改成按面积比例 + 区分“有意高光”（与 ramp 最亮阶相邻则放行） | — |
| Jaggies（锯齿线） | 沿轮廓链码分段，检测段长序列不单调（如 2-1-2-1-3）→ 不平滑曲线 | Lospec jaggies 教程 https://lospec.com/pixel-art-tutorials/tags/jaggies |
| Banding | 相邻两条 ramp 色带等宽且平行于轮廓（距离变换后每个距离环同色）→ 报 banding | 同上（像素画常识规则） |
| Pillow shading | 明暗等级与“到轮廓的距离”高度相关、与光照方向无关 → 计算明度对距离场与对光向量 `dot(pos, L)` 的相关系数 | — |
| 描边连续性 | 外轮廓 8-连通链是否闭合、是否出现双像素厚角（L 形“肘”） | — |
| 1× 剪影可读性 | 二值剪影的 Hu 矩/面积凸度；与参考剪影做 IoU；缩到 1× 后连通块数 | — |
| 结构相似度 | SSIM / MS-SSIM（相对参考，同尺寸） | Rust: https://docs.rs/image-compare ，https://github.com/kornelski/dssim （https://docs.rs/dssim-core） |
| 多帧一致性 | 已有 anim_diff、mirror；补“各帧色数直方图稳定”“剪影面积波动”“锚点（脚线）稳定”（已有 `foot_line` 可推广） | — |
| 平铺无缝 | 已有 `tile_checks`；补 2×2 拼贴后边界梯度统计 | — |
| 量化/降色 | 若引入 3D 烘焙/照片，再用 libimagequant 做量化后检查 | https://github.com/ImageOptim/libimagequant |

### 2b. 需要学习型指标或 LLM/VLM 评审

| 方法 | 用途 | 链接 |
|---|---|---|
| LPIPS | 感知距离，与参考比“风格接近度”（需放大到 ≥64px，Python/ONNX） | https://github.com/richzhang/PerceptualSimilarity |
| CLIP / CLIPScore | 文本—图像一致性（“a wooden crate pixel art”） | https://github.com/openai/CLIP ，https://arxiv.org/abs/2104.08718 |
| VLM 评审 | 用固定量表（造型、光源、配色、可读性 1–10）+ 并排参考图打分；就是本次 3–8 分的来源，应固化为 prompt 与评分 JSON | — |
| Aseprite 脚本 | 人工复核环节（Lua API 可批量导出/检查图层与色数） | https://github.com/aseprite/api |
| 像素画矢量化研究 | 了解轮廓/曲线的度量方法 | Kopf & Lischinski, Depixelizing Pixel Art https://johanneskopf.de/publications/pixelart/ |

建议：pxl-qa 加 `rules: "style"`，跑 2a 的确定性检查作为硬门槛；VLM 评分作为 CI 外的可选报告（不确定性高，不宜作硬门槛）。

## 3. 功能缺口表

上游：zfedoran/pixel-sprite-generator（MIT）、MaartenGr/Sprite-Generator（无许可，仅阅读）、Pixelorama `src/Shaders`（MIT）、Piskel（Apache-2.0）。

**Sprite-Generator 的 bug**：`SpriteGenerator/Sprite.py::_apply_colors` 里 `if is_vertical_gradient:` 两个分支完全相同（`ulen=height; vlen=width` 且都用 `mask[u][v]`），所以垂直/水平渐变实际是同一种。zfedoran 原 JS（`renderPixelData`）是正确的。pxl 的 `bollinger.rs` 有测试 `horizontal_gradient_differs_from_vertical`，没有继承这个 bug。

| 来源 | 功能 | pxl 状态 | 位置 | 对差结果的贡献 |
|---|---|---|---|---|
| PSG/SG | 随机身体蒙版 | ✅ | `bollinger.rs::sample_body` | 低 |
| PSG/SG | 镜像 X/Y | ✅ | `bollinger.rs::mirror_grid`，op `flip` | 低 |
| PSG/SG | 描边 | ✅（无选择性描边） | `bollinger.rs::add_edges`，`outline.rs` | 中（选择性描边缺失） |
| PSG/SG | 逐行/逐列换色 + sin 亮度渐变 | ✅（映射到 ramp） | `bollinger.rs::colorize` | 低 |
| PSG/SG | 饱和度/亮度随机 | ⚠️ 只能选 ramp 索引，无连续 HSV | `colorize` | 低 |
| Pixelorama | Offset Pixels | ✅ shift/row_shift（wrap） | `apply.rs` | 低 |
| Pixelorama | Outline/Inline | ✅ outer/inner | `outline.rs` | 中 |
| Pixelorama | Palettize / IndexMap | ⚠️ 原生索引图，但无“RGB→调色板”量化器（cmd.rs 只提示 quantize） | — | 中（阻碍 3D 烘焙/外部素材） |
| Pixelorama | Color Replace | ✅ palette_swap | `apply.rs` | 低 |
| Pixelorama | Rotate（nearest / cleanEdge / OmniScale / SmearRotxel） | ❌ 仅 rotate90，`Op::Rotate` 报错 | `apply.rs` | 中（四向机关、子弹朝向） |
| Pixelorama | Dither（Posterize 带抖动） | ❌ bayer 只用于 dissolve | `apply.rs::dissolve` | **中高**（渐变/过渡） |
| Pixelorama | Gradient / Gradient Map | ❌ | — | **中高**（光照 ramp 映射） |
| Pixelorama | Desaturate / HSV / Brightness-Contrast / Curves | ❌（只能换 ramp） | — | 中（配色调优） |
| Pixelorama | Drop Shadow | ❌（可用 shift+mask_fill 手搓） | — | 低中 |
| Pixelorama | Invert / Luma / Pixelize / Blur / Convolution | ❌ | — | 低 |
| Pixelorama | Flat→Isometric、Corner Pin | ❌ | — | 低（当前素材不需要） |
| Piskel | 水平/垂直翻转 | ✅ flip | `apply.rs::flip` | 低 |
| Piskel | 旋转 90/180 | ✅ rotate90 | `apply.rs::rotate90` | 低 |
| Piskel | 平移、居中、裁剪 | ✅ shift / anchor 对齐 | `apply.rs`（约 405–460 行） | 低 |
| Piskel | 洋葱皮/逐帧编辑 | ⚠️ patch_pixels 可逐帧改 | — | 中（缺人工微调回路） |
| 本项目自列 | autotile、9-slice/字体、区域形变、弹道、手绘立绘、3D 烘焙 | ❌（autotile 显式报未实现） | `cmd.rs::TileCmd::Autotile` | 中（决定“能不能做”，不决定“好不好看”） |

## 结论

缺功能**不是主因**。上游这几个生成器/编辑器的功能 pxl 大多已有（蒙版、镜像、描边、ramp 着色、翻转、平移、换色），而它们本身产出的也只是“程序味”的精灵。风格分 3–6 的主因是：①矩形原语和无手调剪影；②只有边缘明暗，没有光照模型和面分层；③纯噪声材质；④配色无 hue shift、饱和度失当；⑤画布尺寸和参考不一致；⑥QA 没有审美反馈。功能缺口中真正影响画质的是抖动、渐变映射、选择性描边、HSV 调整，其余影响的是能不能做（覆盖面）。

## 优先修复清单

1. **P0 配方层**：按参考尺寸重做（8×8 用 8×8）；用 `pixels` 矩阵手写关键剪影/材质 stamp，不再用矩形拼贴；调色板加 hue-shift ramp（暗部偏冷/偏紫，亮部偏暖），控制饱和度。
2. **P0 明暗**：新 op `light`：按“面/区域 id + 光向量 + 距离场”分配 ramp 阶（支持顶面/侧面、AO、rim），替代纯边缘 `shade`；配 QA 的 pillow-shading 检查。
3. **P1 描边**：`outline` 加 `selective`（描边色 = 邻接内部色 ramp 下移 2 阶，受光侧断开）、内部轮廓、角点 AA。
4. **P1 抖动 + gradient_map**：`dither`（bayer2/4、checker，两 ramp 阶间）与 `gradient_map`（按明度/坐标映射到 ramp）。
5. **P1 材质生成器**：noise-tile 改为“结构化”：Voronoi/团块 + 每团块自带高光/阴影 + 方向性草叶；矿石用团簇而非散点。
6. **P1 QA `style` 规则**：jaggies、banding、pillow、描边连续性、ramp 使用、相对参考 SSIM/剪影 IoU；VLM 量表评分作可选报告。
7. **P2 功能覆盖**：cleanEdge/rotxel 任意角旋转（可参考 Pixelorama MIT shader 移植到 CPU）、autotile（47-blob / 16-wang）、RGB→调色板量化器、HSV/desaturate、drop_shadow、区域形变与弹道宏、9-slice/字体。
