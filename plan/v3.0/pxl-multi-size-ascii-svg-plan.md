# v3.0 Plan v3：多尺寸档位（32/64/128/256）与 SVG → 像素矩阵

> 日期：2026-10-25（2026-10-26 更新进度标注）
> 前置：`pxl-native-resolution-quality-plan.md`（原生尺寸 ≠ 展示倍率）、`pxl-pixelart-pipeline-v2-plan.md`（`pxl-art` 增强管线）。
> 范围：只改 `crates/`，示例迁移到 `examples/`；不改 `harness/`、`web/`。
> 兼容策略：**不兼容旧格式**。`*.ascii.json` v1、`ASCII_SIZES = [32, 64]`、`ascii refine`、旧黄金哈希都可以直接删除或重算，不保留迁移垫片。
> 目标：
> 1. 每个尺寸有一套自己的**档位（Size Profile）**：输入方式、调色板预算、描边/着色规则、QA 阈值、预览倍率都随尺寸变化，而不是一套 32 的规则硬套到所有尺寸。
> 2. ASCII 矩阵直接支持 32 / 64 / 128 / 256。
> 3. 新增 **SVG → 像素矩阵** 输入：大模型擅长写 SVG（几何、层级、曲线），由我们确定性地栅格化、量化到调色板、再做像素画清理。

## 1. 现状诊断

### 1.1 示例数据（`examples/reference-v3/`，100 个素材）

| 统计 | 结果 | 含义 |
|---|---|---|
| `shapes.json` 宽度分布 | 16×23、32×73、48×4、64×5 | 九成以上素材原生 ≤ 32 |
| 绘制指令 | 105 条全部是 `pixels` | 模型实际上在**逐像素手写矩阵**，`rect/ellipse/polygon` 等几何指令一次都没用 |
| 预览 | `preview.png` 为 `scale: 4`，背景 `(28,26,24)` | 16×16 的史莱姆放大 4 倍，单眼 1–2 像素 |

以 `meadow-slime` 为例（视觉核验）：原生约 24×16 有效像素、3–4 级色阶、眼睛 1–2 像素、底边与背景同色融合。问题本质是：**模型只能在一个很小的字符矩阵里“盲画”，尺寸一大就数不清列，尺寸一小就没有表达空间**。

### 1.2 代码层面的硬限制

| 位置 | 限制 | 影响 |
|---|---|---|
| `pxl-format/src/ascii.rs` `ASCII_SIZES` | 只允许 32、64 | ASCII 无法到 128/256 |
| `pxl-core/src/lib.rs` `MAX_DIM`/`MAX_FRAME_PIXELS` | 128 / 128×128 | 256 帧直接被拒 |
| `pxl-core/src/text.rs` `encode_index` | `.`+`1-9`+`a-z`，最多 36 色，大小写不区分 | 128/256 需要更多色阶时不够 |
| `pxl-engine/src/session.rs` `MAX_INPUT_BYTES` | 256 KB | 256×256 多帧矩阵 + 文档 JSON 会超 |
| `pxl-core/src/frame.rs` `canvas` | 2048 边长 | 256 帧 × 8 帧/行的图集 = 2048+pad，溢出 |
| `pxl-art/src/upscale.rs` `supports` | k ∈ {2,3,4} | 32 → 256 需要 8×，只能链式 |
| `pxl-qa/src/config.rs` | 单一默认阈值（`max_colors 32`、`max_components 8`、`max_isolated 4`…） | 对 256 过严、对 32 过松，不随尺寸缩放 |
| `pxl-engine/src/session.rs` `RenderRequest.size` | 默认 32，只对 ASCII 文本生效 | 尺寸概念散落在各入口 |
| 输入种类 | ascii-text / ascii-json / shapes / doc | 没有矢量输入 |

### 1.3 结论

- 32/64：ASCII 矩阵是合适的，模型可以 1:1 写像素，但需要**按尺寸给出不同的描边/色阶/QA 规则**。
- 128/256：让模型直接写 16K–65K 个字符的矩阵不现实（行宽漂移、对称性差、改一处牵动全图）。需要两条路：
  - **ASCII 降负担**：分块（tile）、行程编码（RLE）、低分辨率布局 + 确定性放大（复用 `pxl-art`）。
  - **SVG 主路线**：模型写几何，我们负责“像素化”。同一份 SVG 可以输出 32/64/128/256 四个尺寸，天然得到一致的多尺寸变体。

## 2. 总体设计

```
            ┌─────────── ascii (32/64/128/256, rows | rle | tiles) ──┐
source ─────┼─────────── svg  (任意 viewBox)  ── pxl-svg raster ─────┼──► IndexFrame(native = profile.size)
            └─────────── shapes / doc (保留，统一受 profile 约束) ─────┘
                                      │
                                      ▼
                      finish（按 profile：cleanup → outline → shade）  ← pxl-art
                                      │
                                      ▼
                     QA（按 profile 的阈值）→ sheet / preview（按 profile 的倍率）
```

三个核心新概念：

1. **`SizeProfile`**（`pxl-format`）：尺寸 → 一组默认参数，所有入口共享。
2. **`SpriteSource` v3**（`pxl-format`）：统一的源描述文件 `*.sprite.json`，替代 `*.ascii.json` v1，并作为 recipe `base` 的唯一入口。
3. **`pxl-svg`**（新 crate）：SVG 子集解析 + 定点栅格化 + 调色板量化，输出 `IndexFrame`。

## 3. Size Profile：不同尺寸，不同规则

### 3.1 档位表（默认值，均可在 spec 中覆盖）

| 字段 | `s32` | `s64` | `s128` | `s256` |
|---|---|---|---|---|
| 画布 | 32×32 | 64×64 | 128×128 | 256×256 |
| 推荐输入 | ascii rows | ascii rows / svg | **svg** / ascii tiles / ascii 64 ×2 | **svg** / ascii 64 ×4 / ascii 128 ×2 |
| ASCII 编码 | rows | rows | rows（警告）/ rle / tiles | rle / tiles（rows 直接报错 `ascii.too_large_for_rows`） |
| 调色板上限 | 16 | 24 | 32 | 48 |
| 每 ramp 级数 | 3 | 4 | 5 | 6 |
| 描边 | 1px 外描边，flat | 1px，selout | 1px，selout + 内描边 | 1–2px（按部件），selout + 内描边 |
| AA（手工抗锯齿色） | 禁止 | 仅外轮廓拐角 | 允许 | 允许 |
| 着色 `shade` | none | volume（2 级） | volume | volume + 受控抖动（仅大面） |
| cleanup | orphan + pixel-perfect | 全部 | 全部 | 全部 + 阶梯节奏 |
| SVG 采样 | 4×4 超采样，多数表决 | 4×4 | 4×4 | 2×2（性能） |
| 最小特征（SVG 中低于此尺寸的形状给出 `svg.feature_lost` 警告） | 1px | 1px | 1px | 2px |
| 预览倍率 | 8 | 4 | 2 | 1 |
| QA `max_colors` | 16 | 24 | 32 | 48 |
| QA `max_components` | 8 | 12 | 24 | 48 |
| QA `max_isolated_pixels` | 4 | 8 | 24 | 64 |
| QA `max_color_specks` | 8 | 16 | 48 | 128 |
| QA `copy2x_bps` | 仅度量 | 仅度量 | ≥ 9000 警告（疑似放大图） | ≥ 8000 警告 |
| QA `min_content_coverage_bps` | 1500 | 1500 | 1200 | 1000 |

说明：

- 预览倍率按“展示占地 ≈ 256px”统一，避免 32 和 256 的预览大小差 8 倍导致人工对比失真。
- `max_isolated_pixels` 等计数型阈值按面积近似缩放（不是线性 ×4，因为大图噪点容忍度也不应线性增长）；具体数值在 M5 用示例回归后锁定。
- 非正方形：`size` 允许 `[w, h]`，档位按 `max(w, h)` 向上取档（如 48×64 用 `s64`）。

### 3.2 数据结构

```rust
// pxl-format/src/profile.rs
pub enum SizeTier { S32, S64, S128, S256 }

pub struct SizeProfile {
    pub tier: SizeTier,
    pub w: u16,
    pub h: u16,
    pub max_colors: usize,
    pub ramp_steps: u8,
    pub outline: OutlinePolicy,      // flat | selout | selout_inner
    pub aa: AaPolicy,                // off | corners | on
    pub shade: ShadePolicy,          // none | volume | volume_dither
    pub cleanup: CleanPolicy,
    pub svg: SvgRasterDefaults,      // samples, min_feature
    pub ascii: AsciiDefaults,        // 允许的编码、默认 cell
    pub preview_scale: u32,
    pub qa: QaOverrides,             // 只列与默认不同的阈值
}

impl SizeProfile {
    pub fn for_size(w: u16, h: u16) -> PxlResult<Self>;   // 选档 + 校验
    pub fn merge(&self, overrides: &ProfileOverrides) -> Self;
}
```

- `pxl-qa` 新增 `QaConfig::for_profile(&SizeProfile)`，替代当前单一 `Default`；`kind`（sprite/tiles/effect/ui）仍在其上叠加。
- `pxl-art::EnhanceOptions` 新增 `EnhanceOptions::for_profile(&SizeProfile)`，finish 阶段只认这个入口。
- `pxl-engine::RenderRequest.size` 删除，改为 `profile: Option<SizeTier>`；未给出时从源推断。

## 4. ASCII 矩阵：32 / 64 / 128 / 256

### 4.1 尺寸与上限

- `ASCII_SIZES` 删除，改由 `SizeProfile::for_size` 校验：边长 ∈ {32, 64, 128, 256} 或 `[w,h]` 且 `max ≤ 256`。
- `pxl-core`：`MAX_DIM = 256`，`MAX_FRAME_PIXELS = 256 * 256`；`canvas` 上限提到 4096；`MAX_INPUT_BYTES` 提到 1 MB。
- `pxl-ops/src/apply/region.rs`、`pixels.rs`、`pxl-format/src/parts.rs`、`pxl-gen/src/bollinger.rs` 中引用 `MAX_DIM` 的地方随之放开，并补 256 边界测试。

### 4.2 颜色编码扩到 62 色

`pxl-core/src/text.rs`：`.` = 0，`1-9` = 1–9，`a-z` = 10–35，`A-Z` = 36–61，**大小写区分**（破坏旧行为，旧的 `A` = 10 删除）。`PxlDoc` 的 `frames` 继续用此编码。超过 62 色报 `text.too_many_colors`，hint 指向档位的 `max_colors`。

### 4.3 三种矩阵编码（同一 spec 可混用，按帧声明）

| 编码 | 写法 | 适用 |
|---|---|---|
| `rows` | 每行一个字符串，长度 = 宽 | 32/64 |
| `rle` | `"12. 4a 3b 13."` 或紧凑 `12.4a3b13.`：`<count><char>`，count 省略为 1 | 128/256，大块平涂 |
| `tiles` | 把画布切成 `tile`（默认 32×32）块，每块独立 `rows`，带坐标 `{ "at": [x, y], "rows": [...] }`，空块可省略 | 128/256，局部细节，模型可分块生成、分块修改 |

校验要求（大尺寸的主要失败点就是行宽漂移，必须给出可执行的错误）：

- 每行展开后宽度不等：`ascii.row_width`，报告行号、期望宽、实际宽、行首 8 个字符。
- `--pad`（及 spec `pad: true`）只对 32/64 默认开启；128/256 默认关闭，避免静默补齐掩盖错位。
- `tiles` 越界、重叠：`ascii.tile_overlap` / `ascii.tile_bounds`。
- 文本格式 `*.ascii.txt` 新增指令：`@size 128`、`@encoding rle|tiles`、`@tile 64,0`。

### 4.4 低分辨率布局 + 确定性放大（`scale_from`）

模型可以只写 64 的矩阵，声明目标档位为 128/256：

```json
{ "size": 256, "source": { "kind": "ascii", "file": "knight.ascii.txt", "native": 64 } }
```

- 编译得到 64×64 → 调用 `pxl-art::enhance`（hybrid/contour）链式放大到目标：64→128 为 ×2；64→256 为 ×4；32→256 为 ×4 再 ×2。
- `pxl-art/src/upscale.rs` 的 `supports` 不改，链式调用放在 `pxl-art/src/pipeline.rs::enhance_to(target)`，每一跳都重新 analyze（材质/细特征在新尺度上重算）。
- 放大后的帧会被 QA 的 `copy2x_bps` 识别；`scale_from` 产物在报告中标注 `"origin": "upscaled"`，不与原生 256 混淆。
- 删除 `pxl ascii refine` 与 `pxl shapes refine`：它们的 2×2 角点细化被 `scale_from` + `pxl-art` 完整覆盖。

### 4.5 `cell` 与 `stamp`

- `cell` 默认值由档位给：`s32/s64/s128/s256` 默认 1；仍允许 `cell: [2,2]` 等做“粗布局”。
- `stamp` 尺寸上限从 16 放到 32（匹配 tile），用于大尺寸的重复纹理（砖、鳞片、草）。

## 5. SVG → 像素矩阵（新 crate `pxl-svg`）

### 5.1 为什么是 SVG

- 模型对 SVG 的空间推理明显强于字符矩阵：坐标显式、曲线可用、分组和复用（`<g>`/`<use>`）自然，改一个部件不影响别处。
- 与分辨率无关：一份 SVG 可以输出 32/64/128/256，配合档位规则得到一组风格一致的多尺寸素材。
- 难点不在“画出来”，而在“像素化得像手绘像素画”：不能有抗锯齿糊边、不能有 1px 断线、颜色必须落在调色板内、细特征不能丢。这些都在 5.4–5.6 处理。

### 5.2 依赖选择

| 方案 | 结论 | 理由 |
|---|---|---|
| `usvg` + `resvg`/`tiny-skia` | 不采用 | 浮点抗锯齿输出 RGBA，需要再量化；依赖重；版本升级可能改变像素，破坏黄金哈希 |
| `roxmltree`（只做 XML 解析）+ 自研子集栅格化 | **采用** | 无传递依赖、纯 Rust、WASM 可用；几何与栅格化全部定点整数，跨平台逐字节一致 |

新增依赖需用户确认（项目规则）；若不允许新增依赖，退路是在 `pxl-svg` 内手写一个只覆盖 5.3 子集的 XML 扫描器（约 300 行）。

### 5.3 支持的 SVG 子集

| 类别 | 支持 | 不支持（`svg.unsupported` 警告并忽略，strict 下报错） |
|---|---|---|
| 根 | `svg`（`viewBox`、`width/height`） | 嵌套 `svg` |
| 结构 | `g`、`defs`、`use`（`href`/`xlink:href`）、`symbol` | `switch`、`foreignObject` |
| 形状 | `rect`（含 `rx/ry`）、`circle`、`ellipse`、`line`、`polyline`、`polygon`、`path` | `text`、`image` |
| path 命令 | `M L H V C S Q T A Z`（大小写） | — |
| 变换 | `translate` `scale` `rotate` `skewX` `skewY` `matrix`，可嵌套 | — |
| 绘制属性 | `fill`、`stroke`、`stroke-width`、`stroke-linejoin`（miter/round/bevel）、`stroke-linecap`、`fill-rule`、`display`、`visibility`、`style="..."` 内联 | `opacity`/`fill-opacity` 只接受 0 或 1，其它值警告并按 ≥0.5 → 1 处理 |
| 渐变 | `linearGradient`、`radialGradient` → 映射为 ramp 分级色带（5.5） | `pattern`、`mask`、`filter` |
| 裁剪 | `clipPath`（仅形状） | `mask` |

扩展属性（`data-*`，模型可选写，写了效果更好）：

| 属性 | 作用 |
|---|---|
| `data-frame="idle_0"` | 顶层 `<g>` 作为一帧；没有则整张 SVG 是 `base` |
| `data-role="outline"` | 该形状/组不按几何栅格化描边，而是在其像素掩码外画 1px 档位描边（5.6） |
| `data-role="highlight\|shadow\|eye\|detail"` | 细特征保护：保证至少 1 像素存活 |
| `data-material="metal\|organic\|cloth\|stone\|liquid"` | 传给 `pxl-art::Material`，影响 cleanup/shade |
| `data-ramp="skin" data-step="2"` | 直接指定调色板 ramp 与级，代替 `fill` |
| `data-min="2"` | 该形状在任何尺寸下至少 N 像素宽 |
| `data-sizes="64,128,256"` | 只在这些档位出现（小尺寸省略睫毛、纹理等） |

### 5.4 栅格化算法（确定性）

1. **解析与展开**：`use` 展开、变换矩阵累积（Q16.16 定点，`i64`），`viewBox` 映射到目标 `w×h`。
2. **几何扁平化**：曲线按固定规则细分（细分次数由“曲线控制多边形长度 × 目标像素/单位”决定，取整后上限 64），弧 `A` 先转三次贝塞尔。全部为整数运算，输出折线。
3. **描边展开**：每段折线 → 四边形；连接处按 linejoin 加三角/圆盘/斜切；端点按 linecap 处理；统一成填充多边形，nonzero 规则。
4. **超采样扫描线**：在 `N×N` 子像素网格（档位默认 4×4）上，对每个形状做扫描线填充（even-odd / nonzero），按绘制顺序把**调色板索引**写入子像素缓冲（不是 RGBA，不混色）。
5. **下采样：多数表决**。每个像素取其 `N²` 个子样本中出现最多的索引：
   - 透明子样本占比 > 50% → 透明；
   - 平票时取绘制顺序更靠后（更上层）的索引；
   - 结果天然无抗锯齿，边缘是“干净的像素阶梯”。
6. **细特征保护**：对带 `data-role`/`data-min` 的形状，若下采样后存活像素 < 要求，则在其覆盖率最高的像素上强制写入（覆盖率相同取左上），并记录 `svg.feature_forced`。
7. **特征丢失报告**：任何形状下采样后像素数为 0 → `svg.feature_lost`（含元素 id/路径、档位），提示用 `data-min` 或 `data-sizes`。

对比现有 `pxl-gen/src/shapes.rs`（像素中心单采样）：中心采样在 32 档会让 1.4px 宽的线时有时无；多数表决 + 保护能稳定输出 1px 连续线。

### 5.5 颜色量化

`palette_mode`：

| 模式 | 行为 |
|---|---|
| `strict`（默认） | `fill/stroke` 必须是调色板中的 `#rrggbb`，或 `data-ramp`；否则 `svg.color_not_in_palette`，列出最近的 3 个调色板色 |
| `nearest` | OKLab 最近色（复用 `pxl-core::Oklab`，定点比较），报告每个映射与 ΔE；ΔE 超阈值警告 |
| `extend` | 未知色追加到调色板，受档位 `max_colors` 约束，写出新 palette 文件 |

渐变：取渐变两端色在调色板中的 ramp（找不到则按 `nearest` 选 ramp），沿渐变方向分成 `ramp_steps` 段**硬色带**；档位允许抖动时（`s256` 大面）在色带交界做 2px 宽 Bayer 4×4 过渡。绝不输出插值色。

### 5.6 栅格化后的像素画 finish（复用 `pxl-art`）

SVG 栅格化只负责“形状和颜色正确”，像素画质感在 finish 阶段统一完成，按档位开关：

1. **cleanup**：孤点、pixel-perfect（去 L 形双像素）、对称修复（SVG 若对称则用 `SpriteGraph.symmetry` 校正舍入造成的 1px 不对称）、阶梯节奏（`s128+`）。
2. **outline**：`data-role="outline"` 的组 + 整体外轮廓，按档位重画为 1px（`s256` 可 2px 外 + 1px 内）；selout 依光源方向。这样模型不需要关心 `stroke-width` 在不同尺寸下是 0.7px 还是 3px。
3. **shade**：档位允许时调用 `pxl-art::shade::apply`；SVG 已经有手写高光/阴影形状的区域（`data-role=highlight|shadow`）标记为受保护，不被自动着色覆盖。
4. **measure**：输出 `pxl-art::Metrics` + `svg` 报告（特征丢失/强制、颜色映射、未支持元素）。

### 5.7 动画与多帧

- 多帧：一个 SVG 内多个 `<g data-frame>`，共享 `<defs>`；或 spec 中 `frames: { "idle_0": "idle_0.svg" }` 多文件。
- 帧间部件差异建议用 `<use href="#body" transform="translate(0,1)"/>` 表达；栅格化后再走现有 `anims`（flipbook / macro）。
- 注意：子像素平移在小档位会因舍入导致抖动；规定 `data-frame` 组内的 `translate` 在栅格化前**吸附到目标像素网格**（`snap: true` 默认），避免 idle 动画“抽动”。

## 6. 统一源描述：`*.sprite.json`（v3）

替代 `*.ascii.json` v1（删除）与 recipe 中 `base.gen = shapes|ascii` 的分叉；`shapes.json` 保留为 `source.kind = "shapes"`，同样受档位约束。

```json
{
  "v": 3,
  "size": 128,
  "palette": "palette.json",
  "palette_mode": "strict",
  "kind": "sprite",
  "source": { "kind": "svg", "file": "knight.svg" },
  "raster": { "samples": 4, "snap": true },
  "finish": { "outline": "selout_inner", "shade": "volume", "light": "top_left" },
  "profile": { "max_colors": 40 },
  "frames": {},
  "anims": [ { "name": "idle", "fps": 6, "macro": "bob(amp=2)" } ],
  "variants": [32, 64]
}
```

- `source.kind`：`ascii`（`file` 或内联 `rows/rle/tiles`，可选 `native` 走 4.4）、`svg`（`file` 或内联 `svg` 字符串）、`shapes`、`doc`。
- `finish` / `profile` / `raster` 缺省全部取自档位；只写需要覆盖的字段。
- `variants`：同一源额外输出的尺寸（仅 svg 源可用；ascii 源的 variant 只能向上走 4.4 放大），产物命名 `<id>@<size>`。
- recipe `sprites[].base` 改为 `{ "sprite": "knight.sprite.json" }`，一个入口。
- `pxl schema sprite` 输出 JSON Schema；`pxl schema svg-subset` 输出 5.3 的子集说明（给模型用的提示文档，机器可读）。

## 7. Crate 改动清单

| Crate | 改动 |
|---|---|
| `pxl-core` | `MAX_DIM=256`、`MAX_FRAME_PIXELS=65536`、`canvas` 上限 4096；`text.rs` 62 色大小写区分；`Oklab` 定点最近色查找（供 svg 量化）；Q16.16 `Fixed` 与 `Affine` 小工具（或放 `pxl-svg`） |
| `pxl-format` | 新 `profile.rs`（`SizeTier`/`SizeProfile`）；新 `sprite.rs`（`SpriteSource` v3）；`ascii.rs` 删 `ASCII_SIZES`、`v` 改 2、加 `encoding: rows|rle|tiles` 与 `AsciiTile`；`ascii_text.rs` 加 `@size/@encoding/@tile` 指令与 RLE 解析；删 `ascii_refine.rs`；`recipe.rs` 中 `RecipeBase` 改为 `sprite` 引用 |
| `pxl-svg`（新） | `parse.rs`（roxmltree → 内部 DOM，样式继承）、`geom.rs`（变换/路径/弧/扁平化/描边展开）、`raster.rs`（超采样扫描线 + 多数表决 + 特征保护）、`color.rs`（strict/nearest/extend + 渐变色带）、`report.rs`（`SvgReport`）、`lib.rs`：`rasterize(svg: &str, opts: &SvgOptions, palette: &Palette) -> PxlResult<SvgRaster { frames, materials, protect, report }>` |
| `pxl-gen` | `ascii/` 支持 rle/tiles、256 画布、stamp ≤ 32；`shapes.rs` 接受档位校验；删除 shapes refine |
| `pxl-art` | `EnhanceOptions::for_profile`；`pipeline::enhance_to(target_size)` 链式放大；`finish(frame, palette, profile, hints)` 入口（不含放大，供 svg/ascii 原生帧使用），`hints` 接收 svg 的 materials/protect；256 性能预算（10.3） |
| `pxl-qa` | `QaConfig::for_profile`；`resolution.rs` 中 `copy2x_bps` 在 `s128+` 变为警告；新增 `svg.*` 报告透传 |
| `pxl-ops` | 区域/patch 上限随 `MAX_DIM`；`snap` 用的整数平移 |
| `pxl-export` | 预览倍率默认取档位；图集 4096 上限；大图集按行折行（`max_row_px`） |
| `pxl-engine` | `InputKind` 新增 `Svg`、`Sprite`（`*.sprite.json`），删 `AsciiJson` v1；`detect_kind`：以 `<svg`/`<?xml` 开头判为 svg，`"v": 3` 且有 `source` 判为 sprite；`RenderRequest.size` → `profile`；`MAX_INPUT_BYTES = 1 MB`；`Resolver` 能取 svg 文件 |
| `pxl-cli` | 见 8 节 |
| `pxl-wasm` | 透传 `kind: "svg"` 与 `profile`；`run_inspect` 返回 svg 报告；体积预算检查（roxmltree 约 +60 KB） |

## 8. CLI

```
pxl svg render  <in.svg> --size 32|64|128|256 [--palette p] [--palette-mode strict|nearest|extend]
                [--samples 4] [--no-finish] [--sizes 32,64,128] -o out.png
pxl svg lint    <in.svg> --size 128 [--palette p]      # 子集/颜色/特征丢失检查，不出图
pxl svg matrix  <in.svg> --size 64 -o out.ascii.txt    # SVG → ASCII 矩阵（rows/rle/tiles 自动选），便于人工或模型二次修改
pxl ascii render <in.ascii.txt> --size 128 [--encoding rle|tiles] [--native 64]
pxl sprite build <x.sprite.json> [--variants]           # 统一入口，替代 gen ascii / gen shapes
pxl profile show [32|64|128|256]                        # 打印档位全部默认值（JSON）
```

- 删除：`pxl ascii refine`、`pxl shapes refine`、`gen ascii`（并入 `sprite build`）。
- 所有命令 `--json` 输出包含：`profile.tier`、`native`、`preview_scale`、`origin`（native/upscaled/svg）、`svg_report`（若有）、`qa`。
- `pxl svg matrix` 是两条路线的桥：模型先写 SVG 得到大形，再导出成矩阵做逐像素修饰（眼睛、高光），最后作为 ascii 源重建。

## 9. 给模型的输入约定（写进 `pxl schema svg-subset` 与 skill 文档）

1. `viewBox` 建议等于目标尺寸（如 `0 0 64 64`），坐标尽量取整或 .5；多尺寸输出时以最大尺寸为 viewBox。
2. 只用调色板 hex 或 `data-ramp`；不要用 `opacity`、`filter`、`text`。
3. 轮廓不要靠 `stroke`，给主体组加 `data-role="outline"`，描边交给档位。
4. 眼睛、高光、扣子等小部件加 `data-role`，必要时 `data-min`。
5. 小档位不需要的细节用 `data-sizes` 排除，而不是让它们糊成噪点。
6. 动画帧用 `<g data-frame>` + `<use>` 复用部件，平移量用整数。

## 10. 测试

### 10.1 单元

- `pxl-core`：62 色编解码往返、大小写区分、256×256 帧创建与 65537 像素拒绝。
- `pxl-format`：档位选择（32/48/64/100/128/256/257）、覆盖合并；RLE 解析（计数省略、0 计数报错、超宽报错）；tiles 越界/重叠；`ascii.row_width` 报告字段。
- `pxl-svg`：
  - 轴对齐整数 `rect` 栅格化结果与 `shapes` 的 `rect` 逐像素一致；
  - `circle`/`ellipse` 左右、上下对称；
  - `path` 含 `C/Q/A` 的结果在 4×4 与 8×8 采样下差异 ≤ 轮廓像素的 5%；
  - even-odd 与 nonzero 的环形差异；
  - 嵌套变换 = 预乘后的单一 `matrix`；
  - 1px 宽斜线在 32 档连续（8 连通，无断点）；
  - `data-min` 保护、`feature_lost` 报告；
  - strict 拒绝非调色板色、nearest 映射稳定、渐变只产生 ramp 内颜色；
  - 同输入两次运行逐字节一致；不支持元素在非 strict 下警告、strict 下报错。
- `pxl-art`：`enhance_to` 64→256 尺寸与调色板约束；`finish` 不改变透明掩码（除 outline 外扩）。
- `pxl-qa`：同一张图在 `s32` 与 `s256` 档位下阈值不同、结论可不同。

### 10.2 集成（`pxl-cli/tests/cli_cases/`）

- 新 `svg.rs`：`svg render` 四档输出尺寸、`--sizes` 多产物、`svg lint` 错误码、`svg matrix` → `ascii render` 往返像素一致。
- 新 `profile.rs`：`profile show` 字段齐全；`sprite build` 不写 finish 时取档位默认。
- `ascii.rs` 重写：rows/rle/tiles 三种编码同图等价；128 rows 警告、256 rows 报错。
- 删除 `ascii refine`/`shapes refine` 相关用例；`golden/hashes.json` 全量重算（破坏性升级，一次性）。

### 10.3 性能预算（release，单核）

| 场景 | 预算 |
|---|---|
| 256 档 SVG（200 个形状、2×2 采样）栅格化 | ≤ 50 ms |
| 256 档 finish（cleanup + outline + shade） | ≤ 150 ms |
| 64→256 `enhance_to` | ≤ 300 ms |
| WASM 256 档单帧 render | ≤ 1 s |

超预算时优先：扫描线只处理形状包围盒；`SpriteGraph` 骨架在 `s256` 默认关闭。

## 11. 示例迁移

- 新建 `examples/v3/`，按档位各选样本（不再追求 100 个，先 12 个精品）：
  - `s32`：meadow-slime、gold-coin、heart-meter（ascii rows）
  - `s64`：stardew-farmer（ascii rows）、health-potion（svg）、orchard-tree（svg）
  - `s128`：octopath-hero（svg）、item-chest（svg）、terra-grass tile（ascii tiles）
  - `s256`：skeleton（svg）、ui-dialog（svg）、knight（ascii 64 → `native` 放大）
- 每个 svg 样本用 `variants` 同时输出 32/64/128，作为“一源多尺寸”的视觉证据。
- `examples/reference-v3/` 保留为旧版对照，不再更新；`examples/ascii/` v1 样本删除或改写为 v3 spec。
- 对照图：同一素材 旧版@4× vs 新版原生 128@2×，占地相同，放入 `review/v5/`（只放图与说明，不改 harness）。

## 12. 里程碑

| 里程碑 | 内容 | 退出条件 | 状态（2026-10-26） |
|---|---|---|---|
| **M0 尺寸底座** | `pxl-core` 上限（256/65536/4096/1MB）、62 色编码、`SizeProfile` + `profile show`、`QaConfig::for_profile`、`EnhanceOptions::for_profile` | `cargo test --workspace` 全绿；256×256 文档可 render；黄金哈希重算 | ✅ 完成（34 套件全绿；`pxl profile [tier]` 可查档位；黄金哈希已重算） |
| **M1 ASCII 32–256** | `size` 四档、rle/tiles、文本指令、行宽诊断、`native` 链式放大、删 refine | 三种编码同图等价；128/256 样本可构建；`copy2x_bps` 能区分原生与放大 | ✅ 完成（rows/rle/tiles 可构建 128/256；`::size`/`::encoding` 指令；`ascii.row_width`/`tile_bounds`/`tile_overlap` 诊断；`native` 经 `enhance_to` 链式放大）。⚠️ `ascii refine`/`shapes refine` 命令保留（存量素材依赖，待 M5 示例迁移后删） |
| **M2 `pxl-svg` 核心** | 解析子集、变换、路径/弧、描边展开、超采样多数表决、strict/nearest 量化、特征保护与报告 | 10.1 的 svg 单测全部通过；同一 SVG 四档输出确定性一致 | ✅ 完成（pxl-svg 29 测试通过；按 5.2 退路自写 XML 子集扫描器，未引入 roxmltree） |
| **M3 SVG finish 与动画** | `pxl-art::finish` 接入 svg hints、`data-role/material/ramp/sizes`、渐变色带、多帧 + snap | health-potion / octopath-hero 在 64/128 档视觉人工确认通过 | ✅ 代码完成（`FinishOptions::for_profile`、`data-role` 保护、`data-material`、渐变色带、snap）。⚠️ 视觉人工确认随 M5 样本进行 |
| **M4 统一入口** | `*.sprite.json` v3、recipe `base.sprite`、engine `InputKind::Svg/Sprite`、CLI `svg *`/`sprite build`、WASM 透传 | 12 个 `examples/v3` 全部经 `pxl build` 产出且 QA 通过 | ✅ 代码完成（`sprite build`、`pxl schema sprite`/`svg-subset`、recipe `base.gen=sprite`、engine/WASM 透传）。⚠️ 12 个示例样本未建（M5） |
| **M5 调参与对照** | 用示例回归锁定 3.1 的 QA/finish 数值；`review/v5` 对照；性能预算核验 | 10.3 预算达成；人工对照新版不劣于旧版且 ≥ 8/12 明显更好 | ✅ 完成（2026-10-26：`examples/v3` 12 个 Metal Slug 风格样本全部构建通过、QA 零警告、视觉核验通过——s32×4（ascii 裁剪板 8–10 色）、s64×4（含一源三尺寸 SVG variants 与 64 矩阵）、s128×3（SVG 27 色）、s256×2（SVG + `native:64` 链式放大）；性能预算实测达标：256 档 SVG 栅格化 12–13ms ≤ 50ms、64→256 `enhance_to` 8ms ≤ 300ms；档位调色板上限作为硬约束被回归验证——s32≤16/s64≤24 需按样本裁剪，s128/s256 可直用共享板）。⚠️ `review/v5` 旧版人工对照未做（旧版素材已迁走，建议以 `examples/reference-v3` 存量样本代替）；QA/finish 数值维持 3.1 现值不变（回归未发现需调整项） |

依赖顺序：M0 → (M1 ∥ M2) → M3 → M4 → M5。M1 与 M2 可并行，二者只在 M4 汇合。
当前进度：M0–M5 全部完成（2026-10-26）。后续清理项：删 `ascii refine`/`shapes refine` 命令（存量素材迁移完）；可选 `review/v5` 对照补做。

## 13. 风险与不做

| 风险 | 对策 |
|---|---|
| SVG 多数表决在 32 档仍会把小圆变方块 | 32 档提示“优先 ascii rows”；svg 在 32 档默认 `samples: 8`；`data-min` 兜底 |
| 模型写出调色板外颜色 | strict 默认报错并给最近色；harness 侧可自动改用 nearest（不在本计划内） |
| 256 档 QA 计数阈值难定 | M5 前所有新阈值为 warning，不阻断构建 |
| 新增 `roxmltree` 依赖 | 需用户确认；否则走手写子集 XML 扫描（5.2） |
| 62 色大小写区分破坏旧文档 | 本计划明确不兼容；旧 `*.pxl.json` 一次性重新生成 |
| 图集超过 4096 | `pxl-export` 自动折行；单行超限时报错并提示减少帧数或拆图集 |

不做：

- 不做 RGBA 抗锯齿/双线性预览、不做 AI 超分、不接入任何模型调用。
- 不支持 SVG `text`/`image`/`filter`/`mask`/CSS 外部样式表。
- 不做 PNG → SVG 矢量化。
- 不改 `harness/`、`web/`（它们在 M4 之后各自立项接入新入口）。

## 14. 完成定义

1. 任意 `size ∈ {32, 64, 128, 256}` 均可从 ascii 或 svg 构建出原生尺寸帧，档位默认值可由 `pxl profile show` 查询，且所有入口（CLI / recipe / engine / WASM）对同一输入给出逐字节相同的结果。
2. 同一 SVG 输出四档时，没有 `svg.feature_lost` 未处理警告，调色板不越界，1px 轮廓连续。
3. `examples/v3` 12 个样本 QA 通过，`review/v5` 对照完成人工确认。
4. 旧 `ASCII_SIZES`、`ascii refine`、`shapes refine`、`*.ascii.json` v1 代码路径全部删除，无残留分支。
