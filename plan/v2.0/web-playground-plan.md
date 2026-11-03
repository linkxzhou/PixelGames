# v2.0 Web Playground 计划：ASCII / JSON 规格 → 像素 PNG / GIF（WASM）

> 日期：2026-10-23　状态：**计划（已确认决策，未实施）**
>
> **已确认决策（2026-10-23）**：
> 1. “规格 JSON”按默认范围：`*.ascii.json`、`*.shapes.json`、`*.pxl.json`（PxlDoc）三种；recipe 等多文件构建不进首版。
> 2. 站点只在**本地**运行，用 **yarn** 构建，不做线上部署、不接 CI 发布。
> 3. 接受抽出 `pxl-engine` 并重构 `pxl-cli`（`pxl-cli` 改为调用 `pxl-engine`，不在 `pxl-wasm` 里复制第二份组装逻辑）。
>
> 目标：做一个纯前端站点。用户在输入框里填 ASCII 或规格 JSON，浏览器内通过 WASM 调用 `crates/` 生成像素 PNG 或 GIF 动效图，不需要后端。
>
> 本计划里的结论都来自实际读代码和一次 wasm 编译验证。没有核实的地方标【估】或【未验证】。

---

## 0. 摘要

| 问题 | 结论 |
|---|---|
| 1. `crates/` 能编译成 wasm 吗？ | **底层 6 个 crate 能。** 2026-10-23 在干净副本上执行 `cargo check --target wasm32-unknown-unknown -p pxl-core -p pxl-format -p pxl-gen -p pxl-ops -p pxl-qa -p pxl-export`，全部通过。`pxl-cli` 不能直接用：它依赖 `clap`、`std::fs` 和 stdout。<br>**真正的缺口**：ASCII/JSON → 文档 → 动画 → 帧这条组装链在 `pxl-cli` 里，并且依赖文件系统。要先抽成一个无 IO 的库 crate，再包 wasm。 |
| 2. web 技术栈 | `web/`：Vite + Vue 3 + TypeScript + Tailwind + **shadcn-vue**，黑白极简，**yarn** 构建、本地运行。左边输入，右边预览，输入变化后防抖调用 wasm。 |
| 3. 示例 | 4 个：ASCII 静态图、ASCII 多帧动画、规格 JSON（shapes）、宏生成动效。见第 5 节。 |
| 4. 输出 GIF 还缺什么？ | GIF 编码器 `pxl-export::encode_gif_timed` **已经够用**（逐帧延迟、循环开关、透明索引 0、纯内存输出）。缺的是编码器之外的东西，共 3 项必须、3 项建议，见第 4 节。 |

---

## 1. 现状核查（事实）

### 1.1 workspace 与依赖

- `Cargo.toml` 的 members：`pxl-core / format / gen / ops / qa / export / cli`，外加 `harness` 和 `games/battle-city/runtime`。
- 根 `Cargo.toml` 有 `[patch.crates-io] miniquad = { path = "vendor/miniquad" }`，这是 battle-city 的 macroquad 补丁。只用 `-p <crate>` 构建 wasm 时不会编译它，但 cargo 仍会加载整个 workspace。
- 已装工具：`wasm32-unknown-unknown` target、`wasm-bindgen` CLI **0.2.129**、node 25、yarn **4.18.0**（Berry）。
- 未装工具：`wasm-pack`、`wasm-opt`。

### 1.2 各 crate 的 wasm 适配性

| crate | 有无 IO | wasm 结论 |
|---|---|---|
| `pxl-core` | 无。`#![no_std]` + alloc，依赖 `sha2`（关闭默认特性） | 通过 |
| `pxl-format` | 无。依赖 `serde`、`serde_json`、`schemars` | 通过 |
| `pxl-gen` | 无。模板和调色板用 `include_str!` 内嵌（`builtin.rs`）；`Instant::now` 只出现在 `#[test]` | 通过 |
| `pxl-ops` | 无 | 通过 |
| `pxl-qa` | 无 | 通过（网页首版不用） |
| `pxl-export` | 无。`png`、`gif` 都写入内存 `Vec<u8>` | 通过 |
| `pxl-cli` | 有：`clap`、`std::fs`、`stdin/stdout`、`std::env` | **不编译进 wasm** |

### 1.3 关键缺口：组装逻辑在 `pxl-cli` 里

下面这些在 `crates/pxl-cli/src/cmd/` 内，网页必须复用，但它们现在带着文件系统依赖：

| 功能 | 位置 | 文件依赖 |
|---|---|---|
| ASCII 文本 → `AsciiSpec`（`build_spec`、`auto_legend`、`parse_cell`） | `ascii/mod.rs` | `util::read_text`、`load_legend_file`（`fs::read_to_string`） |
| `AsciiSpec` → 帧、镜像、delta、post（`compile_frames`、`mirror_ops`） | `ascii/compile.rs` | `load_legend_ref` 读文件 |
| 加载调色板 | `util::load_palette` | 先查内置，再按路径读文件 |
| `AsciiSpec` → `PxlDoc` + 动画（`build_parts`、`attach_anims`） | `ascii/compile.rs` | 调色板、legend 都走文件 |
| 动画限制 `check_anim_timing`（fps 1..=60，帧数 1..=16） | `animation.rs` | 无，但在 CLI 内 |
| 帧序列 → PNG/GIF 字节（`render_doc`） | `render.rs` | 生成字节后直接 `write_out` 写文件 |
| shapes 规格 → 帧 | `generate.rs` | 用 `read_text` 读规格、调色板 |

所以只写一个 `wasm-bindgen` 外壳不够，要先做一次重构。

---

## 2. 架构方案

```
┌────────────────────────── 浏览器 ──────────────────────────┐
│  web/ (Vue + shadcn-vue)                                   │
│    输入框 ──防抖──▶ Web Worker ──▶ pxl-wasm.render()        │
│                          ▲              │                  │
│                          └── PNG/GIF 字节 + 元信息 ◀───────┘│
└────────────────────────────────────────────────────────────┘

crates/
  pxl-core / format / gen / ops / export     （已有，不改行为）
  pxl-engine   ★新增：无 IO 的组装与渲染库
  pxl-wasm     ★新增：cdylib，wasm-bindgen 薄封装
  pxl-cli      改为调用 pxl-engine（行为不变，黄金测试保证）
```

### 2.1 新增 `pxl-engine`（无 IO）

迁出 `pxl-cli` 里与文件无关的逻辑，文件访问换成一个 trait：

```rust
/// 读取 legend / palette / parts 等被引用的文本。CLI 实现为读磁盘，wasm 实现为内存表 + 内置资源。
pub trait Resolver {
    fn read_text(&self, name: &str) -> Option<String>;
}
```

对外 API（草案）：

```rust
pub enum InputKind { AsciiText, AsciiJson, ShapesJson, DocJson } // 加 Auto 自动识别

pub struct RenderRequest<'a> {
    pub input: &'a str,
    pub kind: InputKind,
    pub palette: Option<&'a str>, // 内置 id，如 "p16"、"tank"
    pub anim: Option<&'a str>,    // None = 自动选择，见 4.2
    pub dir: Option<&'a str>,
    pub scale: u32,               // 1..=16
    pub seed: u64,
    pub format: Format,           // Png | Gif | Auto
    pub bg: Background,           // Transparent | Solid(Rgba)
}

pub struct RenderOutput {
    pub format: Format,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub anims: Vec<AnimInfo>,     // 名称、fps、帧数、是否循环
    pub warnings: Vec<String>,
    pub hash: String,             // sha256，和 CLI 同口径
}

pub fn render(req: &RenderRequest, res: &dyn Resolver) -> PxlResult<RenderOutput>;
```

迁移原则：

- 搬运代码，不改算法，不改默认值。
- `pxl-cli` 的命令只保留“参数解析 → 调 `pxl-engine` → 写文件/打印 JSON”。
- 保护网：现有 `crates/pxl-cli/tests/golden.rs` 和 `cli_cases/*`（全工作区 176 项测试）必须保持全绿，输出哈希不变。

### 2.2 新增 `pxl-wasm`

- `crate-type = ["cdylib", "rlib"]`，依赖 `wasm-bindgen =0.2.129`。版本必须和已装的 CLI 一致。
- 导出函数，错误统一抛出 JSON 对象 `{code, message, path?, hint?}`，对应 `PxlError`：

| 导出 | 作用 |
|---|---|
| `render(input, options) -> { format, bytes, width, height, frameCount, anims, warnings, hash }` | 主入口 |
| `inspect(input, options) -> { kind, anims, frames, size }` | 不编码，只列动画，用于下拉框 |
| `version() -> string` | 返回 `PXL_VERSION` |
| `schema(kind) -> string` | 复用 `pxl-format` 的 JSON Schema，给编辑器提示用（后续） |

- `bytes` 用 `Uint8Array` 返回，前端转 `Blob` + `URL.createObjectURL`。
- Resolver 实现：内存 `BTreeMap<String,String>`（来自调用方的 `files` 字段）+ `pxl-gen` 内置调色板和模板。

### 2.3 构建链

```
cargo build --release --target wasm32-unknown-unknown -p pxl-wasm
wasm-bindgen --target web --out-dir web/src/wasm target/wasm32-unknown-unknown/release/pxl_wasm.wasm
# 可选：wasm-opt -Oz（需先安装 binaryen）
```

- 用 `wasm-bindgen` CLI 即可，不依赖 `wasm-pack`。封装成 `scripts/build-wasm.sh`。
- release profile 增加 `opt-level = "z"`、`lto = true`、`panic = "abort"`、`codegen-units = 1`。这会影响整个 workspace，所以只对 wasm 构建通过 `--config` 或单独 profile 设置，不动现有 profile。
- 体积目标 ≤ 400 KB（未压缩、不含 wasm-opt）【估】；旧的 `architecture.md` 里 300 KB 是针对更小的 API 面估的，这里放宽。实测后修正。
- 产物 `web/src/wasm/` 加入 `.gitignore`，由 `yarn wasm` 在本地生成。`web/node_modules`、`web/dist` 同样忽略。
- 根 `Cargo.toml` 的 `members` 加入 `crates/pxl-engine`、`crates/pxl-wasm`，`workspace.dependencies` 加对应条目。

---

## 3. Web 站点方案（`web/`）

### 3.1 技术栈

| 项 | 选择 |
|---|---|
| 构建 | Vite + **yarn**（4.x Berry）。`web/.yarnrc.yml` 设 `nodeLinker: node-modules`，避免 PnP 与 Vite、shadcn-vue 的兼容问题；`package.json` 写 `packageManager` 字段固定版本。脚本：`yarn wasm`（调用 `scripts/build-wasm.sh`）、`yarn dev`、`yarn build`、`yarn preview`、`yarn test` |
| 框架 | Vue 3 + TypeScript（`<script setup>`） |
| UI | **shadcn-vue**（Tailwind + Reka UI），按需引入：Tabs、Button、Select、Slider、Switch、Textarea、Badge、Card、Separator、ScrollArea、Sonner（Toast）、Tooltip |
| 样式 | Tailwind，主题用 shadcn 的 `neutral` 基底，**只用黑白灰**，不引入强调色 |
| WASM 运行方式 | `Web Worker`：UI 线程不被阻塞，请求带自增 id，只采用最新一次结果 |
| 编辑器 | 首版用原生 `<textarea>`（等宽字体），后续按需换 CodeMirror 6 |

TypeScript 约定：对象类型一律用 `interface`，不用 `type`（用户规则）。

### 3.2 视觉风格（黑白）

- 默认深色：背景 `#000`，文字 `#fff`，边框 1px `#fff/20`；可切换纯白底黑字。
- 无圆角阴影渐变，圆角取 0 或 2px；字体全站等宽（如 `JetBrains Mono`，系统等宽兜底）。
- 预览区背景用灰度棋盘格，让透明像素可见；图片使用 `image-rendering: pixelated`。
- 状态靠文字和边框表达，不用红绿色：错误用反色块（白底黑字）加 `ERR` 前缀。

### 3.3 布局

```
┌──────────────────────────────────────────────────────────────┐
│ PIXELGAMES / PLAYGROUND                    [Dark|Light] [GitHub]│
├──────────────────────────────────────────────────────────────┤
│ Examples: [1 ASCII] [2 Anim] [3 JSON] [4 Effect]              │
├───────────────────────────┬──────────────────────────────────┤
│ INPUT                     │ PREVIEW                          │
│ [ASCII | Spec JSON | Auto]│  ┌────────────────────────────┐  │
│ ┌───────────────────────┐ │  │                            │  │
│ │ :: base               │ │  │     (棋盘格 + 像素图)       │  │
│ │ ..##..                │ │  │                            │  │
│ │ .#$$#.                │ │  └────────────────────────────┘  │
│ │ ...                   │ │  64x64 · 2 frames · 8 fps · 1.2KB │
│ └───────────────────────┘ │  [Download PNG] [Download GIF]   │
│ Palette [p16 ▾]  Seed [1] │  [Copy hash]                     │
│ Anim [auto ▾]  Scale ●──  │                                  │
│ FPS (只读，取自规格)       │                                  │
├───────────────────────────┴──────────────────────────────────┤
│ STATUS: ok · render 4ms · warnings: 0                         │
└──────────────────────────────────────────────────────────────┘
```

- 窄屏上下堆叠（输入在上，预览在下）。
- 输入变化后 250 ms 防抖自动渲染，另有 `Render` 按钮和 `Ctrl/Cmd+Enter` 快捷键。
- 错误面板显示 `code`、`message`、`path`、`hint`；如能从 `path` 或消息中得到行号，再高亮输入框对应行（后续）。
- 下载文件名：`<id>.png` / `<id>.gif`。
- 输入内容与选项写入 URL hash（压缩后），方便分享。

### 3.4 目录结构

```
web/
├─ package.json, yarn.lock, .yarnrc.yml, vite.config.ts, tsconfig.json, components.json
├─ index.html
└─ src/
   ├─ main.ts, App.vue
   ├─ styles/globals.css         # Tailwind + 黑白主题变量
   ├─ components/ui/             # shadcn-vue 生成
   ├─ components/
   │   ├─ InputPanel.vue         # 输入类型、textarea、选项
   │   ├─ PreviewPanel.vue       # 棋盘格背景、下载按钮、统计
   │   ├─ ExampleBar.vue         # 示例切换
   │   └─ ErrorBlock.vue
   ├─ lib/
   │   ├─ pxl.worker.ts          # 加载 wasm，处理 render/inspect 消息
   │   ├─ pxl-client.ts          # 主线程封装：Promise API + 过期结果丢弃
   │   └─ examples.ts            # 4 个示例数据
   ├─ wasm/                      # wasm-bindgen 产物（gitignore）
   └─ types.ts                   # RenderOptions / RenderResult 等 interface
```

---

## 4. 输出 GIF 还缺什么（对 `crates/` 的检查结论）

### 4.1 已经具备（不用再做）

读 `crates/pxl-export/src/gif.rs` 的结论：

- `encode_gif_timed(frames, palette, scale, delays_cs, looped)`：逐帧延迟、可选循环（不写 NETSCAPE 块即只播一次），有单测。
- `gif_delays_cs(fps, n)`：把四舍五入误差摊到各帧，总时长不漂移，有单测。
- 透明：全局调色板 + 透明索引 0 + `DisposalMethod::Background`，逐帧完整重画，没有叠帧残影。
- 最近邻放大，`scale_nearest` 限制 2048 像素，帧尺寸一致性检查。
- 输出是内存 `Vec<u8>`，不碰文件系统；`gif` 和 `png` crate 已验证能编到 wasm32。
- 动画数据：`Anim { fps, looped, frames }`、12 个动画宏（`bob`、`blink`、`pulse_glow`、`spin_projectile`、`death_dissolve`、`walk_side` 等）和 ASCII 规格里的 `anims` 都已存在。

### 4.2 缺口

| 编号 | 优先级 | 缺口 | 说明与方案 |
|---|---|---|---|
| **G1** | 必须 | **组装链不在库里** | 见 1.3。ASCII/JSON → `PxlDoc` → 动画 → 帧序列的代码在 `pxl-cli`，且读文件。方案：抽到 `pxl-engine` + `Resolver`（2.1）。这是整个计划里最大的一块工作。 |
| **G2** | 必须 | **“一键出图”的选择规则** | CLI 的 `render` 要用户显式给 `--anim`，没给就只出第一帧的 PNG。网页需要明确规则：<br>① 选了 `Auto`：规格里有动画则输出 GIF，否则输出 PNG。<br>② 有多个动画：默认选第一个（按名字排序，或优先 `idle`），并让用户用下拉框切换（`inspect` 提供列表）。<br>③ 用户明确要 GIF 但规格没有动画：返回清晰错误，提示“添加 `anims` 或使用宏”。不要静默出单帧 GIF。 |
| **G3** | 必须 | **动画限制和浏览器延迟** | `check_anim_timing`（fps 1..=60、帧数 1..=16）要随组装链迁入 `pxl-engine`，保证网页和 CLI 一致。另外，GIF 延迟单位是 1/100 秒，`gif_delays_cs` 在 fps 很高时会产出 1cs。许多浏览器会把过小的延迟当成 10cs 播放【依据常见浏览器行为，本仓库未验证】。建议：网页上限先取 **fps ≤ 50**（延迟 ≥ 2cs）并给出 warning，或在 `pxl-engine` 里对 web 场景 clamp 到 2cs。需要在真实浏览器里实测后定。 |
| **G4** | 建议 | **背景色选项** | `encode_gif_timed` 目前只输出透明背景。`render_preview_png` 则固定把透明变成深色底（用于 CLI 预览）。网页需要：`Transparent`（默认，PNG 用 `encode_indexed_png` 保留透明，体积也小）和 `Solid(color)`（导出到不支持透明的场景）。实现：给 GIF 和 PNG 路径加一个可选的合成步骤，不改现有函数签名，新增 `encode_gif_with_bg`。 |
| **G5** | 建议 | **GIF 体积优化** | 现在每帧都是全幅完整图，没有帧间差分和局部调色板。对 ≤64×64、≤16 帧的小图，LZW 对大色块压缩得很好，首版可以忽略；放大到 scale 16 后体积大小【未验证】。留作 P2：在体积实测超标（比如 >500 KB）时再做“与上一帧不同的矩形区域 + 透明保留”。 |
| **G6** | 建议 | **`gif` crate 默认特性** | 根 `Cargo.toml` 使用 `gif = "0.13"` 的默认特性，默认带 `color_quant`（NeuQuant，给 RGB 输入量化用）。我们只写索引色，用不到。关闭默认特性（`default-features = false, features = ["std"]`）可能减小 wasm 体积【未验证，需先确认编译通过再度量】。 |
| **G7** | 建议 | **跨端一致性测试** | 目标：同一输入，wasm 与原生 CLI 输出的字节哈希相同。当前黄金测试只覆盖原生。新增：用 `wasm-bindgen --target nodejs` 或 `wasm-bindgen-test-runner`（已安装）在 node 里渲染 4 个示例，与 `pxl` CLI 的 `hash` 比对。 |

### 4.3 明确不需要

- 不需要 `image` crate、不需要量化器：素材本来就是调色板索引色，最多 256 色。
- 不需要 APNG / WebP 动图：需求只要 GIF 和 PNG。
- 不需要改 `pxl-core` / `pxl-gen` / `pxl-ops` 的行为。

---

## 5. 四个示例

示例数据来自 `examples/ascii/*` 与 `examples/tank/*`，实施时逐个用 CLI 先跑一遍确认输出，再固化到 `web/src/lib/examples.ts`。

| # | 名称 | 输入类型 | 内容 | 输出 |
|---|---|---|---|---|
| 1 | **ASCII 宝石** | ASCII 文本（`printable`） | `examples/ascii/gem/gem.ascii.txt`：6×6 的 `.#$` 字符画，调色板 p16，图例 `# → outline`、`$ → 14` | PNG（静态） |
| 2 | **ASCII 史莱姆（眨眼动画）** | ASCII 多帧文本，`:: base` / `:: blink` 两块 | `examples/ascii/slime`：两帧加规格里的 `anims`，需要用到 `legend_ref` 的共享图例，放进 `files` 内存表 | GIF（idle，fps 4） |
| 3 | **规格 JSON：坦克车身** | `shapes` 规格 JSON | `examples/tank/tank_body.shapes.json`：16×16、`mirror: x`、`rect` + `pattern: stripe_y:4`；调色板用内置 `tank` | PNG |
| 4 | **动效生成：宏** | `*.ascii.json` 规格，`anims` 里用 `macro` | 以 gem 或 slime 为底，加 `{"name":"pulse","fps":8,"loop":true,"macro":"pulse_glow(...)"}` 或 `bob(amp=1)`；宏参数以 `pxl list macros` 为准 | GIF（宏生成帧） |

补充说明：

- 示例 1 和 2 展示 ASCII 输入；示例 3 展示规格 JSON；示例 4 展示动效生成，覆盖了“JSON、ASCII、动效”三类。
- 示例 2 与 4 依赖“多文件引用”（`legend_ref`、调色板）。网页里 `files` 以内存表提供，用户的单个输入框里不需要写这些，由示例预置。**用户自己写 ASCII 时用 `palette` 下拉选择内置调色板，并使用 `printable` 字符集或 `digits` 字符集的自动图例，不需要外部文件。**
- 示例 4 的宏名和参数【未验证】：实施时先 `pxl list macros` 对照。

---

## 6. 里程碑

| 阶段 | 内容 | 验收 | 预估【估】 |
|---|---|---|---|
| **P0** | 可行性验证 | ✅ 已完成：6 个底层 crate 通过 `cargo check --target wasm32-unknown-unknown` | 0 |
| **P1** | ✅ **已完成（2026-10-23）**：抽 `pxl-engine`（`Resolver`、`build_spec`、`build_parts`、`frames_for_recipe`、`check_anim_timing`、`render_doc_bytes`、`render/inspect` 入口）；`pxl-cli` 改调库 | 全工作区 `cargo test` 全绿（含 CLI 65 项、golden 1 项，哈希不变）；`pxl-engine` 无 `std::fs` / `std::io`，并已通过 `cargo check --target wasm32-unknown-unknown`；新增 14 项示例测试 | 1.5–2 天 |
| **P2** | ✅ **核心已完成（2026-10-23）**：`pxl-wasm`（导出 `render` / `inspect` / `version` / `maxInputBytes`）、`scripts/build-wasm.sh`、G2 自动选择规则。**未做**：G4 背景色选项；G3 的 fps 浏览器实测（并入 P5） | node 下 `render` 三个示例出图成功，wasm 输出与原生 CLI 逐字节相同（gem：262488 字节，`cmp` 一致）；wasm **755 KiB**（gzip 后 256 KiB），目标 ≤400 KB 未达到，见下方“体积记录” | 1 天 |
| **P3** | `web/` 脚手架：Vite + Vue + Tailwind + shadcn-vue + 黑白主题 + Worker 调用 | `yarn install && yarn wasm && yarn dev` 能调用 wasm 出图 | 0.5–1 天 |
| **P4** | UI：输入面板、预览面板、下载、错误面板、URL 分享、4 个示例 | 4 个示例都能一键运行并下载 | 1–1.5 天 |
| **P5** | 测试与度量：G7 跨端哈希比对；vitest 对 `pxl-client`；浏览器实测 GIF 延迟（G3）、体积（G5/G6） | 4 个示例哈希与 CLI 一致；记录结论后决定是否做 G5/G6 | 0.5–1 天 |
| **P6** | 本地构建与运行：`yarn build` 产出 `web/dist`，`yarn preview` 本地预览；根目录 `web/README.md` 写明“前置工具 → 一条命令构建 → 访问地址”；`vite.config.ts` 的 `base` 用默认 `/`，并确认 `.wasm` 以 `application/wasm` 提供 | 在一台干净机器上按 README 的命令序列能跑起来；`dist` 用任意静态服务器打开（`yarn preview`）可出图 | 0.5 天 |

依赖顺序：P1 → P2 → P3 → P4 → P5 → P6。P3 的页面骨架可以在 P2 完成前用 mock 数据并行做。

### 实施记录（P1–P2）

**体积记录**（`scripts/build-wasm.sh`，`opt-level=z`、LTO、`panic=abort`、`strip=symbols`，未装 `wasm-opt`）：

| 阶段 | wasm 大小 |
|---|---|
| 首次构建（未 strip，带 name section 388 KB） | 1137 KiB |
| `strip=symbols` | **755 KiB**（gzip 256 KiB） |

代码段 669 KiB，数据段 83 KiB。体积主要来自 `serde_json`、`schemars`（`pxl-format` 的 JSON Schema 派生）、`png`/`flate2`/`gif`。本地运行不受体积影响，暂不优化；若要压，优先顺序：装 `wasm-opt -Oz` → 给 `pxl-format` 加 feature 关闭 `schemars` → 关闭 `gif` 默认特性（G6）。

**实现中与原计划的偏差：**

1. **`Resolver` 的签名**：`read_text(name, kind) -> PxlResult<Option<String>>`，多了 `ResourceKind`。原因：CLI 原先对调色板多查一个“同目录同名文件”的候选，legend 不查，要保持行为不变。
2. **入口放在 `pxl-engine`**：`render` / `inspect` / `InputKind` 检测都在 `pxl-engine::session`，`pxl-wasm` 只做类型转换，可在原生环境直接测试。
3. **ASCII 文本自动选字符集**：页面用户不该知道 `--charset`。含非数字字符时用 `printable`，否则 `digits`；未选调色板时默认 `p16`。CLI 行为不变（仍需显式 `--charset`）。
4. **G2 规则落地**：`Auto` 下有动画则 GIF（优先名为 `idle` 的，否则按名字排序第一个），没有动画则 PNG；要求 GIF 但无动画返回 `E_SEMANTIC`。每个示例的输出类型都有测试。
5. **选项通过 JSON 字符串传入**，结果用 `RenderResult { format, bytes, metaJson }`，不引入 `serde-wasm-bindgen`。错误以 `{code,message,path,hint}` 对象抛出。选项里有未知键会被拒绝（防止拼写错误被悄悄忽略）。
6. **示例 4 的宏**已用 `pxl list macros` 核实：`idle_breathe(period=4, amp=1)`（4 帧）可用。
7. 根 `Cargo.toml` 加入 `pxl-engine`、`pxl-wasm` 成员；`.gitignore` 加入 `/web/node_modules/`、`/web/dist/`、`/web/src/wasm/`。

本地一条龙（P6 完成后的目标命令）：

```
cd web
yarn install
yarn wasm      # cargo build + wasm-bindgen，产物写入 src/wasm/
yarn build     # 产出 dist/
yarn preview   # 本地访问
```

---

## 7. 风险与应对

| 风险 | 影响 | 应对 |
|---|---|---|
| 抽 `pxl-engine` 时改变 CLI 输出 | 破坏黄金测试 | 先搬运后重构；每搬一块就跑 `cargo test`；哈希不变是硬门槛 |
| `wasm-bindgen` crate 与已装 CLI 版本不一致 | 生成胶水代码时报 schema 版本错误 | `wasm-bindgen = "=0.2.129"`；脚本里先比对 `wasm-bindgen --version` |
| 根 workspace 带 `[patch.crates-io] miniquad` 和 battle-city 成员 | wasm 构建被无关成员拖慢或失败 | 始终 `-p pxl-wasm`；如仍有问题，把 web 用的 crate 放进独立 workspace（备选，需另议） |
| wasm 体积偏大（`schemars` 带来宏与 JSON Schema 代码） | 首次加载慢 | P2 实测；必要时给 `pxl-format` 加 feature 关闭 `schemars` 的 derive |
| 输入太大/太慢卡住页面 | 体验 | 在 Worker 里跑；输入长度和画布尺寸沿用现有上限（画布 ≤64，scale ≤16，缩放后 ≤2048，动画 ≤16 帧）；超时则提示 |
| GIF 在不同浏览器延迟表现不同 | 动画速度失真 | G3：实测后固定上限并在界面标注实际 fps |
| 用户填多文件规格（recipe、`legend_ref`）时找不到引用 | 报错 | 首版只支持单输入框 + 内置调色板；错误信息提示“网页版暂不支持外部文件引用”；recipe 支持放到后续 |

---

## 8. 范围边界

**首版包含**：ASCII 文本、`*.ascii.json`、`*.shapes.json`、`*.pxl.json`（PxlDoc）四类输入；PNG / GIF 输出；内置调色板（p16、tank）；缩放、种子、动画选择；4 个示例；下载与分享链接。

**首版不含**：recipe/sheet 多文件构建、QA（`pxl-qa`）面板、`parts` 骨架、用户上传调色板、编辑器语法高亮与 Schema 提示、账号与后端、AI 生成。以上都留作 v2.x。

---

## 9. 已确认决策与影响

| 决策 | 影响 |
|---|---|
| 规格 JSON 取默认范围（ascii.json / shapes.json / PxlDoc） | 首版不处理 recipe 与多文件引用；示例所需的 `legend_ref` 由示例预置的内存文件表提供 |
| 只在本地运行，yarn 构建 | 去掉 GitHub Pages / Cloudflare / CI 发布；构建入口统一为 `yarn` 脚本；不需要处理 `base` 子路径与 CDN 缓存 |
| 抽出 `pxl-engine`，重构 `pxl-cli` | P1 是关键路径；以 golden 哈希不变为硬门槛；wasm 与 CLI 共用同一份组装逻辑，不保留第二套实现 |

仍待实施时核实的项（不阻塞开工）：示例 4 的宏参数、浏览器 GIF 小延迟行为（G3）、wasm 体积与 `gif` 默认特性（G6）。
