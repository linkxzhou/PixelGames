# v3.0 Plan v2：ASCII / JSON → 像素图的确定性重建管线

> 日期：2026-10-24
> 前置：`pxl-native-resolution-quality-plan.md` 第 1–11 节（原生分辨率、`ascii refine`、`copy2x_bps`）。
> 范围：只改 `crates/` 与 `examples/`；不引入模型调用、不改 `harness/`、`web/`。
> 目标：把“16 网格骨架 + EPX 圆角”升级为**语义感知的多阶段像素画重建**：分析 → 放大 → 清理 → 描边 → 着色 → 细节 → 展示 → 度量。所有阶段为整数/定点、确定性、只在索引帧上工作，不输出插值模糊。
> 关于“好 100 倍”：无法用单一数字证明。本计划把它拆成 12 项可测指标（第 9 节）与人工对照验收，每一阶段必须让指标单调改善才允许合入。

## 0. 先修：编译与测试恢复（P0，阻塞一切）

**状态更新（2026-10-24 08:40）：V0 已关闭。B1/B2 均已解决，`cargo test --workspace --no-fail-fast` 全绿（`pxl-art` 43 项通过）。**

（以下为 07:24 的历史记录，保留作过程参考。）

已完成：

- `crates/pxl-core/src/scale.rs` 已创建：`scale_copy` / `double_copy` / `epx2x`，块外按透明（0）处理；`lib.rs` 已导出。原 `pxl-format` 的两个 E0425 消失。
- 根 `Cargo.toml` workspace members 补上 `crates/pxl-engine`、`crates/pxl-wasm`（此前游离在 members 外，不参与 workspace 编译）。
- `cargo check --workspace` 通过（仅 vendor/miniquad 的 cfg 噪声警告）。

剩余阻塞：

- **B1（golden 失败）**：`cargo test --workspace` 中仅 `golden_hashes_match` 失败：`examples/ascii/brick_tile/brick_tile.ascii.json` 的 legend `"1"` 是 8×8 stamp，而 `cell` 为 `[2,2]`，触发 `ascii.stamp_size`（校验在 `pxl-gen/src/ascii/frames.rs`）。该素材由会话生成、`examples/ascii/` 整体未跟踪。修法二选一：把 stamp 缩到 2×2（保持 32 网格语义），或 `cell` 提到 8 且 rows 同步调整；修复后重算 `golden/hashes.json` 的 `ascii.brick.base`（slime 两条哈希已写入且可通过）。
- **B2（pxl-art 未入 workspace）**：`crates/pxl-art` 已存在但不在 members 中，`cargo check --workspace` 完全跳过它；其 `lib.rs` 已声明并 `pub use` 尚不存在的 `metrics` / `outline` / `pipeline` 三模块，此刻加入 members 会立即编译失败。必须先补齐三模块（见第 13 节），再加入 members 并 `cargo test -p pxl-art`。

原验收不变：`cargo test --workspace` 恢复通过，`examples/retro_sprites_32/*.ascii.txt` 能由 `ascii refine` 字节级重现（`bomberman`/`castlevania_simon` 是纯 corners 草稿，可直接作为回归黄金）。

## 1. 截图诊断（上：48@2× 旧版；下：32 网格 refine 版）

逐图观察，并对应到代码根因：

| # | 现象 | 根因 | 归属阶段 |
|---|---|---|---|
| D1 | 描边几乎看不见，轮廓靠填充色撑着 | 描边 `#202028` 与预览背景 `(28,26,24)` 几乎同色（RGB 各通道差 ≤ 16，未做精确 ΔE 计算）；`composite` 背景写死 | 展示 S7 / QA |
| D2 | 坦克履带被圆成一串“胶囊” | EPX 对每个 2×1 履带节独立圆角；机械材质本应保持硬边 | 放大 S2（材质感知） |
| D3 | 坦克履带高光用了皮肤色 `#F0B888` | `retro9` 只有 9 色，索引 7 是皮肤；无 ramp，手工补色只能借用 | 调色 S5 |
| D4 | 西蒙胸口黄色块变成菱形；炸弹人天线变菱形 | EPX 的经典伪影：2×2 以下孤立色块四角全被削 | 放大 S2（小特征保护） |
| D5 | 炸弹人手、脚出现单像素凸起/缺口 | EPX 只看 4 邻域，不看走向，产生 L 形“double” 与孤点 | 清理 S3 |
| D6 | 斜线阶梯不均（西蒙头发、手臂） | 原 16 网格的阶梯被 2× 后变成 2-2-2 等长步，没有 1-2-3 渐变节奏 | 轮廓重建 S2b |
| D7 | 所有色块纯平，无体积 | 调色板无 `ramps`；`shade` 只看透明邻居 | 着色 S5 |
| D8 | 32 网格新增信息只有 44–86 像素（≤8.4%） | refine 不产生新结构，眼、手、装备细节仍是旧布局 | 细节 S6 |
| D9 | 内部描边（脸/盔、腰带/身体）与外描边同色同粗 | 无 selout、无内外描边区分 | 描边 S4 |

结论：**放大算法本身只是瓶颈之一**。截图里最显眼的问题（D1、D3、D7）与放大无关；真正的收益来自“语义 + 调色 + 清理 + 展示”整条链。

## 2. 总体设计

### 2.1 管线

```
ASCII text / ascii.json / shapes.json
   │  (现有) parse + legend + compile → IndexFrame(native)
   ▼
S1 Analyze   连通域、材质、骨架、走向、细特征、对称轴      → SpriteGraph
S2 Upscale   index-xBR / contour 重建 / EPX / copy           → IndexFrame(k×)
S3 Cleanup   pixel-perfect、孤点、阶梯节奏、对称修复
S4 Outline   统一外描边、内描边、selout、断线修补
S5 Shade     OKLab ramp 生成、光照（距离场法线）、AA、受控抖动
S6 Detail    语义 stamp、材质纹理、高光点
S7 Present   背景自适应、棋盘格、对照拼图（仅预览）
S8 Measure   12 项指标 + 候选排名
```

每阶段输入输出都是 `IndexFrame` + 可选附属层（材质图 `MaterialMap`、保护掩码 `Protect`），可单独开关、单独测试、单独度量。

### 2.2 新 crate：`pxl-art`

| 模块 | 内容 |
|---|---|
| `analyze.rs` | 连通域（4/8）、区域邻接图、骨架（Zhang–Suen）、细特征检测、对称轴检测 |
| `material.rs` | `MaterialMap`：每像素材质 id；由 legend `role`/新字段 `material` 推出 |
| `upscale/` | `xbr.rs`、`contour.rs`、`epx.rs`（转调 core）、`eagle.rs`、`scale3x.rs` |
| `cleanup.rs` | pixel-perfect、orphan、jaggy rhythm、symmetry |
| `outline.rs` | 外/内描边、selout、断点修补 |
| `shade.rs` | 距离场、法线估计、ramp 着色、AA、Bayer 抖动 |
| `detail.rs` | 语义 stamp 匹配与落位、材质纹理 |
| `metrics.rs` | 第 9 节指标 |
| `pipeline.rs` | `EnhanceSpec` → 依次执行；候选生成与排名 |

依赖：只依赖 `pxl-core`（`no_std + alloc`，保证 WASM 可用）。颜色数学（OKLab、ΔE）放进 `pxl-core/src/color.rs`，用定点整数（Q16）实现，避免平台浮点差异影响确定性。

不放进 `pxl-ops`：`pxl-ops` 是单步 op 解释器；`pxl-art` 是需要全局分析的多阶段算法。`pxl-ops` 只增加一个 `Op::Enhance` 薄转发（第 7 节）。

### 2.3 硬约束（所有算法共享）

1. **不发明调色板外的颜色**：新颜色只能来自已声明 ramp 或 S5 生成并写回 palette 文件的 ramp。
2. **确定性**：无浮点比较分支；哈希种子沿用 `op_seed`；同输入同输出跨平台一致。
3. **不做插值/锐化**：输出始终是索引帧；预览仍是 nearest。
4. **可关闭**：旧 spec 不含 `enhance` 时结果与现在逐字节一致；黄金哈希不变。
5. **透明语义**：透明 = 0，不参与“颜色相等”判断以外的混色。

## 3. S1 Analyze：先理解精灵，再动像素

所有后续算法的质量取决于这一步。EPX 失败（D2/D4/D5）的本质是“只看 4 个邻居，不知道自己在画什么”。

### 3.1 `SpriteGraph`

```rust
pub struct SpriteGraph {
    pub regions: Vec<Region>,          // 同色 4-连通域
    pub adjacency: Vec<(u16, u16, u32)>, // 区域邻接与共享边长
    pub outline_index: Option<u8>,     // palette.special.outline
    pub material: MaterialMap,         // 每像素材质 id
    pub thin: BitMask,                 // 宽度 ≤ 1 原生像素的特征（天线、眼、腰带扣）
    pub small: Vec<u16>,               // 面积 ≤ 4 的区域（受保护，不能被削角）
    pub skeleton: BitMask,             // Zhang–Suen 细化结果
    pub symmetry: Option<Axis>,        // 镜像对称轴（容差 ≤ 3% 像素）
    pub edges: Vec<EdgeRun>,           // 轮廓链码（8 方向）
}
```

### 3.2 材质来源（优先级从高到低）

1. legend 新字段 `material: "metal" | "cloth" | "skin" | "hair" | "glass" | "organic" | "emissive" | "outline"`（`LegendEntry` 增加可选字段，`deny_unknown_fields` 下需同步 schema）。
2. legend 已有 `role`（映射表：`skin→skin`、`outline→outline` …）。
3. ASCII 新指令 `:: materials` 块：与帧同尺寸的字符图，每个字符对应材质（适用于 digits charset，无 legend 时）。
4. 启发式默认：outline 索引 → `outline`；其余 → `organic`。

材质决定后续行为：

| 材质 | 放大 | 清理 | 着色 | 细节 |
|---|---|---|---|---|
| `metal` | 保持直角，只圆外轮廓 | 保留硬阶梯 | 高对比 3–4 阶 + 镜面点 | 铆钉、分节线 |
| `cloth` | 平滑 | 允许 1px 褶皱 | 柔和 2–3 阶 | 褶皱线 |
| `skin` | 平滑 | 强制 pixel-perfect | 2 阶 + 腮红可选 | 无 |
| `hair` | 方向性平滑（沿骨架） | 保留尖端 | 条带高光 | 发丝 |
| `glass`/`emissive` | 平滑 | — | 中心亮 | 反光点 |
| `outline` | 跟随相邻填充 | 断线修补 | 不着色 | — |

### 3.3 细特征保护（修 D4）

- `small` 区域（面积 ≤ 4 原生像素）与 `thin` 像素加入 `Protect` 掩码；S2 对其使用 `copy` 或“十字保形”（只削去与背景同侧的一个角），避免黄色块变菱形、天线变菱形。
- 1×1 眼睛、2×1 扣子：保持轴对齐矩形，放大后为 2×2 / 4×2 实心。

### 3.4 对称轴

- 计算镜像差异像素数，左右对称率 ≥ 97% 视为对称。S3 用它修复因 EPX 方向偏置导致的左右不一致（炸弹人两手、坦克两侧履带）。
- 有意不对称的精灵（西蒙的鞭）低于阈值，自动不启用。

## 4. S2 Upscale：可选算法族与选择规则

全部实现为 **索引空间** 算法：颜色“相等”用索引相等；需要“相近”时用 palette 的 OKLab 距离（定点）。

| 算法 | 倍率 | 特点 | 适用 |
|---|---|---|---|
| `copy` | k | 基线 | 对照、metal 内部 |
| `epx`（Scale2x） | 2 | 已有；削角 | 草稿 |
| `scale3x` | 3 | Scale2x 的 3× 版本，保留中心 | 16→48 |
| `eagle` | 2 | 对角线更强 | 仅实验 |
| `xbr-index` | 2/3/4 | xBR 的边缘方向判定改为索引+OKLab 距离，**不混色**，只选邻色 | organic/skin/cloth 主力 |
| `contour` | 任意整数 | 轮廓矢量化后重栅格化（4.2） | 外轮廓、hair |
| `hybrid`（默认） | 2/3/4 | 按材质与保护掩码逐区域选择上面算法 | 生产 |

### 4.1 `xbr-index`

- 对每个像素在 5×5 窗口内比较两条对角方向的“边缘权重”（加权的 OKLab 距离和），权重小者为连续边方向。
- 输出子像素只能取 **中心色或参与判定的邻居色**；原版 xBR 的线性混合被替换为“按覆盖率 ≥ 50% 取色”，因此永远不产生新颜色。
- 透明参与判定（距离视为最大），保证轮廓收边干净。
- 结果对 1 像素宽斜线保持连续（EPX 会断）。

### 4.2 `contour`：轮廓重建（修 D6，“细节真正变多”的关键）

1. 对每个区域边界做 Marching Squares，得到原生网格上的折线。
2. 用 Ramer–Douglas–Peucker（定点阈值 0.5px）简化，保留角点（转角 ≥ 60° 或材质为 metal）。
3. 把直线段之间的阶梯识别为“理想斜率”，按像素画规则重新栅格化：同一线段内步长只取 `n` 或 `n+1`，并按 Bresenham 均匀分布（1-1-2-1-1-2…），**禁止 2-1-3 这类不规则步**。
4. 曲线段（连续 3 个以上转角同向）用整数中点椭圆/二次曲线拟合，阶梯形成 1-1-2-3-… 的渐变节奏。
5. 结果为 k× 的区域填充；区域按原 z 序（outline 最后）回填。

`contour` 是本计划唯一真正“生成新几何信息”的放大器：输出不再是原生像素的函数，而是对原形状意图的重新采样，因而 32@1× 的轮廓会比 16@2× 平滑得多，同时保持硬边像素画。

### 4.3 `hybrid` 规则

```
Protect 像素            → copy
material == metal       → 内部 copy；外轮廓 contour(corner_keep=true)
material == hair        → contour(沿骨架方向) 
skin / cloth / organic  → xbr-index
outline                 → 不放大，S4 重新生成
```

先放大“去掉描边的填充层”，描边由 S4 在 k× 网格上重新生成 1px 线 —— 避免 2× 后得到 2px 粗描边（截图中描边偏粗、在暗背景上糊成一团的原因之一）。

### 4.4 倍率

- 2×：16→32（现有路线）。
- 3×：16→48，主体与旧 48 版同尺寸，可直接与上排截图同占地对照；`size 64` 画布足够。
- 4×：16→64；对 32 网格素材 32→128 依赖 `MAX_DIM=128`（已具备）。
- ASCII 侧仍受 `size` 白名单 32/64 限制：S2 在编译后的 `IndexFrame` 上运行，输出交给 `PxlDoc`，不经过 ASCII size 校验；若要回写 `.ascii.txt`，只允许 ≤64。

## 5. S3 Cleanup：像素画规范化

| 规则 | 说明 | 修复 |
|---|---|---|
| pixel-perfect | 删除 1px 线中的 L 形拐点（3 像素构成直角时删中间） | D5 |
| orphan | 删除与同色不连通、面积 1、四邻全异色的孤点（Protect 除外） | D5 |
| notch fill | 填补轮廓上 1px 凹口（三面同色） | D5 |
| jaggy rhythm | 检测轮廓阶梯步长序列，修正为单调或均匀（与 4.2 同规则，作用于非 contour 算法的输出） | D6 |
| symmetry | 对称精灵：以“多数侧”为准镜像修复差异像素，差异仅限 S2 引入的像素（与 copy 基线比较得出） | D5 |
| banding | 检测平行于轮廓的同宽色带（新手常见错误），提示或将内侧带错开 1px | D7 |

每条规则输出修改计数，写入报告；修改像素数超过阈值（默认 5%）时只告警不静默大改。

## 6. S4–S6：描边、着色与细节

### 6.1 S4 Outline

- **统一外描边**：在 k× 网格上对填充层做 1px outer outline（复用 `paint_outline`，brush=`diamond` 避免方角加粗）。
- **内描边**（区域边界）：相邻区域之间画 1px 分界，颜色取两侧较暗者的 ramp 最暗阶，而不是统一黑色。
- **selout**（selective outline）：外描边每个像素取其内侧填充色 ramp 的最暗阶；受光侧（S5 光源方向）再提亮一阶。解决 D1（描边与背景同色）和 D9。
- **断线修补**：8-连通检查外描边，单像素断口补齐。
- 选项：`outline: none | black | selout | selout_lit`，默认 `selout`。

### 6.2 S5 Shade

1. **Ramp 生成**（修 D3/D7）：`pxl palette ramp` —— 对每个基色在 OKLab 中生成 3–5 阶 ramp，带 hue shift（暗部向冷色偏 8–15°，亮部向暖色偏），亮度等距；写入 palette 的 `ramps` 并追加颜色。总色数上限可配（默认 32），超出时按使用面积合并相近色（中位切分 + OKLab）。
2. **体积法线**：对每个材质区域计算距离变换（Chamfer 3-4，整数），由距离场梯度估计 2D 法线，结合光源方向（默认左上）得到明度等级；按 ramp 映射。比现有 `shade`（只看透明邻居）多出真正的“圆柱/球体”体积。
3. **材质参数**：metal 高对比 + 镜面点（距离场极值附近 1–2 像素）；skin 只 2 阶；cloth 在骨架交叉点加褶皱阴影。
4. **AA（手工风格抗锯齿）**：仅在外轮廓内侧的阶梯拐角处放 1 个中间色像素，长度 = 阶梯步长−1，最多 1 层；不对外描边外侧做 AA（保持透明资产干净）。
5. **抖动**：可选 Bayer 2×2/4×4，仅在 ramp 相邻两阶之间、且区域面积 ≥ 24 像素时启用；默认关闭。

### 6.3 S6 Detail：语义 stamp

- `examples/stamps/` 新建 stamp 库（`.ascii.txt`，多尺寸）：眼睛（2×2/3×2/4×3）、手（3×3/4×4）、靴子、铆钉、履带节、腰带扣、头发尖。
- 匹配：S1 的 `small`/`thin` 区域按“材质 + 尺寸 + 位置（头部上 1/3 为眼睛候选）”匹配 stamp 槽位；也可在 ASCII 中用新 legend 字段 `slot: "eye"` 显式标注（推荐，避免猜错）。
- 落位后由 ramp 着色，stamp 只携带“相对阶”（`0=主色, -1=暗, +1=亮, o=描边`），自动适配任何调色板。
- metal 纹理：按材质方向放置分节线（履带节距可配置），不是随机噪声，修 D2。
- 所有 detail 写入受材质掩码约束，复用现有 `Detail` op 的位置哈希做“微变化”。

## 7. 接口：JSON / ASCII / CLI / Engine

### 7.1 spec 字段（ascii.json 与 shapes.json 共用）

```json
"enhance": {
  "v": 1,
  "scale": 3,
  "upscale": "hybrid",
  "cleanup": ["pixel_perfect", "orphan", "notch", "rhythm", "symmetry"],
  "outline": "selout",
  "light": "top_left",
  "shade": { "levels": 3, "aa": true, "dither": "none" },
  "detail": { "stamps": "examples/stamps", "slots": "auto" },
  "palette": { "ramps": "generate", "max_colors": 32, "hue_shift": 12 }
}
```

- `AsciiSpec` 与 shapes spec 增加可选 `enhance`（`skip_serializing_if = "Option::is_none"`），旧文件无此字段时行为不变。
- 执行位置：`pxl-engine/src/ascii_compile.rs` 中 `delta ops → mirror → post` 之后、动画展开之前；对每个关键帧执行一次，动画帧在放大后的空间里重新应用（见 7.4）。
- 输出 `size` = `native × scale`；engine `DocInfo` 报告 `native_size`、`enhanced_size`、`scale`、`stages`。

### 7.2 Op

`Op::Enhance { spec: EnhanceSpec }` 薄转发到 `pxl_art::pipeline::run`，可放在 recipe `post` 中做局部实验；`OP_NAMES` 与 schema 同步。

### 7.3 CLI

```
pxl enhance <in.(ascii.json|shapes.json|png)> -o out.png [--scale 3] [--upscale hybrid] [--outline selout] [--report out.json]
pxl enhance compare <in> -o sheet.png --variants copy,epx,xbr,contour,hybrid --scale 3
pxl palette ramp <palette.json> -o <out.palette.json> [--levels 4] [--hue-shift 12] [--max 32]
pxl ascii refine ... --mode xbr|contour|hybrid      # 文本侧草稿也能使用新算法
pxl qa ... --metrics art                           # 输出第 9 节指标
```

- PNG 输入：先用现有“精确块复制”检测把 k× 放大图还原成原生帧，再运行管线（避免对 `pxlgen.png` 这种 16× 图做无意义处理）。
- `compare` 生成同占地对照拼图，背景用 S7 的棋盘格，标注算法名与指标。

### 7.4 动画

- 关键帧放大后，`anims` 中的 `dx/dy/region` 按 `scale` 乘算（这是位置，不是线宽），线宽类参数不乘。
- 帧间一致性：同一区域在各帧中的 S2/S3 决策用“基准帧决策 + 位移”复用，避免描边/AA 在帧间闪烁；QA 增加 `anim.enhance_flicker`：相邻帧在静止区域内的差异像素数。
- `parts/rig` 路径：对每个 part 单独增强再 compose，比对整帧增强更稳定（推荐）。

### 7.5 WASM

`pxl-art` 为 `no_std + alloc`，engine 已经是 WASM 薄适配，`enhance` 自动可用；对 128×128 × 16 帧设置时间预算测试（第 10 节）。

## 8. S7 Present：展示不再吃掉描边（修 D1）

- `composite` 的背景 `(28,26,24)` 改为可配置：`bg: auto | checker | #rrggbb`。
- `auto`：计算 palette 中 outline 色的 OKLab L，与背景 ΔL < 0.15 时自动切换为中灰 `#6b6b78` 或棋盘格（8×8 原生像素）。
- 默认值保持兼容：旧调用仍得到原背景；新 `enhance compare` 默认 `checker`。
- 截图对比中，上下两排都在近黑背景上，深描边“消失”，主体看起来都更粗糙；先修展示才能公平评估算法。

## 9. S8 Measure：把“好 100 倍”拆成可测指标

全部为 `pxl-art/src/metrics.rs` 纯函数，`pxl qa --metrics art` 输出，Info 级，不进入 `qa.ok`。方向列表示“越大/越小越好”。

| # | 指标 | 定义 | 方向 | 对应问题 |
|---|---|---|---|---|
| M1 | `copy2x_bps` | 已有 | ↓ | D8 |
| M2 | `new_info_bps` | 与 `copy` 基线不同的像素占主体比例 | ↑（上限 4000） | D8 |
| M3 | `jaggy_irregular` | 轮廓阶梯序列中非单调/非均匀步的个数 | ↓ | D6 |
| M4 | `lshape_count` | 1px 线中的 L 形拐点数 | ↓ | D5 |
| M5 | `orphan_count` | 孤立单像素数（Protect 外） | ↓ | D5 |
| M6 | `small_feature_kept` | S1 `small` 区域放大后保持轴对齐矩形的比例 | ↑（=100%） | D4 |
| M7 | `outline_bg_dl` | 描边与预览背景的 OKLab ΔL | ↑（≥0.15） | D1 |
| M8 | `outline_continuity` | 外描边 8-连通分量数 / 主体连通分量数 | →1 | D9 |
| M9 | `ramp_coverage` | 主体像素中属于 ≥3 阶 ramp 的比例 | ↑ | D3/D7 |
| M10 | `value_range` | 主体 OKLab L 的 P95−P5 | ↑（适度） | D7 |
| M11 | `symmetry_err` | 对称精灵的镜像差异像素数 | ↓ | D5 |
| M12 | `banding_count` | 平行色带段数 | ↓ | D7 |

合入规则：每个阶段的 PR 必须附 3 个基准样例在启用前后的指标表；目标指标改善、其他指标不恶化超过 5%。指标只防倒退，**最终美术结论仍以人工同占地对照为准**。

### 9.1 候选排名（可选）

`enhance compare --rank`：对多个变体计算加权分（权重在 `pxl-art/src/metrics.rs` 常量里，可被 config 覆盖），输出排名与分项，用于批量处理 20 个角色时初筛；不替代人工确认。

## 10. 测试

### 10.1 单元（`pxl-art`）

- 每个放大器：单像素、2×2 块、1px 斜线（连续性）、L 形、棋盘、全透明、全填充、边缘贴画布、调色板只含 2 色。
- 不变量（属性测试，固定种子生成 500 张随机 16×16 索引图）：
  - 输出颜色集合 ⊆ 输入颜色集合 ∪ 生成 ramp；
  - 同输入同输出（运行两次哈希一致）；
  - `copy` 模式与 `scale_nearest` 逐像素一致；
  - 输出尺寸 = 输入 × scale；不 panic。
- `contour`：圆形（中点圆参考）重建误差 ≤ 1 像素；直线斜率 1/2、1/3 的步长序列为均匀。
- cleanup：每条规则正例/反例；Protect 像素不被修改。
- OKLab 定点：与 f64 参考实现误差 < 1/256，且全平台一致（只用整数）。
- 距离场与法线：球形区域的明度等级同心分布。

### 10.2 集成（`pxl-cli/tests/cli_cases/enhance.rs`）

- `pxl enhance` 处理 `retro_sprites_48` 的 3 个基准角色 ×（2×、3×），输出尺寸正确、QA 合法。
- `enhance compare` 生成拼图且标注齐全。
- 旧 spec（无 `enhance`）的全部黄金哈希不变。
- PNG 输入：`examples/hero_falcon/pxlgen.png` 还原为 24×24 原生帧后再增强。
- 动画：带 `anims` 的样例增强后帧数、时序不变，`anim.enhance_flicker` ≤ 阈值。

### 10.3 性能

- 记录 16→48、32→128 单帧与 16 帧动画的耗时基线（native 与 WASM）；先测量再定门槛，不预先承诺数字。
- `contour` 与距离场复杂度 O(n)，xbr O(n·25)；总像素受 `MAX_FRAME_PIXELS` 约束。

## 11. 里程碑

| 里程碑 | 内容 | 退出条件 | 预期可见收益 | 状态（2026-10-24） |
|---|---|---|---|---|
| **V0** | 第 0 节编译修复；`pxl-core/scale.rs`；OKLab 定点 | `cargo test --workspace` 通过 | 恢复可用 | **完成（2026-10-24）**：B1/B2 已解决，workspace 全绿 |
| **V1** | S7 背景自适应 + `enhance compare` 拼图 + M1–M12 指标 | 对现有 32/48 样例出指标表与新对照图 | 公平评估；描边可见（D1） | 代码完成；人工确认待确认；`--rank` 未做 |
| **V2** | `pxl palette ramp` + S5 着色（距离场 + selout） | 3 基准样例 M7/M9/M10 改善 | 体积感、颜色层次（D3/D7/D9），**单项收益最大** | 代码完成：`palette ramp`、`enhance --shade volume`；退出条件部分达成（见 13.3）；AA/抖动/材质褶皱未做；人工确认待确认 |
| **V3** | S1 Analyze + S2 `xbr-index` / `scale3x` / `hybrid` + Protect | M4/M5/M6 改善，M6=100% | 不再菱形化、胶囊化（D2/D4） | 代码完成：Protect CLI、标准 Scale3x、2× contour 采样已修；3 样例目标指标达成（见 13.4）；人工确认待确认 |
| **V4** | S2 `contour` + S3 cleanup | M3/M11 改善 | 曲线节奏、对称（D5/D6） | 代码完成：4.2 矢量重建、rhythm/symmetry 已接入；M3/M11 目标达成，M6/M8 保持；M2 部分样例下降（见 13.5）；人工确认待确认 |
| **V5** | S4 完整描边 + S6 stamp 库 + legend `material`/`slot` | 眼/手/履带有真实结构；M2 ≥ 2000 bps | 真正新增细节（D8） | 未开始 |
| **V6** | `Op::Enhance`、动画一致性、WASM、`ascii refine --mode hybrid` | 动画样例无闪烁；WASM 一致 | 进入生产链路 | 未开始 |
| **V7** | 批量迁移 `retro_sprites_48` 全部 20 角色，生成 `examples/retro_sprites_hd/` | 全部 QA 合法；人工抽检 | 规模化 | 未开始 |

建议顺序：V0 → V1 → V2 → V3 → V4 → V5 → V6 → V7。V2 放在 V3 之前是刻意的：截图里观感差距最大的是“纯平 + 描边不可见”，而不是放大算法。

每个里程碑交付时更新本文第 13 节实施记录，附：指标表、同占地对照图（旧 48@2×、32-refine@3×、新 hybrid@k）、人工确认结论（未确认写“待确认”）。

## 12. 风险与不做

| 风险 | 缓解 |
|---|---|
| 自动着色把复古平涂风格“现代化”，不符合用户意图 | 每阶段可关；提供 `preset: nes`（无 AA、≤4 色/区域、无抖动）与 `preset: modern` |
| 启发式误判材质/眼睛位置 | 显式 `material`/`slot` 优先；启发式结果写入报告供审阅 |
| 调色板扩色破坏 `palette_mode: strict` | `ramps: generate` 只在 `extend` 模式允许；strict 模式只用已声明 ramp，否则报错并给出 `pxl palette ramp` 提示 |
| contour 在极小精灵（≤12px）上丢形 | 区域面积 < 阈值时回退 xbr/copy；M6 守护 |
| 帧间闪烁 | 基准帧决策复用 + `anim.enhance_flicker` |
| 浮点导致跨平台不一致 | 全部定点；OKLab 用查表 + Q16 |

不做：神经网络超分、双线性/双三次、任意角度旋转（仍归 cleanEdge 单列）、`harness`/`web` 改动、把 `enhance` 设为默认开启。

## 13. 实施记录

### 13.0 进度快照（2026-10-24 07:24，恢复工作时从这里继续）

**当前工作区实测状态**（未提交，全部改动在工作区/未跟踪区）：

- `cargo check --workspace`：通过（仅 vendor/miniquad cfg 警告）。
- `cargo test --workspace`：除 `golden_hashes_match` 外全部通过（74 passed pxl-gen 等）。唯一失败见 B1。
- `pxl-art` 不在 workspace members 中 → **其代码从未被编译过**，测试也未运行过。

**已落地（代码在工作区，待验证）**：

| 文件 | 内容 | 状态 |
|---|---|---|
| `crates/pxl-core/src/scale.rs`（新） | `scale_copy` / `double_copy` / `epx2x`，块外透明 | 已编译、已测试（V0 主体） |
| `crates/pxl-core/src/color.rs`（+131 行） | `Oklab` 定点（Q16）`from_rgba`/`to_rgba`/`dist2`；查表 LUT | 已编译、随 workspace 测试 |
| 根 `Cargo.toml` | members 补 `pxl-engine`、`pxl-wasm` | 已生效 |
| `crates/pxl-cli/src/cmd/shapes.rs`（新） | `pxl shapes refine`（copy/corners 模式，写回双倍矩阵 spec） | 已编译；属前一计划遗留功能 |
| `crates/pxl-gen/src/shapes.rs`（+292 行） | `ellipse` / `polygon` / `thick line` 绘制原语 + 测试 | 已编译、已测试 |
| `crates/pxl-cli/tests/golden.rs`（+25 行） | 新增 `ascii.slime.*`（通过）、`ascii.brick.*`（失败，见 B1）哈希锚点 | 部分通过 |
| `crates/pxl-art/Cargo.toml` + `src/lib.rs` | no_std + alloc 骨架；声明并 `pub use` 七模块 | **未编译** |
| `crates/pxl-art/src/analyze.rs`（310 行） | `Material`（8 材质）/`Axis`/`SpriteGraph::build`（连通域、protect/thin/small 掩码、对称检测）+ 4 测试 | **未编译** |
| `crates/pxl-art/src/upscale.rs`（464 行） | `Mode`：copy/epx/scale3x/xbr/contour/hybrid；`chamfer`/`signed_distance`；hybrid 按材质分派；+4 测试 | **未编译**；contour 为距离场近似，非 4.2 矢量版 |
| `crates/pxl-art/src/cleanup.rs`（205 行） | orphan/elbow/notch 三规则 + `CleanReport` + 4 测试 | **未编译**；rhythm/symmetry 规则未做 |
| `crates/pxl-art/src/palette.rs`（282 行） | `RampOptions`（levels/hue_shift/max_colors/generate）、`build_shades` ramp 生成（复用近色、上限截断）+ `ShadeMap` + 4 测试 | **未编译**；`pxl palette ramp` CLI 未接 |

**缺失（lib.rs 已声明、必须补齐才能编译）**：

- `crates/pxl-art/src/pipeline.rs`：`enhance()` / `EnhanceOptions` / `EnhanceReport` / `OutlineMode` / `Upscale` / `Light` —— 编排 S1→S5 并产出报告。
- `crates/pxl-art/src/outline.rs`：外描边/selout/断线修补。
- `crates/pxl-art/src/metrics.rs`：`Metrics` 结构与 M1–M12。

**下一批工作（按序）**：

1. 写 `pipeline.rs` / `outline.rs` / `metrics.rs` 三个最小实现，让 `pxl-art` 编译过（第 13.0 节上表之外的接口不动）。
2. 根 `Cargo.toml` members 加 `crates/pxl-art`；`cargo test -p pxl-art` 通过。
3. 修 B1（brick_tile stamp 尺寸），重算 golden 哈希；`cargo test --workspace` 全绿 → V0 关闭。
4. 进入 V1：S7 背景自适应 + `enhance compare` + 指标输出到 CLI。

**注意**：`examples/ascii/` 整个目录未被 git 跟踪（含 slime/brick_tile）；`examples/reference-v3/` 大量 `pxlgen.png`/`reference.png` 被删除、recipe/shapes 被改动 —— 这些是早前计划的迁移产物，与本计划无关，提交时注意分开。

### 13.1 V0（已完成，2026-10-24 08:40）

- [x] `pxl-core/scale.rs` 三函数 + 导出。
- [x] OKLab 定点 `color.rs`（Q16，含 LUT）。
- [x] workspace members 补 engine/wasm/pxl-art，并在 `[workspace.dependencies]` 注册 `pxl-art`。
- [x] B1：`brick_tile` 的 legend `"1"` stamp 由 8×8 改为 2×2（`["44","55"]`），保持 `cell=[2,2]` 与 32 网格语义。
- [x] B2：补齐 `pxl-art/src/outline.rs`、`metrics.rs`、`pipeline.rs`；`cargo test -p pxl-art` 43 项通过。
- [x] `cargo test --workspace --no-fail-fast` 全绿。
- [ ] 指标表、同占地对照图（旧 48@2×、32-refine@3×、新 hybrid@k）、人工确认（待确认；归 V1）。

**本轮新增与修正**

- `outline.rs`：`OutlineMode`（keep/none/flat/selout/selout_lit）、`strip_outer`（只剥与透明相邻的外描边，内部描边色像素保留）、`apply`（1px 菱形外环 + 单像素断线修补；selout 取填充色 ramp 的 `deep`，selout_lit 在受光侧提亮一阶；无 ramp 的颜色回退到 `special.outline`）。
- `metrics.rs`：`Metrics` 含 M1–M12（`copy2x_bps` 为可选，奇数边不报；`symmetry_err` 仅对称精灵报），`entries()` 给出稳定顺序的 `(id, value)`，便于 CLI 打印和 diff。
- `pipeline.rs`：`enhance(native, palette, &EnhanceOptions) -> Enhanced { frame, palette, report }`。顺序：analyze → strip_outline → ramps（仅 selout 系）→ upscale → cleanup → outline → measure。`keep + copy + 无 cleanup` 与 `scale_copy` 逐字节一致（旧行为不变的回归）；cleanup 改动超过 5% 时进 `warnings`；ramps 触顶进 `warnings`。
- 修正了已写模块里的 bug：`upscale::signed_distance` 内侧距离取错了数组（`contour` 因此对内部像素给出错误符号）；`analyze::detect_symmetry` 只遍历了左半、统计的是半边差异，改为遍历全宽、只数 `x < w/2` 的镜像对。
- 修正了与实现不符的测试预期：`scale3x` 的中心像素在 3× 下展开为 3×3 实心块（而不是 1 像素）；"大块边角受保护" 的断言改为"软材质轮廓可圆角"（保护只来自 small/thin/硬材质）；`palette` 的 cap 用例按"首色需 3 个新阶"重算。
- `pxl-engine/tests/examples.rs` 两处把 `examples/tank/tank_body.shapes.json` 断言为 16×16；该素材已被早前的 32 网格迁移改为 32×32，断言同步为 32。
- golden：`hashes.json` 重算。已验证：用 HEAD 的代码 + 当前 examples 重算，55 个共有键与当前代码**完全一致**，即哈希变化 100% 来自素材（`tank_body` 迁移到 32×32 影响 `shapes.tank_body`/`op.*`/`macro.*`/`png`/`gif`，以及未跟踪的 `examples/ascii/*`），不是引擎行为漂移。

**已知遗留 / 风险**

- `examples/tank/*` 与 `examples/ascii/*` 仍是未提交改动；golden 的新哈希绑定这些素材，提交时需一并带上，否则 golden 会失败。
- `contour` 仍是距离场近似；`rhythm` / `symmetry` 清理规则、`banding` 修复未做（metrics 只度量）。
- `pxl-art` 尚未接入 CLI / engine，下一步是 V1。

### 13.2 V1（代码完成，人工确认待确认，2026-10-24）

- [x] `pxl enhance <input> -o out.png`：输入支持 ascii / shapes / `*.pxl.json` / png（png 先按整数倍还原为原生帧）；`--upscale`、`--scale`、`--outline`、`--cleanup`、`--background auto|checker|classic|#rrggbb`、`--metrics`。
- [x] `pxl enhance compare <input> -o sheet.png --variants … --scale k`：同占地拼图，首格为 copy 基线，背景默认 `checker`，每格标注算法名与指标；`--json` 输出每个变体的 M1–M12。
- [x] S7：背景走 `pxl-export` 的 `Background` / `composite_on`；旧调用不传 `--background` 时行为不变。
- [x] CLI 集成测试 `tests/cli_cases/enhance.rs` 4 项通过（compare 拼图、同输入同字节、缺参数 `E_USAGE`、不带 `--overwrite` 不覆盖）。
- [x] `cargo test --workspace --no-fail-fast` 全绿。
- [ ] 人工确认：`bomberman` 的 3× 对照图已生成并目测，未做正式评审，结论待确认。

**实现备注**

- `enhance` 与 `enhance compare` 共用一个 clap 命令，`input` / `-o` 在 clap 层可选、运行时校验，否则展开的必填位置参数会吞掉子命令名。
- `epx` / `xbr` 只支持 2×、`scale3x` 只支持 3×；`compare` 对不支持当前 `--scale` 的变体直接跳过（3× 拼图没有 epx/xbr）。

**已知问题**

- ~~2× 下 `contour` / `hybrid` 与 `copy` 相同~~：已在 V3 修正采样覆盖，见 13.4。
- ~~`scale3x` 在 `bomberman` 上出现孤立像素~~：已在 V3 修正边缘中心规则并接入 Protect，见 13.4。
- `enhance compare --rank`（计划 9 节）未做。

### 13.3 V2（代码完成，退出条件部分达成，人工确认待确认，2026-10-24）

- [x] `pxl palette ramp <palette> -o out.palette.json [--levels 4] [--hue-shift 40] [--max 36]`：为每个可见色（描边色除外）生成 shadow/light/deep 阶，写回 `ramps`（`auto_<idx>`）并追加颜色；已声明的 ramp 优先；触顶时跳过并告警。`pxl_art::palette::ramp_palette` 为库入口。
- [x] `pxl-art/src/shade.rs`：`ShadeMode::{None, Volume}`。每个材质区域做 3-4 chamfer 距离场，由梯度得法线，与光向点积后在边缘带内上/下移一阶 ramp；skin 只下移不上移；metal/glass 在深处加 1–2 像素高光；厚度 < 2px 的区域保持平；emissive 不动；描边像素不算区域成员。
- [x] `pxl enhance --shade none|volume`（默认 `none`，旧行为不变）；报告含 `shade.{volumes,flat,lit,shaded,speculars}`。
- [x] 测试：`pxl-art` 57 项（shade 10 项 + pipeline 3 项 + ramp_palette 1 项）；`cargo test --workspace --no-fail-fast` 全绿。

**3 个基准样例（scale 3，hybrid，shade none → volume）**

| 样例 | M10 value_range | M9 ramp_coverage | M7 outline_bg_dl | 其他变化 |
|---|---|---|---|---|
| slime | 32642 → 49602（+52%） | 10000 → 10000 | 2975 → 2975 | `symmetry_err` 0 → 102；`new_info_bps` 835 → 2152 |
| gem | 38642 → 44078（+14%） | 10000 → 10000 | 2975 → 2975 | `symmetry_err` 0 → 138；`new_info_bps` 1723 → 3331 |
| brick_tile | 19910 → 19910 | 10000 → 10000 | 2802 → 2802 | `lshape_count` 0 → 1 |

**退出条件复核（"M7/M9/M10 改善"）**

- M10 在 slime、gem 上改善，brick_tile 不变（平铺材质不该打光，预期如此）。
- M9 在这三个样例上 `shade none` 时已是 10000（p16 已声明 ramp、`selout` 默认开启会生成 ramp），没有提升空间，不能算"改善"。
- M7 只取决于描边色与预览背景，`shade` 不影响它；这一项由 V1 的 `--background auto` 解决，不属于 V2 的产出。
- 所以 V2 对 M10 有效，M9/M7 在这批样例上是持平，退出条件只达成一部分，如实记录。

**已知问题**

- ~~光照使对称精灵的 `symmetry_err` 误报~~：已在 V4 改为 ramp 归一后的镜像比较，slime 102 → 0、gem 138 → 0，见 13.5。
- `selout` 只替换与透明相邻的那一圈描边。素材描边是 2 像素厚（slime、gem 都是）时，内圈描边色保留，新外圈碰到的就是描边色，没有 ramp，于是回退成 `special.outline`，`flat`/`selout`/`selout_lit` 三种模式输出逐字节相同。用单像素描边或无描边素材，selout 才有可见差别（已用球体验证：外圈 `#223e7e`，`selout_lit` 受光面 `#3d5da4`）。需要决定是否把 `strip_outer` 改为剥掉整条描边带，留待 V5（S4 完整描边）。
- 声明 ramp 里排在最前面的颜色（如 `heat` 的索引 2），`deep`/`shadow` 钳在自身，selout 同样回退。
- AA（6.2 第 4 点）、Bayer 抖动（第 5 点）、cloth 褶皱未做。
- `enhance compare` 未加 `--shade` 之外的对比维度；panel 里还没有显示 shade 报告。

### 13.4 V3（代码完成，目标指标达成，人工确认待确认，2026-10-24）

- [x] `pxl enhance [--protect auto|none]`：默认 `auto`；`compare` 共用该参数。报告含 `protect.{enabled,pixels,small,thin}`，非法值返回 `E_USAGE`。
- [x] Protect 接入整条管线：S1 仍负责 small/thin/材质掩码；所有非 copy 放大器结束后恢复受保护原生像素块；cleanup 继续跳过这些像素。`none` 只关闭保护，保留 small/thin 供指标对照。
- [x] `scale3x` 改为标准 3×3 规则：四个角谓词驱动对应边缘中心，不再把对侧边缘色写入当前块；增加“右/下邻色不得写入上边缘中心”的回归测试。
- [x] 2× `contour` / `hybrid` 不再退化为 copy：采样覆盖整个原生像素，最外采样点到达原生像素角；增加 2× 色边重建单元测试。
- [x] CLI 集成测试直接解码 PNG，统计包含 Protect 在内的真实孤点，并验证 `auto/none` 的 M6 差异。

**3 个基准样例（`cleanup none`、`outline keep`、`shade none`；数值为 protect none → auto）**

| 样例 | 算法 | M4 `lshape_count` | M5 `orphan_count` | M6 `small_feature_kept_bps` | 真实孤点（含 Protect） |
|---|---|---:|---:|---:|---:|
| bomberman | scale3x@3 | 0 → 0 | 2 → 0 | 0 → 10000 | 2 → 0 |
| bomberman | xbr@2 | 10 → 0 | 2 → 0 | 0 → 10000 | 2 → 0 |
| bomberman | hybrid@2 | 2 → 0 | 0 → 0 | 0 → 10000 | 0 → 0 |
| castlevania_simon | scale3x@3 | 0 → 0 | 2 → 0 | 1111 → 10000 | 2 → 0 |
| castlevania_simon | xbr@2 | 22 → 0 | 2 → 0 | 0 → 10000 | 2 → 0 |
| castlevania_simon | hybrid@2 | 2 → 0 | 0 → 0 | 5555 → 10000 | 0 → 0 |
| battlecity_tank | scale3x@3 | 0 → 0 | 0 → 0 | 7500 → 10000 | 0 → 0 |
| battlecity_tank | xbr@2 | 0 → 0 | 0 → 0 | 7500 → 10000 | 0 → 0 |
| battlecity_tank | hybrid@2 | 0 → 0 | 0 → 0 | 5000 → 10000 | 0 → 0 |

- `hybrid@3` 的 M6 也分别从 0 / 5555 / 5000 提升到 10000，三样例 M4/M5 和真实孤点均为 0。
- 2× 下 `new_info_bps`：copy 均为 0；Protect auto 的 contour/hybrid 在 bomberman / castlevania_simon / battlecity_tank 上分别为 217 / 283 / 88，确认输出不再逐字节等于 copy。
- 已生成并查看三样例的 2× copy/contour/hybrid/xbr 同占地对照图；小特征保持完整，正式人工评审仍待确认。

**验证与边界**

- `pxl-art` 59 项测试通过；`cargo test --workspace --no-fail-fast` 共 484 项通过、0 失败。
- ~~`contour` 仍是距离场近似~~：V4 已换成 4.2 矢量轮廓重建，并接入 rhythm/symmetry，见 13.5。
- `enhance compare --rank` 仍未做，沿用 V1 的待办。

### 13.5 V4（代码完成，M3/M11 达成，M2 待人工复核，2026-10-24）

- [x] `pxl-art/src/contour.rs` 替换距离场近似：按颜色+材质提取 4 连通区域；以有向网格边界环实现等价 Marching Squares 追踪，对角 saddle 用右手转向避免并环；孔洞为独立环。
- [x] 每个闭合环先做规则/不规则直线阶梯识别，再做 0.5px 定点 RDP；直线保留均匀级差与 n/n+1 步长，连续至少 3 个同向缓转角用整数二次曲线拟合；metal/glass/outline 保留硬角。
- [x] 区域按扫描顺序回填、outline 最后；输出只使用源索引色。新引入且不重叠源孔洞的透明封闭分量会恢复，避免 Simon 的 1/3 像素假孔洞；Protect 最后逐原生块恢复。
- [x] `CleanOptions` 增加 `rhythm`、`symmetry`，`all` 默认包含；CLI 接受 `--cleanup rhythm,symmetry`，报告增加对应修改数并纳入 5% 告警。
- [x] rhythm 与 M3 共用轮廓 profile 和 `irregular_runs`：只对不规则阶梯重排内部步长，单调曲线不改，修改目标逐像素检查 Protect。
- [x] symmetry 先记录 S2 相对 copy 改动的原生块，再以偏离 copy 较少的一侧为多数侧镜像修复；不动未触及块和 Protect 目标。
- [x] M11 先把同一 ramp 的颜色归一为逻辑基色再比较镜像，消除 volume 光照的误报；真实轮廓/跨 ramp 差异仍计数。

**3 个基准样例（contour，Protect auto，outline keep，shade none）**

M3 为“V3 cleanup all → V4 cleanup all”；M11 为“V4 cleanup none → V4 cleanup all”。M6/M8 为最终值，M8 同时与 V3 对照。

| 样例@倍率 | M3 | M11 | M6 | M8 V3 → V4 | symmetry 修复像素 |
|---|---:|---:|---:|---:|---:|
| bomberman@2 | 0 → 0 | 7 → 0 | 10000 | 10000 → 10000 | 7 |
| bomberman@3 | 0 → 0 | 19 → 0 | 10000 | 10000 → 10000 | 19 |
| castlevania_simon@2 | 1 → 0 | —（非对称） | 10000 | 20000 → 20000 | 0 |
| castlevania_simon@3 | 0 → 0 | —（非对称） | 10000 | 20000 → 20000 | 0 |
| battlecity_tank@2 | 0 → 0 | 2 → 0 | 10000 | 10000 → 10000 | 2 |
| battlecity_tank@3 | 0 → 0 | 10 → 0 | 10000 | 20000 → 20000 | 12 |

- `shade volume` 的 M11：slime 102 → 0，gem 138 → 0（V2 记录值 → V4）。
- rhythm 在这三个样例上没有可修项（计数 0）；2-1-3 正例和单调曲线反例由单元测试覆盖。
- hybrid 在这三个样例上的 M3/M6/M8/M11 与 contour 相同。

**非目标指标复核**

| 样例@倍率 | M2 `new_info_bps` V3 → V4 | M1 `copy2x_bps` V3 → V4 |
|---|---:|---:|
| bomberman@2 | 217 → 22 | 9609 → 9960 |
| bomberman@3 | 97 → 19 | 9444 → 9461 |
| castlevania_simon@2 | 283 → 90 | 9433 → 9882 |
| castlevania_simon@3 | 127 → 93 | 9062 → 9040 |
| battlecity_tank@2 | 88 → 53 | 9824 → 9960 |
| battlecity_tank@3 | 43 → 47 | 9262 → 9236 |

- M1 的上升幅度均不超过 5%，其余未列指标持平；拓扑保护后 M8 不再退化。
- M2 除 tank@3 外下降，部分超过 5%，所以本里程碑**不宣称“所有非目标指标均不退化超过 5%”已通过**。旧距离场版本主要靠更多颜色替换提高 M2，新矢量版本更保守地改几何；需要人工同占地评审判断观感，而不是为提高 M2 故意增加变色。

**测试与人工确认**

- 几何测试覆盖：矩形/孔洞/对角 saddle、RDP、二次曲线、1:2 与 1:3 斜线均匀步长、中点圆边界误差 ≤ 1 原生像素、新孔洞恢复。
- 固定种子 500 张随机 16×16 帧 × 2/3/4 倍：尺寸、确定性、颜色集合均通过，测试约 2.53 秒。
- `pxl-art` 71 项通过；CLI 增加 contour symmetry 端到端与 cleanup 参数测试；`cargo test --workspace --no-fail-fast` 共 497 项通过、0 失败；构建 0 警告，修改文件 rustfmt 检查通过。
- 已生成并查看 bomberman、castlevania_simon、battlecity_tank 的 2× 同占地对照图及 bomberman 的 3× 图；未见新孔洞或明显不对称，小特征完整。正式人工评审与 M2 取舍仍**待确认**。

