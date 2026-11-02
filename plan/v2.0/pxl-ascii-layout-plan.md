# pxl ASCII 布局输入计划：ASCII → JSON → 图片

日期：2026-10-23（同日修订：字符集默认收窄到数字，画布限定 32/64）。性质：**新能力计划**。状态：**A1–A5 已实现**（同日），P2 项未做；实施记录见第 10 节。

## 0. 决策记录

| 编号 | 决策 | 说明 |
|---|---|---|
| D1 | **字符集默认只允许数字 `0-9`**（`charset: "digits"`） | 默认 `0` 透明；不写 legend 时，数字直接作为调色板索引，与现有 `PxlDoc` 矩阵的 `0-9` 含义一致。写了 legend 时，可以把数字重映射到任意颜色、图案或印章。需要多于 10 种符号时，显式切换到 `charset: "printable"`（可打印 ASCII 0x20–0x7E），或者用印章表达细节 |
| D2 | **画布限定为 32×32 或 64×64 的正方形**（`size: 32 \| 64`，默认 32） | 布局展开后的像素尺寸（列数 × `cell.w`，行数 × `cell.h`）必须 ≤ `size`。比画布小时，按 `anchor` 和 `offset` 放进画布，其余区域透明。16×16、非正方形等其他尺寸不支持，仍然走 shapes 或 bollinger |
| D3 | 其他项按初稿默认 | 中间 JSON 是唯一真值；编译产物就是 `PxlDoc`；hex 颜色不就近量化；36 色和 16 帧上限不变；构建配置里的动画与 ASCII JSON 里的动画不能同时出现 |

## 1. 诉求与边界

**诉求**：用一段 ASCII（默认是数字字符画）快速描述像素布局，一个字符对应一个像素或一个 N×M 像素块，然后走统一的三段式流程：

```
ASCII 文本（只描述布局，默认只含 0-9）
  └─ pxl ascii import ─▶ *.ascii.json（布局 + 颜色 legend + 帧/动效 + 块尺寸 + 画布 32/64）  ← 人或 Agent 在这一层补颜色和动效
        └─ pxl gen ascii ─▶ *.pxl.json（现有 PxlDoc，w=h=size，可选附带生成的 *.palette.json）
              └─ pxl render / sheet / build / validate ─▶ PNG / GIF / sheet（全部复用现有链路）
```

**原则**

1. **ASCII 只表达布局**。颜色、块尺寸、画布、动效、调色板都不写在 ASCII 里，统一放在中间 JSON；同一份 ASCII 可以换 legend 或调色板重新出图。
2. **中间 JSON 是唯一真值**，它可以落盘、审阅、diff，也可以作为构建配置（recipe）的输入。`pxl ascii render` 这类一步到位的快捷命令内部也走同一个 JSON，不另开旁路。
3. **编译产物就是现有的 `PxlDoc`**。render、sheet、build、validate 和黄金测试都不需要知道 ASCII 的存在。
4. **确定性**：同样的输入（ASCII、legend、palette）必须得到字节一致的 JSON 和 PNG，纳入黄金测试。

**非目标**

- 不做"图片转 ASCII"，也不做把文字渲染成像素字体。
- 不支持 Unicode、全角字符或 emoji。默认只接受 `0-9`，`printable` 模式也只接受 0x20–0x7E。
- 不支持 32、64 以外的画布，也不支持非正方形画布。
- 不改 `PxlDoc` 文本矩阵格式（`.`、`0-9a-z`），不改 `MAX_DIM = 64`、`MAX_ANIM_FRAMES = 16`、36 色上限，不给 `PxlDoc` 增加图层概念（图层在编译时压平）。
- 不在本计划内接 harness 工具或 MCP（列为 P2 后续项）。

## 2. 现状（已核实）

| 事实 | 位置 | 对本计划的含义 |
|---|---|---|
| `PxlDoc.frames: BTreeMap<String, Vec<String>>`，每帧是一组文本行，字符写死为 `.`=0、`0-9`=0–9、`a-z`/`A-Z`=10–35 | `crates/pxl-format/src/model.rs:212`，`crates/pxl-core/src/text.rs` 的 `decode_index` | 数字模式不写 legend 时与现有矩阵字符语义一致。缺的是**可自定义的 legend** 和**画布放置**。 |
| `ShapesFile` 的 `Pixels{pixels, x, y, mirror}` 印章、`patch_pixels` op、`PartsFile` 的 `SpriteSrc::Rows` 都用逐行字符串，但都只接受标准索引字符 | `model.rs:376`、`crates/pxl-format/src/ops.rs:56`、`crates/pxl-format/src/parts.rs:73`、`crates/pxl-gen/src/shapes.rs:132` | 这些可作为"块印章"和"差量帧"的执行层复用，不需要重写。 |
| `TemplateFile.legend` 只是说明文字，解析时不使用，cells 硬编码为 0–5 | `crates/pxl-gen/src/bollinger.rs:53` | 不能扩展它来实现 ASCII legend，语义不同。 |
| `pxl gen` 按 generator 分派，产出 `IndexFrame` 后由 `doc_from_frames` 组装成文档，只写 `base` 等少量帧，不写 anims | `crates/pxl-cli/src/cmd/generate.rs` | `gen ascii` 需要多帧和 anims，要单独组装 `PxlDoc`。 |
| 构建配置里 `RecipeBase.gen` 的分派在 `build_sprite`，目前支持 `shapes`、`bollinger`、`parts` | `crates/pxl-cli/src/cmd/sprite.rs:36`、`crates/pxl-format/src/recipe.rs:11` | 新增 `gen = "ascii"` 分支即可接入 build、sheet、QA 和 manifest。 |
| 动画宏 `expand_macro` 有 12 个宏（`bob`、`blink`、`pulse_glow` 等）；`Op` DSL 是 serde tag 写法 | `crates/pxl-ops/src/macros/{mod,catalog}.rs` | ASCII 动效可以直接引用宏和 op，不新增动画语义。 |
| `Meta` 的可选字段都带 `skip_serializing_if = "Option::is_none"` | `model.rs:120` | 新增 `meta.ascii` 不会让现有黄金测试的哈希变化。 |
| `pxl schema` 通过 `SCHEMA_KINDS` 和 `schema_json` 分支注册；`pxl list generators` 的列表写死 | `crates/pxl-format/src/schema.rs`、`crates/pxl-cli/src/cmd/catalog.rs:28` | 需要新增 `ascii` 和 `legend` 两种 schema，并把 `ascii` 加进 generators 列表。 |

## 3. ASCII 文本格式（第一段输入）

文件建议用 `*.ascii.txt`。规则如下：

- **一个字符对应一个格子**，格子大小由 JSON 的 `cell` 决定（默认 1×1，即一个像素）。
- **字符集**：
  - `digits`（默认）：只允许 `0-9`，`0` 默认透明。空格、`.`、字母、Tab、非 ASCII 字符都报 `ascii.bad_char`，并给出 `file:line:col`。
  - `printable`（通过 `--charset printable` 或 JSON 的 `charset` 开启）：允许 0x20–0x7E，`.` 和空格默认透明。
- **块**：一组连续的非空行组成一个块，块之间用空行分隔。
- **指令行**：只认 `^:: <name>( frame| layer)?$` 这种格式（`name` 匹配 `[a-z_][a-z0-9_]*`），用来给下一个块命名或指定类型。
  - 数字模式下 `:` 不是像素字符，所以不会和画面内容冲突。
  - printable 模式下，`::` 开头但不符合格式的行报错，不当作像素；可以用 `--no-directives` 关闭指令解析。
- **其他校验**：
  - CRLF 自动归一为 LF，并去掉 UTF-8 BOM。
  - 行宽不一致默认报错（`ascii.ragged_row`）。加 `--pad` 时在右侧补透明字符（数字模式补 `0`），并给出警告。
  - 数字模式不使用空格表示透明，所以编辑器删掉行尾空格不会丢失透明列。
- **尺寸约束**：块的列数 × `cell.w` 和行数 × `cell.h` 都必须 ≤ `size`（32 或 64），否则报 `ascii.size_overflow`，hint 中给出能放下的最大 `cell` 或建议改用 `size: 64`。

示例 `slime.ascii.txt`（两帧；`0` 透明，`1` 描边，`2` 身体，`3` 眼睛，`4` 闭眼）：

```
:: base
00111100
01222210
12322321
12222221
01111110

:: blink
00111100
01222210
12422421
12222221
01111110
```

导入时的块类型：

- 默认 `--blocks frames`，每个块成为一帧。
- `--blocks layers` 时每个块成为一个图层，再生成一帧 `base`，按图层顺序合成。
- 指令后缀 `frame` 或 `layer` 可以对单个块覆盖默认类型。

## 4. 中间 JSON：`*.ascii.json`（AsciiSpec）

### 4.1 结构（pxl-format，serde + schemars）

```rust
pub struct AsciiSpec {
    pub v: u32,                               // 固定 1
    pub charset: Charset,                     // digits（默认）| printable
    pub size: u16,                            // 画布边长，只能是 32 或 64，默认 32；PxlDoc 的 w = h = size
    pub cell: CellSize,                       // [w, h]，默认 [1, 1]；一个字符 = w×h 像素
    pub anchor: Anchor,                       // center（默认）| bottom-center | top-left
    pub offset: [i16; 2],                     // 在 anchor 位置基础上的像素偏移，默认 [0, 0]
    pub palette: Option<String>,              // 内置 id 或 *.palette.json；不填则由 legend 的 hex 生成调色板
    pub palette_mode: PaletteMode,            // strict（默认）| extend
    pub legend_ref: Option<String>,           // 共享的 *.legend.json，本文件的 legend 覆盖同名键
    pub legend: BTreeMap<String, LegendValue>,// 键必须是单个字符，且属于当前 charset
    pub keep: char,                           // 差量 patch 里表示"不改动"的字符；digits 模式默认 '.'，printable 模式默认 '_'；不得出现在 legend 中
    pub layers: Vec<AsciiLayer>,              // 可选；数组顺序就是从下到上的绘制顺序
    pub frames: BTreeMap<String, AsciiFrame>, // 为空时等价于 { "base": { "layers": <全部可见图层> } }
    pub anims: Vec<AsciiAnim>,
    pub post: Vec<Op>,                        // 放进画布后对每一帧执行，例如 outline
    pub mirror: Option<String>,               // 复用 Mirror::parse
    pub kind: Option<String>,                 // sprite|tiles|effect|ui|overlay，沿用 KINDS
    pub source: Option<AsciiSource>,          // { file, sha256, blocks: [{name, line}] }，用于回溯和报错定位
}

pub struct AsciiLayer { pub name: String, pub rows: Vec<String>, pub x: i16, pub y: i16, pub visible: bool } // x、y 以格子为单位

#[serde(untagged)]
pub enum AsciiFrame {
    Rows   { rows: Vec<String> },                                 // 直接写布局
    Layers { layers: Vec<String> },                               // 按给定顺序合成图层
    Delta  { from: String, patch: Vec<AsciiPatch>, ops: Vec<Op> },// 以某帧为底做局部改动，或执行 op
}
pub struct AsciiPatch { pub x: u16, pub y: u16, pub rows: Vec<String> } // 坐标以格子为单位，keep 字符表示不改动

pub struct AsciiAnim {
    pub name: String, pub fps: u32, pub looped: bool /* JSON 键为 loop */, pub dirs: Vec<String>,
    pub mode: AnimMode,
    pub frames: Vec<String>,          // 逐帧翻页：引用 frames 里的帧名
    pub macro_call: Option<String>,   // JSON 键为 macro，例如 "bob(amp=1)"，作用于 from 帧
    pub ops: Vec<Vec<Op>>,            // 显式写出的每帧 op 列表，作用于 from 帧
    pub from: Option<String>,         // macro 或 ops 的源帧，默认 base
}
```

约束：

- 动画的 `frames`、`macro`、`ops` 三者必须且只能有一个非空，否则报 `E_SCHEMA`。
- `size` 不是 32 或 64 时报 `E_SCHEMA`（`ascii.bad_size`）。
- 所有帧共用同一个 `size`、`cell`、`anchor`、`offset`，保证动画帧对齐。

### 4.2 画布放置

1. 先把格子矩阵按 `cell` 展开成布局图，尺寸为 `lw × lh`。
2. 再按 `anchor` 求左上角坐标：
   - `center`：`((size - lw) / 2, (size - lh) / 2)`，整数向下取整。
   - `bottom-center`：`((size - lw) / 2, size - lh)`。适合角色或坦克这类需要"脚底对齐"的素材。
   - `top-left`：`(0, 0)`。适合 tile 和 UI。
3. 最后加上 `offset`。布局超出画布时报 `ascii.offset_out_of_bounds`，不裁剪。
4. 画布中未被覆盖的区域填索引 0（透明）。
5. `pxl ascii import --cell fit` 会求出能放下布局的最大整数倍数 `floor(size / max(cols, rows))`，并把具体数值写进 JSON（JSON 里不保存 `fit`）。例如 8×5 的 slime 在 `size: 32` 下得到 `cell: [4, 4]`，即 32×20 的布局，居中后上下各留 6 行透明。

### 4.3 Legend：字符到颜色

**数字模式的默认映射（不写 legend 时）**：`0` 对应 `special.transparent`，`1-9` 直接对应调色板索引 1–9。这时必须指定 `palette`，用到的数字超出调色板长度时报错。写了 legend 的数字按 legend 解析，没写的数字仍按默认映射。

`LegendValue` 支持简写和完整写法（untagged）：

| 写法 | 含义 |
|---|---|
| `12` | 调色板索引 12（JSON 里的索引不受 0–9 限制，最大 35） |
| `"#38b764"` / `"#38b764ff"` | 直接指定颜色（见 4.4 调色板解析） |
| `"transparent"`、`"outline"`、`"flash"` | 角色，映射到 `special.transparent/outline/flash` |
| `{ "ramp": "skin", "step": 2 }` | 调色板 ramp 的第 2 档 |
| `{ "index": 7, "pattern": "checker", "alt": 8 }` | 块级图案，只在 `cell > 1×1` 时有意义，取值 `solid`、`checker`、`stripe_x`、`stripe_y`、`bevel`。`bevel` 用 ramp 相邻档做左上亮、右下暗 |
| `{ "stamp": ["44444445", "..."] }` | **块印章**：一个字符展开成 cell 尺寸的小图，内容用标准索引矩阵（`.`/`0-9a-z`，不递归引用 legend），尺寸必须等于 cell。适合砖块、地砖这类重复块 |
| 任意完整写法加 `"auto": true` | 表示这是导入时自动分配的颜色，未经人工确认 |
| 任意完整写法加 `"note": "..."` | 说明文字，不参与编译 |

完整写法里 `index`、`color`、`role`、`ramp`、`stamp` 必须恰好出现一个。

数字模式下一份布局最多 10 种符号，不够用时有三种办法：

1. 给符号配图案或印章。
2. 用多个图层分别映射（各图层可以有自己的 legend，P2）。
3. 显式切到 `printable`。

### 4.4 调色板解析

1. **指定了 `palette` 且 `palette_mode=strict`**：hex 颜色必须和调色板中某个颜色精确相等（统一规整成 `#rrggbbaa` 比较），否则报 `E_SEMANTIC`，hint 给出最近的索引及其色值。不做就近量化。
2. **指定了 `palette` 且 `palette_mode=extend`**：调色板里没有的 hex 按 legend 键的排序依次追加，写出 `<out_stem>.palette.json`（`id` 为 `<stem>`，保留原有 ramps、cycles、special），文档的 `palette` 指向新文件。追加后超过 36 色报错。
3. **没有指定 `palette`**：legend 必须覆盖所有用到的字符，且只能使用 hex 和角色。生成一个新调色板：
   - 索引 0 为透明。
   - 用到 `outline` 角色时，索引 1 为 outline（默认 `#000000ff`，可以用 `{"role": "outline", "color": "#1d2b53"}` 覆盖）。
   - 其余颜色按 legend 键的字符码顺序分配。
4. **可选 `legend_ref`**：例如 `battle_city.legend.json`，结构是 `{ v, charset, palette, legend }`，`charset` 必须和引用它的 spec 一致。这让同一个游戏的多份 ASCII 共用一套"数字 → 颜色"约定，这是"快速布局"的关键：画新素材时只写数字。

### 4.5 示例

由第 3 节的 ASCII 导入，再人工补充 legend 和动效：

```json
{
  "v": 1,
  "charset": "digits",
  "size": 32,
  "cell": [4, 4],
  "anchor": "bottom-center",
  "palette": "p16",
  "palette_mode": "strict",
  "legend": {
    "0": "transparent",
    "1": "outline",
    "2": 11,
    "3": 1,
    "4": { "index": 3, "auto": true }
  },
  "frames": {
    "base":  { "rows": ["00111100", "01222210", "12322321", "12222221", "01111110"] },
    "blink": { "from": "base", "patch": [{ "x": 2, "y": 2, "rows": ["4..4"] }] }
  },
  "anims": [
    { "name": "idle", "fps": 4, "loop": true, "frames": ["base", "base", "base", "blink"] },
    { "name": "hop",  "fps": 8, "loop": true, "macro": "bob(amp=1)" }
  ],
  "kind": "sprite",
  "source": { "file": "slime.ascii.txt", "sha256": "<digest>", "blocks": [{ "name": "base", "line": 1 }, { "name": "blink", "line": 8 }] }
}
```

- `import` 默认把每个块写成完整的 `rows`。上面的 `blink` 是人工改写的差量写法，两种写法编译结果一致。
- 编译结果是 32×32 的 `PxlDoc`，史莱姆占 32×20，贴底放置。

块示例（砖墙 tile：4×4 个格子，每格 8×8 像素，正好 32×32）：

```json
{
  "v": 1, "size": 32, "cell": [8, 8], "anchor": "top-left", "palette": "tank", "kind": "tiles",
  "legend": {
    "0": "transparent",
    "1": { "stamp": ["44444445", "44444445", "44444445", "55555555",
                     "44454444", "44454444", "44454444", "55555555"] },
    "2": { "ramp": "steel", "step": 1, "pattern": "bevel" }
  },
  "frames": { "base": { "rows": ["1111", "1221", "1221", "1111"] } }
}
```

## 5. 编译语义（pxl-gen）

`compile_ascii(spec: &AsciiSpec, palette: &Palette) -> PxlResult<AsciiCompiled>`，产出：

```rust
pub struct AsciiCompiled {
    pub frames: Vec<(String, IndexFrame)>, // 按帧名排序，每帧都是 size×size
    pub palette: Option<PaletteFile>,      // 只有 extend 或自动生成调色板时才有
    pub warnings: Vec<AsciiLint>,          // { id, path, message }
}
```

执行顺序固定：

1. 校验 `charset`、`size`、`keep`，以及所有布局字符都属于当前 charset。
2. 合并 legend：`legend_ref` 在下，本文件 legend 覆盖，没写的数字补默认映射。然后把每个字符解析成 `CellPaint`（Index、Pattern 或 Stamp）。
3. 对每帧求格子矩阵：
   - `Rows`：直接使用。
   - `Layers`：按顺序叠加，透明字符不覆盖，`x`、`y` 按格子偏移。
   - `Delta`：先递归求 `from` 帧（检测循环引用并报错），再应用 patch：`keep` 字符不改动，透明字符擦除。
4. 按 `cell` 展开成布局图：solid 直接填充，pattern 或 bevel 按图案填充，stamp 拷贝印章内容。
5. 按第 4.2 节放进 `size × size` 画布。
6. 依次执行 `mirror` 和 `post` ops（复用 `pxl_ops::apply_ops`，outline 走 `paint_outline`）。差量帧里的 `ops` 在 patch 之后、`post` 之前执行。所有 op 都作用于整张画布。
7. 校验所有帧都是 `size × size`，所用索引都在调色板范围内，调色板满足 `check_text_encodable`。

动画在 CLI 层组装成 `PxlDoc.anims`：

- 逐帧翻页：直接把帧名写进 `AnimFrames::Seq`。
- `macro` 和 `ops`：沿用 `build_sprite` 的做法，对 `from` 帧调用 `expand_macro` 或 `apply_ops` 生成 `<anim>_<i>` 帧。
- 有 `dirs` 时按方向生成 `ByDir`，各方向的源帧用 `<from>_<dir>` 约定查找，找不到就报错，不偷偷回退到 base。
- 帧数超过 `MAX_ANIM_FRAMES` 报错。

**编译期 lint**（id 稳定，便于 Agent 消费）：

| id | 级别 | 触发条件 |
|---|---|---|
| `ascii.bad_char` | 错误 | 字符不属于当前 charset（包括数字模式下的字母、空格、`.`、Tab、非 ASCII）；报错带 `/frames/<f>/rows/<y>`、列号，以及原 txt 的 `file:line:col` |
| `ascii.undefined_char` | 错误 | printable 模式下布局字符没有 legend 定义；数字模式下没有 palette 且 legend 没覆盖所用数字 |
| `ascii.ragged_row` | 错误 | 见第 3 节 |
| `ascii.bad_size` | 错误 | `size` 不是 32 或 64 |
| `ascii.size_overflow` / `ascii.offset_out_of_bounds` | 错误 | 布局大于 `size`，或放置后越出画布 |
| `ascii.palette_overflow` / `ascii.color_not_in_palette` | 错误 | 超过 36 色；strict 模式下 hex 不在调色板中 |
| `ascii.stamp_size` / `ascii.frame_cycle` / `ascii.anim_source` | 错误 | 印章尺寸 ≠ cell；`from` 循环引用；动画源帧或方向帧不存在 |
| `ascii.unused_legend` | 警告 | legend 里有布局没用到的字符（`legend_ref` 带来的不计入） |
| `ascii.auto_legend` | 警告（`--strict` 时为错误） | 仍有 `auto: true` 的颜色未确认 |

警告走现有的 `util::guard_warnings`，所以 `--strict` 自然生效。编译之后照常可以跑 `pxl validate`，QA 规则不做任何修改。

## 6. CLI 设计（pxl-cli）

| 命令 | 作用 |
|---|---|
| `pxl ascii import <layout.ascii.txt> -o x.ascii.json [--charset digits\|printable] [--size 32\|64] [--cell NxM\|fit] [--anchor center\|bottom-center\|top-left] [--palette p] [--legend l.legend.json] [--blocks frames\|layers] [--pad] [--no-directives]` | 第一段：把 ASCII 解析成 AsciiSpec。没有被 legend 覆盖的字符按下面规则处理：<br>• 数字模式且给了 `--palette`：数字直接作为索引，不写 legend 条目。<br>• 数字模式且没给 palette：为用到的 `1-9` 写 `"#ff00ffff"` 占位色并标 `auto`。<br>• printable 模式：`.` 和空格设为透明；其余字符在给了 palette 时按首次出现顺序从索引 2 开始分配（跳过 special 索引），没给时写占位色，两种情况都标 `auto`。<br>`--json` 输出 `chars`、`auto`、`frames`、`size`、`cell`、`layout_px`，方便 Agent 接着补色 |
| `pxl gen ascii --spec x.ascii.json -o x.pxl.json [--palette p]` | 第二段：编译成 PxlDoc（`w = h = size`，包含全部帧和 anims），需要时同时写出 `<stem>.palette.json`。`meta.ascii = "x.ascii.json"`，`meta.kind` 取 spec 的 `kind` |
| `pxl ascii render <x.ascii.txt\|x.ascii.json> -o out.png\|out.gif [import 的全部参数] [--anim a --dir d] [--scale 1..16] [--grid] [--emit-json path] [--emit-doc path]` | 快捷命令：import、gen、render 在内存里串起来，输出规则和 `pxl render` 一致（`.gif` 走 `encode_gif_timed`，单帧走 `render_preview_png`，多帧 PNG 拼成横条）。`--emit-*` 用来把中间产物落盘，保证"中间 JSON 是唯一真值"的原则依然可查 |
| `pxl ascii export <x.pxl.json> -o x.ascii.txt [--legend l] [--frame f]`（P2） | 反向导出：只支持 32 或 64 的文档；优先用 legend 的反向映射，否则在索引都 ≤ 9 时直接按数字导出，超出时提示改用 `--charset printable` |
| 构建配置：`"base": { "gen": "ascii", "spec": "x.ascii.json", "frame": "base" }` | 接入 `pxl build`。`RecipeBase` 新增可选字段 `frame`（默认 `base`）。构建配置自身的 `directions`、`anims` 照常生效。spec 里的 anims 和构建配置的 anims 同时非空时报 `E_SCHEMA`，避免出现两份真值；spec 里的逐帧翻页动画要接入构建配置留到 P2（`RecipeAnim.ascii_frames`） |
| `pxl schema ascii`、`pxl schema legend`、`pxl list generators` | 注册新的 schema 种类和生成器 |

`Cmd` 新增 `Ascii { Import | Render | Export }` 子命令，参数结构放在 `cmd/args.rs`，实现放在新文件 `cmd/ascii.rs`。`gen ascii` 在 `generate.rs` 里加一个分支，只负责调用 `cmd/ascii.rs` 的公共函数，保证 `gen`、`ascii render` 和构建配置三条入口共用同一个编译函数。

## 7. 模块落点

| crate | 新增 / 修改 | 说明 |
|---|---|---|
| pxl-core | 可选：`text.rs` 增加 `decode_matrix_with(rows, f: impl Fn(char) -> PxlResult<u8>)` | 让行宽校验和尺寸上限检查只有一份实现，`decode_matrix` 改为基于它实现。保持 `no_std` |
| pxl-format | 新增 `src/ascii.rs`（AsciiSpec、Charset、Anchor、LegendValue、AsciiFrame、AsciiAnim、`parse_ascii_spec`、`parse_legend_file`、`ASCII_SIZES = [32, 64]`）和 `src/ascii_text.rs`（`parse_ascii_text(text, opts) -> AsciiLayout`，纯文本解析，按 charset 校验，不依赖调色板）；`model.rs` 的 `Meta` 增加 `ascii: Option<String>`（带 `skip_serializing_if`）；`recipe.rs` 的 `RecipeBase` 增加 `frame: Option<String>`；`schema.rs` 注册 `ascii` 和 `legend` | 每个文件 < 600 行，遵守 `crates-module-split-plan.md` |
| pxl-gen | 新增 `src/ascii.rs`：`compile_ascii`、legend 解析（包括数字默认映射）、格子展开、图案和印章、画布放置、调色板 strict/extend/生成 | 依赖 format 和 core。帧级 op 需要用到 `pxl-ops`，但 gen 不能反向依赖 ops，所以 **mirror、post、Delta.ops 放到 CLI 层或 pxl-ops 执行**，gen 只产出放进画布后的原始帧（与 shapes 的分层方式一致） |
| pxl-ops | 不新增语义 | 复用 `apply_ops`、`expand_macro` |
| pxl-export / pxl-qa | 不改 | 编译产物就是标准的 `PxlDoc` 和 `IndexFrame` |
| pxl-cli | `lib.rs`（`Cmd::Ascii`）、`cmd/args.rs`、新增 `cmd/ascii.rs`、`cmd/generate.rs`（新分支）、`cmd/sprite.rs`（`"ascii"` 分支）、`cmd/catalog.rs`（generators 列表） | `doc_from_frames` 只支持单帧，所以新写 `doc_from_ascii(compiled, anims)` 组装多帧和 anims |
| examples | 新增 `examples/ascii/{slime,brick_tile,tank_blink}/`，每个目录包含 `.ascii.txt`、`.ascii.json`，至少 1 个目录带 `.recipe.json`，以及共享的 `examples/ascii/battle_city.legend.json`。三个样例都用数字模式，至少一个用 `size: 64`，另加一个 printable 模式的小样例 | 作为测试和文档的样例 |

## 8. 实施批次（每批独立提交，先写测试再实现）

### A1：文本解析与 JSON 模型（pxl-format）

- 实现 `parse_ascii_text`：两种字符集的校验、块切分、指令解析、CRLF 和 BOM 处理、ragged 和 pad、行号映射。
- 实现 `AsciiSpec` 和 `LegendValue` 的反序列化，包括简写写法、"恰好一个字段"的校验、`size ∈ {32, 64}` 和 `keep` 冲突的校验，以及 `pxl schema ascii|legend`。
- **验收**：单测覆盖以下情况：
  - 空文件、单块、多块、`layers` 模式。
  - 数字模式下出现字母、空格、`.`、Tab、全角字符时报错并带正确的行列号。
  - printable 模式下 `::` 误用。
  - `size` 取 16、48、65 时报错。
  - schema 能通过 `pxl schema ascii` 输出，并能校验 `examples/ascii/*.ascii.json`。

### A2：编译核心（pxl-gen）

- 实现 legend 合并（包括 `legend_ref` 和数字默认映射）、Rows、Layers、Delta（含循环检测）、cell 展开，以及 solid、checker、stripe、bevel 图案和印章。
- 实现三种 anchor 加 offset 的画布放置，以及 `fit` 的计算。
- 实现调色板 strict、extend 和自动生成三种模式。
- 实现第 5 节的全部 lint。
- **验收**：
  - 每种 legend 写法、每种帧写法、每种调色板模式至少各有 1 个单测。
  - 不写 legend 的 32×32 数字布局，编译结果与手写的 `PxlDoc` 矩阵逐像素一致。
  - 8×5 布局在 `size: 32` 下用 `fit` 得到 `cell` 4，`bottom-center` 放置后的像素坐标正确。
  - 布局越界时报 `size_overflow` 或 `offset_out_of_bounds`。
  - 同一输入编译两次，结果字节一致。

### A3：CLI 三段式与快捷命令（pxl-cli）

- 实现 `pxl ascii import`、`pxl gen ascii`、`pxl ascii render`，以及 `--emit-json` 和 `--emit-doc`。
- `--json` 输出的结构与现有 `ok_base` 保持一致；错误带 `path` 和 `hint`。
- **验收**：新增 `tests/cli_cases/ascii.rs`，覆盖：
  - txt 到 png、txt 到 gif、json 到 pxl.json 再到 render 三条路径。
  - `ascii render` 一步完成和分三步执行得到的 PNG 哈希一致。
  - 输出的 PNG 在 `--scale 1` 下为 32×32 或 64×64。
  - `--strict` 下有 `auto_legend` 时命令失败。
  - `list generators` 中包含 `ascii`。

### A4：动效与构建配置接入

- 在 CLI 层组装逐帧翻页、macro、ops、dirs 动画，以及 `post`、`mirror`、`Delta.ops`。
- 构建配置支持 `gen = "ascii"` 和 `base.frame`，并实现"spec 动画与构建配置动画不能同时非空"的校验。
- **验收**：
  - `examples/ascii/slime` 能通过 `pxl build` 产出 sheet 和 preview，`pxl validate` 默认规则通过。
  - `tank_blink` 在 `dirs` 场景下找不到方向帧时报 `ascii.anim_source`。
  - GIF 帧延时与 fps 一致。

### A5：黄金测试与文档

- 把 `examples/ascii/*` 的 PNG/GIF 哈希加入 `tests/golden/hashes.json`，并确认原有黄金测试的哈希都不变（`meta.ascii` 为 `None` 时不序列化）。
- 在 `../v1.0/cli-design.md` 补充 `pxl ascii` 子命令，在 `../v1.0/asset-pipeline.md` 补充 AsciiSpec 数据模型，并更新 `../README.md` 中本计划的状态。
- **验收**：全工作区 `cargo test` 全部通过；crates 内所有 `.rs` 文件 < 600 行。

### P2（不在本轮承诺）

- `pxl ascii export` 反向导出。
- 构建配置引用 spec 里的逐帧翻页动画（`RecipeAnim.ascii_frames`）。
- 图层级 legend：每个图层单独定义 legend，用来突破数字模式只有 10 种符号的限制。
- legend 的 `cycle` 写法：绑定调色板 cycle，用于水面、火焰这类调色板循环动画。
- harness 资产 Agent 提供 `ascii` 起稿工具和受限 schema 工具；`pxl mcp` 暴露 `ascii.import` 和 `ascii.render`。
- 视需求再评估是否开放 16×16（Battle City 原尺寸 tile）。在此之前，16×16 素材继续走 shapes。

## 9. 风险与待定项

| 风险 / 待定 | 处理 |
|---|---|
| 数字模式只有 10 种符号，复杂素材不够用 | 用图案或印章补细节；必要时显式切到 `printable`；P2 再做图层级 legend。默认不放宽 |
| 32/64 固定画布与现有 16×16 素材（如 Battle City tile）不兼容 | 本计划不覆盖 16×16，这类素材继续走 shapes，P2 再评估。不偷偷缩放，因为最近邻缩放会改变像素语义 |
| 小布局放进大画布后，四周透明区域很大，可能影响 bbox 等 QA 指标 | 用 `anchor` 和 `offset` 控制位置；QA 阈值不改，确实不合适时改用 `size: 32` 或调大 `cell` |
| 64×64 加 36 色的上限 | 沿用现有约束，在 lint 阶段提前报错 |
| hex 颜色不在调色板中时是否就近量化 | **不量化**，strict 报错并给出最近色作为 hint，需要新颜色时用 extend 显式追加。理由是静默量化会让"所写即所得"失效 |
| 自动生成的调色板没有 ramps，导致 `bevel` 和 `ramp` 写法不可用 | 使用这些写法时要求指定 `palette` 或在 legend 中写全 `alt` 色，否则报错 |
| 依赖方向：帧级 op 在 pxl-ops，而 gen 不能依赖 ops | gen 只负责展开和放置，op 统一在 CLI 层执行（见第 7 节）；实施前核实 `pxl-gen` 的 `Cargo.toml` 依赖 |
| printable 模式下指令语法 `::` 与画面内容冲突 | 指令只匹配严格正则，不匹配时报错而不是当作像素；`--no-directives` 兜底。数字模式不存在这个问题 |
| 多帧布局尺寸不一致 | 所有帧共用同一画布和放置规则，所以尺寸不同的布局也能对齐到同一画布；超出画布时报错 |

## 10. 实施记录（2026-10-23）

**落地范围**：A1–A5 全部实现，`cargo test --workspace` 通过（新增单测 + 10 个 CLI 集成用例 + 3 个 ascii 黄金哈希）。

| 层 | 文件 | 内容 |
|---|---|---|
| pxl-format | `src/ascii.rs`（新）、`src/ascii_text.rs`（新） | `AsciiSpec`/`LegendValue`/`AsciiFrame`/`AsciiAnim`/`LegendFile` 与文本解析；`pxl schema ascii\|legend` |
| pxl-format | `src/model.rs`、`src/recipe.rs`、`src/schema.rs` | `meta.ascii`、`RecipeBase.frame`、schema 注册 |
| pxl-gen | `src/ascii/{mod,legend,frames}.rs`（新） | legend 合并与调色板 strict/extend/derive、cell 展开、图案/印章、图层合成、差量 patch、anchor+offset 放置、编译期 lint |
| pxl-cli | `src/cmd/ascii/{mod,compile}.rs`（新） | `pxl ascii import\|render`、`pxl gen ascii`、recipe 的 `gen = "ascii"`、delta/mirror/post 执行与动画组装 |
| pxl-cli | `cmd/{args,generate,sprite,render,catalog,mod}.rs` | 子命令、生成器分支、recipe 分支、`render_doc` 复用 |
| examples | `examples/ascii/{slime,tank_blink,brick_tile,gem}/` + `shared/p16.legend.json` | 数字/printable、32/64、块印章、差量 blink、spec 动效、recipe 各一例 |

**与计划的偏差（均为收敛后的实现选择）**：

1. `pxl ascii import` 的自动帧名：第一个无头块命名为 `base`（而非 `f0`），后续为 `f1`、`f2`；`--blocks layers` 仍为 `layer0`、`layer1`。
2. 配方中的 spec 动效：若 recipe 未声明 anims，则允许 spec 的 anims 原样导入（flipbook/macro/ops 走同一 `attach_anims`）；两者同时非空报 `ascii.recipe_anims`。`palette_mode = extend` 在 recipe 中仍不支持，因为 build 只复制一份 recipe 调色板。
3. 文字输入 + `--anim` 直接报 `ascii.anim_source`，提示先 `import` 再补 anims；文本本身不承载动效。
4. 新增 lint id：`ascii.pattern_alt_missing`（checker/stripe 缺 `alt`）、`ascii.pattern_on_transparent`、`ascii.ramp_missing`/`ascii.ramp_step`、`ascii.flash_needs_color`/`ascii.outline_needs_color`、`ascii.derive_requires_color`（无调色板时禁止显式索引）、`ascii.stamp_requires_palette`、`ascii.empty_palette`、`ascii.frame_source`/`ascii.undefined_layer`。
5. `--cell fit` 只写入具体数值，JSON 中不保存 `fit`；`import` 的 `--json` 额外返回 `chars`/`auto`/`layout_px`/`blocks`。
6. 复查补齐（同日）：
   - `--pad` 补齐行时产生 `ascii.padded_row` 警告（`import` 的 `warnings` 字段输出，`--strict` 下失败且不落盘），此前缺失。
   - `ascii.size_overflow` 的 hint 现在给出能放下的最大 `cell`（如 `6x6`）；`legend_ref` 的 `charset` 与 spec 不一致时报 `ascii.legend_charset`（第 4.4 节约束）。
   - 补验收用例：64 画布输出 64×64、GIF 帧数与头、`dirs` 缺方向帧报 `ascii.anim_source`、同输入两次编译字节一致、四个示例的 `source.sha256` 与文本一致、`--pad` 警告与 strict。
   - 已知遗留：`crates/pxl-cli/tests/cli_cases/qa_assets.rs` 为 612 行，超过第 8 节 A5 的 600 行上限；该文件属于 pxl-qa 重构的既有改动，不在本计划范围，需在那条线上拆分。
7. 未实现（P2）：`pxl ascii export`、recipe 引用 spec 的逐帧翻页动画（当前整段 spec anims 直接导入）、图层级 legend、legend `cycle`、harness/MCP 工具、16×16 画布。
