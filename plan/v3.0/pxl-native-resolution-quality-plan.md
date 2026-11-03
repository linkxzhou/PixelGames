# v3.0：原生分辨率与像素细节质量优化计划

> 日期：2026-10-23
> 状态：初始计划与历史实施记录见第 1–10 节；截图专项（仅 `crates`、2×2 真细化）见第 11 节，其实施记录在 11.9。
> 分析对象：初始分析为 `examples/hero_falcon/`；截图专项为 `examples/retro_sprites_48/` 及 `crates/` 中的 ASCII 编译、QA 与 CLI 链路。源码包含尚未提交的改动，以各阶段核验结果为准。
> 目标：解决“大像素块、细节不足、放大后显得模糊”，保留清晰、确定性的像素画风格。

## 1. 结论：不是 PNG 压缩模糊，而是低分辨率内容被大倍率展示

**`pxlgen.png` 虽然是 384×384，但实际仅包含 24×24 的逻辑像素信息：每个逻辑像素被精确复制为 16×16 的同色块。人物主体只占 236 个不透明逻辑像素。**

因此，本例首先要解决的是原生绘制信息量和展示尺度，不是换 PNG 编码器、提高压缩质量或加锐化。

推荐主路线：

1. 保留 24×24 版本作为低分辨率基线。
2. 用现有绘制能力制作 **48×48 原生精绘版**，重新设计轮廓、脸部、手部、服装结构和小型配件；不能只把旧坐标乘二。
3. 优先以 **24×24@4× 与 48×48@2×** 做相同 96×96 展示占地的比较；同时检查各自原生 1×图。
4. 正式展示关闭网格，继续使用最近邻整数倍缩放；网格只用于编辑检查。
5. 由精绘样板暴露的具体瓶颈驱动 `crates` 扩展，不先建设通用图像编辑器或自动超分引擎。

48×48 的像素位置预算是 24×24 的四倍，但这只是表达空间，不保证质量自动提升。若实际必须占据 384×384，48×48@8× 仍会有明显颗粒；需先确认这是编辑器放大视图还是实际游戏展示需求，再决定是否扩展到 96/128 原生尺寸。

## 2. 本次核验与证据边界

### 2.1 对现存图片的实际解码结果

使用本地 Pillow 解码统计，未重新生成或覆盖素材。

| 文件 | 实际尺寸 | 核验结果 |
|---|---|---|
| `examples/hero_falcon/pxlgen.png` | 384×384 | 18 种 RGBA 值，全部不透明；严格符合 16×16 同色块复制 |
| `examples/hero_falcon/out/hero_falcon.sheet.png` | 99×149 | 19 种 RGBA 值，含透明像素；这是原生图集，不是单帧原生分辨率 |
| `examples/hero_falcon/out/hero_falcon.preview.png` | 396×596 | 20 种 RGBA 值，全部不透明；对应 4×图集预览，包含网格 |
| `examples/hero_falcon/out/hero_falcon.walk.gif` | 96×96，4 帧 | 首帧为严格 4×同色块复制；首帧颜色统计为 18，保留透明背景 |

进一步将 `pxlgen.png` 按最近邻采样回 24×24：

- 236 个前景像素的 RGBA，与文档中 `base`、`left`、`idle_left_0`、`hurt_left_1` 完全一致。
- 340 个背景像素均为 `(28, 26, 24, 255)`，与预览合成器使用的背景一致。
- 这证明它的像素内容等价于该原生帧加背景后放大 16 倍；**不能据此断言它由哪条历史命令生成**。
- `palette.json` 有 26 个条目；“PNG 有多少种颜色”不能等同于主体使用了多少材质或多少有效细节。

### 2.2 限制

- 当前没有 `glance`，不提供未经核验的视觉评分或“已经更好看”的结论。
- 未检查浏览器、游戏运行时的 CSS/GPU 采样；外部二次插值只能列为待核查因素。
- 未运行 Cargo 测试、构建或重新执行 recipe；现有输出与当前源码的完整重现性需要在实施阶段验证。
- 工作区已有大量改动，实施时应基于当时工作区重新核对，不重置或覆盖这些改动。

## 3. 源码根因与已有能力

### 3.1 原生内容预算不足，而且缺少更精细的输入

```1:4:examples/hero_falcon/hero_falcon.shapes.json
{
  "w": 24,
  "h": 24,
  "draw": [
```

此文件仅有 57 条绘制指令，以矩形和单像素线为主。头部、手部和装备在很小的网格内相互覆盖，增加输出尺寸不会恢复没有绘制的信息。

`crates/pxl-gen/src/shapes.rs` 的 `generate_shapes` 直接按 `w/h` 建立索引帧，`draw_stamp` 在目标网格上栅格化，没有先绘制高清图再缩小的隐藏阶段。

`crates/pxl-format/src/model.rs` 的 `DrawCmd` 已支持 `Rect`、`Circle`、`Line`、`Pixels`、`Outline`。`Pixels` 能表达现有分辨率内的任意合法像素排列，因此“缺少 polygon”不是无法做精细样板的前置障碍。

### 3.2 预览放大不是内容升级

```151:156:examples/hero_falcon/hero_falcon.recipe.json
  "preview": [
    {
      "sheet": true,
      "scale": 4,
      "grid": true,
      "file": "hero_falcon.preview.png"
```

`crates/pxl-export/src/raster.rs` 的 `scale_nearest`、`composite`、`scale_rgba` 均复制源像素，没有双线性或双三次滤波。`composite` 还会将透明背景替换为深色，并可绘制网格。

4×预览开启网格时，每个 4×4 块最后一行和一列成为网格，占 7/16 的输出像素位置。这会强化网格感，并遮掉部分色块面积，但不是插值模糊。`pxlgen.png` 的精确同色块核验则表明它不带这类逐像素网格。

`crates/pxl-export/src/png.rs` 中 `Compression::Fast` 和 `FilterType::NoFilter` 属于无损 PNG 编码策略，不是视觉平滑滤镜；修改这些参数不会增加细节。

### 3.3 更高原生尺寸现在就能做，但相关坐标必须一起设计

- `crates/pxl-core/src/frame.rs`：`IndexFrame::new` 支持单帧边长 1–64；`canvas` 的 2048 上限用于图集等大画布，不能混为同一约束。
- 48×48、64×64 的 shapes 不需要先提高单帧上限。ASCII 文本入口当前有自己的 32/64 规格，不能把 shapes 的尺寸能力直接套用到所有入口。
- `hero_falcon.recipe.json` 中 `region`、`dx/dy` 写死在 24×24 网格。只改 `w/h` 会扩大空白；只倍增绘图坐标却不迁移动画区域，会导致局部动作错位。
- `crates/pxl-ops/src/parts.rs` 已有 `compose_rig`、`world_pivot`、分层和整数变换，可替代依赖固定矩形切片的脆弱局部动画。
- `crates/pxl-ops/src/apply/pixels.rs` 的 patch/mask 当前存在 16×16 限制；更精细的局部编辑需要分块复用或明确扩展。

### 3.4 现有着色不是自动细节生成

- `crates/pxl-ops/src/apply/color.rs` 的 `shade` 主要基于透明邻居沿 ramp 调色，不推断内部面朝向、服装褶皱或材质结构。
- 已有 ramp 选择和 outline 保护，不应重复规划已实现的修复。
- `texture` 当前将生成图按比例采样到 region；低分辨率纹理被铺大不等于新增细节。hero_falcon 没有使用这一路径，不能将它作为本例直接根因。
- `crates/pxl-gen/src/ascii/` 已有 stamp 和部分 pattern；大 cell 的纯色填充仍可能让高分辨率画布只承载粗布局，stamp 则可提供真实的 cell 内细节。

### 3.5 对使用者缺少统一的尺寸解释和质量反馈

- `crates/pxl-cli/src/cmd/generate.rs`：shapes 尺寸来自 spec，`GenArgs.size` 不控制 shapes 的原生尺寸。
- `crates/pxl-engine/src/session.rs`：`size/cell` 主要针对 ASCII text；已有 `DocInfo` 原生尺寸与 `RenderOutput` 输出尺寸，默认展示倍率为 8。
- CLI render 默认倍率为 1；跨入口比较应显式对齐倍率，不将默认值差异误认为算法差异。
- `crates/pxl-wasm/src/lib.rs` 是 engine 薄适配；当前 engine 的单文本输入不支持完整 recipe，不应宣称 CLI build 与 WASM 已拥有相同构建能力。
- 留存 `hero_falcon.build.json` 的 QA 为通过，但 `visual_review` 是 `not_run`。技术合法不代表达到目标美术质量。

## 4. 尺寸与输出契约

本节字段名是**设计提案**，不是当前可直接填写到 JSON 的 API。

### 4.1 必须分开的概念

| 概念 | 定义 | 本例 |
|---|---|---|
| 原生画布 `native_size` | 真正参与绘制的索引网格 | 24×24 |
| 内容边界 `content_bounds` | 不透明主体包围盒，用于识别无效扩画布 | 从帧像素计算 |
| 展示倍率 `display_scale` | 一个逻辑像素复制成多少输出像素 | `pxlgen.png` 为 16 |
| 输出尺寸 `output_size` | 实际 PNG/GIF 编码宽高 | 384×384 |
| 目标展示尺寸 `target_display_size` | 调用者预期的游戏/预览占地，可选 | 对照先取 96×96 |
| 输出用途 `purpose` | 透明资产或合成预览 | `asset` / `preview` |

设计约束：

1. 保留既有 `w/h` 和 `scale` 含义，不通过同名字段偷偷改变算法。
2. `native_size` 优先来自生成后的文档，不再引入另一份可能冲突的必填尺寸。
3. `display_scale` 只影响输出，不能改变原生帧或随机种子结果。
4. 新目标尺寸首先用于报告/诊断，不默认触发缩放、裁剪、补边或生成器重绘。
5. 对不支持的显式参数给出诊断；必须能区分“调用者显式传入”与“内部默认值”，避免 shapes 因默认 `size=32` 被错误拒绝。
6. 旧调用保留默认倍率以避免破坏兼容；新增明确 preset 或提示，不在本轮悄悄统一默认值。

### 4.2 透明资产与预览分离

- `asset`：保留透明度，不叠加网格或背景；图集仍以原生尺寸打包。
- `preview`：允许背景、网格、展示放大，结果必须注明不是更高精度资产。
- 两种输出共享 nearest 与尺寸预算校验，不各写一套采样逻辑。
- 已有 `render_preview_png` 明确属于预览。新增资产输出入口时保留旧 render 的默认行为，避免把兼容修复和视觉改进混在一起。
- 默认不引入 bilinear、bicubic、锐化、抖动或超分。以后若增加平滑预览，也只能显式选择，不能污染索引资产或黄金样本。

## 5. 分阶段实施任务

### M0：建立可复现基线与尺寸诊断（P0）

#### R0.1 hero_falcon 专项回归

- 位置：`crates/pxl-cli/tests/cli_cases/recipes.rs`、`crates/pxl-cli/tests/golden.rs`；必要时增加专用测试 fixture。
- 在独立测试输出目录重新构建现有 recipe，不覆盖示例 `out/`。
- 固定 base 索引帧、帧清单、动画时序和 sheet placements；核对留存 PNG/GIF 的解码像素。
- 区分“索引帧一致”“解码 RGBA 一致”“编码字节一致”，不因 PNG 压缩变化盲目更新所有黄金哈希。
- 验收：能够重现当前 24×24 内容；若留存产物与源码不同，先记录差异，不强行将任一方当作正确答案。

#### R0.2 统一尺寸与输出元数据

- 位置：`crates/pxl-engine/src/session.rs`、`crates/pxl-engine/src/render.rs`、`crates/pxl-cli/src/cmd/render.rs`、`crates/pxl-cli/src/cmd/build.rs`、`crates/pxl-wasm/src/lib.rs`。
- 在现有结果对象上增加原生尺寸、输出尺寸、倍率、nearest、grid、输出用途；build 区分单帧与整个 sheet 的尺寸。
- 添加“24×24 原生 → 384×384 输出，16×，不增加细节”这类可读说明。
- 参数诊断同步更新 `crates/pxl-cli/src/cmd/args.rs`、格式 schema 和 WASM options；只对明确不适用的显式参数提示，不改变默认调用。
- 验收：同一 shapes 输入经 engine/WASM/CLI render 返回一致的尺寸语义；旧字段和默认输出保持兼容。

#### R0.3 安全、清晰的导出入口

- 位置：`crates/pxl-core/src/frame.rs`、`crates/pxl-export/src/raster.rs`、`png.rs`、`preview.rs`、`sheet.rs`、`crates/pxl-engine/src/render.rs`。
- 将像素数量、RGBA 字节数、倍率乘法和 sheet 布局计算集中为 checked 路径；转换成 `u16/u32/usize` 前检查范围。
- 不仅检查最终阈值，还要确保计算阈值之前的乘法本身不会溢出；已有 `scale_rgba_checked` 也要覆盖极端输入。
- 收敛公开的未检查入口；旧 API 需要兼容包装或明确弃用期，不随意改变签名。
- 提供明确的透明资产导出模式；预览 grid 显式启用，质量对照使用无网格模式。
- 验收：超大尺寸/倍率/间距返回错误而非 panic、回绕或尝试超预算分配；透明资产不出现深色底和格线。

### M1：先用现有能力证明画质可以提高（P0，视觉收益核心）

#### R1.1 48×48 原生精绘样板

- 位置：未来新增独立的 `examples/hero_falcon_48/`，保留原示例；相关回归放在 `crates/pxl-cli/tests/`。
- 优先复用 shapes `pixels`、小范围 `patch_pixels`、已有 palette ramps、ASCII stamp、parts。
- 重新组织轮廓转折和负空间；补齐脸、手与服装之间的分界，使用少量有目的的 1–2 原生像素细节。
- 先产出合格静帧，再迁移左右向和动画；48 版的描述需要明确强调原生精绘而非旧图 nearest 放大。
- 旧坐标乘二只能作为布局草稿；描边、亮边和细节宽度单独设定，禁止把旧 1px 描边机械升级为全局 2px。
- 第一轮保留现有调色板以减少对照变量；若发现相邻部位对比不足，再建立单独的配色实验。
- 验收：输出 24@4 与 48@2 的 96×96 对照，各自原生 1×图，并记录用户视觉确认结果；缺少确认时不得记作画质已验收。

#### R1.2 动画坐标与局部编辑迁移

- 位置：配方、`crates/pxl-ops/src/parts.rs`、`crates/pxl-ops/src/apply/pixels.rs`、相关 op 测试。
- 同步检查 region、位移、支点、挂点、描边和裁剪边界。坐标倍率与动作幅度可以不同，不能全字段统一乘二。
- 为两倍分辨率的移动幅度建立初始映射，然后按动作重新校正；检查足底位置、部件连接和边缘裁切。
- 先使用现有 parts/rig，避免对整幅已合成图反复切片造成覆盖或留洞。
- 大 patch/mask 先明确透明字符表示“保持原像素”还是“擦除”；扩展尺寸时受目标帧和资源预算约束，而不是机械把 16 改成另一个魔法数字。
- 保留普通变换与 `compose` 重绘的既有 post 语义，防止重复着色/描边。
- 验收：静帧新增细节在所有动作帧中保持有效；方向翻转、局部移动、遮挡关系和帧尺寸正确。

### M2：补被样板证明有价值的表达能力（P1）

#### R2.1 几何表达与可选坐标变换

- 位置：`crates/pxl-format/src/model.rs`、`schema.rs`、`crates/pxl-gen/src/shapes.rs`。
- 按实际重复编辑量决定是否新增 polygon、ellipse、显式粗线；不要求一次全部实现。
- 新原语明确像素中心规则、端点/边界包含规则、裁剪、镜像、层序和确定性；默认不抗锯齿。
- 可选设计坐标变换不是 M1 前置条件。若实施，显式区分几何坐标和原生像素宽度，并覆盖 region/挂点的映射；不对任意 JSON 数值盲目缩放。
- 新语义使用显式版本/特性选择；旧 `gen_version:1` 和旧指令保持黄金结果。
- 验收：退化图形、负坐标、越界、镜像、重叠和极端坐标都有确定性测试；不得仅以“新增了原语数量”作为收益指标。

#### R2.2 材质区域与受控细节

- 位置：`crates/pxl-format/src/ops.rs`、`crates/pxl-ops/src/apply/color.rs`、`pixels.rs`，必要时扩展 `pxl-gen`。
- 先复用现有 region/ramp；只有矩形选择不能满足样板时再增加命名 mask/材质区域。
- 区域绑定明确 ramp、可选的面明暗和轮廓保护；细节层只写入允许区域，种子稳定。
- mask 在 flip/shift/compose 后必须与像素一起变换或明确重新计算，不能继续引用旧坐标。
- 禁止全图随机噪声充当“高清化”；不宣称邻居透明检测能恢复法线或材质。
- 验收：皮肤区域不会串到服装 ramp，outline 不被误改，同 seed 可复现，新增细节不破坏主体连通与动画稳定性。

### M3：质量检查与入口一致性（P1）

#### R3.1 增加分辨率相关 QA，但不自动打审美分

- 位置：`crates/pxl-qa/src/model.rs`、`config.rs`、`pixels.rs`、`validate.rs` 及规则测试。
- 硬约束：声明的原生尺寸/输出尺寸不一致、非法倍率、无效索引、超预算、资产混入预览效果。
- 告警：相对于可选目标显示尺寸倍率过高、仅扩画布而主体覆盖率大幅降低、输入是精确块复制图。
- 输出内容包围盒、覆盖率、按 ROI 的边界/色块分布作为诊断，不把更多颜色、更碎像素或更高熵定义为更好。
- 精确块复制只能证明冗余复制，不证明美术质量差；合法复古 16×16/24×24 资产应能通过对应 profile，不被全局强制升级。
- 低对比、细节分布等启发式仅建议；人工审核记录与 `qa.ok` 分开保存。

#### R3.2 CLI / engine / WASM 契约一致

- 位置：`crates/pxl-engine/tests/examples.rs`、`crates/pxl-cli/tests/`、`crates/pxl-wasm/src/lib.rs` 与实际 WASM 运行测试。
- 对同一 shapes/palette/frame/scale，比较解码像素及元数据；不得用仅在 native 上执行 WASM wrapper 的测试代替真正的 WASM 检验。
- shape rendering 的一致性不要求先将完整 recipe build 搬到 engine。只有确有浏览器构建需求时，再单列基于 `Resolver` 的 recipe 编排抽取任务。
- 不将 QA 对 CLI 文件系统的依赖引入 WASM，不在前端或适配器重写另一套光栅化算法。

### M4：大于 64 的原生分辨率（P2，条件触发）

只有 48/64 精绘已通过同展示尺寸对照、且实际展示确实需要更细网格时再启动。

- 联动检查 `IndexFrame`、文本矩阵解析、`PxlDoc` 尺寸一致性、生成器、ops/mask、sheet 打包、QA 阈值、CLI、engine 输入字节预算及 WASM 内存预算。
- 候选规格是 96/128；不默认无限画布，不把 `canvas(2048)` 直接套到所有 sprite。
- 使用配置化尺寸/总像素/总帧预算；96/128 增加样板与压力测试后再确定默认限额。
- 记录不同尺寸、动画帧数和倍率的时间及内存基线。先测量再设性能门槛，不把本次未测量的数据写成承诺。
- 用户选择保留粗颗粒复古风时，不应被迫迁移。

## 6. Crate 责任与依赖顺序

| Crate | 核心职责 | 依赖 |
|---|---|---|
| `pxl-core` | 原生帧契约、checked 尺寸与资源预算，不加入展示偏好 | M0 |
| `pxl-format` | 参数/schema、显式新几何/材质语义与兼容 | 先定义契约，再扩展 |
| `pxl-gen` | 在原生网格上生成；复用 pixels/stamp，按需加原语 | M1 验证瓶颈后进入 M2 |
| `pxl-ops` | 局部编辑、parts、mask/材质与动画坐标一致性 | M1–M2 |
| `pxl-export` | nearest、透明资产/预览分离、PNG/GIF/sheet 输出安全 | M0 |
| `pxl-engine` | 共享尺寸解释、渲染请求及结果，不重复底层算法 | M0，贯穿后续 |
| `pxl-qa` | 硬约束与风格建议分离，面向目标的分辨率诊断 | M0 契约之后 |
| `pxl-cli` | 参数诊断、构建报告、样板回归 | M0–M3 |
| `pxl-wasm` | engine 薄适配、序列化结果与实际 WASM 一致性 | 跟随 engine |

最短交付路径：**R0.1 → R0.2/R0.3 与静帧样板并行 → R1.1 视觉确认 → R1.2 → 按需 R2 → R3 → 决定 R4 是否需要。**

为避免无关重构，完整 recipe engine 化、外部图像超分、3D 烘焙、运行时缩放改造不列为本轮必交项。

## 7. 验收矩阵

| 场景 | 判定方式 |
|---|---|
| 旧 24×24 资产 | 原生索引帧、动画时序、sheet placements 保持；既有黄金不无理由更新 |
| 放大不增细节 | 关闭 grid，1/2/4/8/16×结果逐块等于源像素；预览背景按约定单独处理 |
| 真正提高原生预算 | 48 精绘版不是旧 24 图的简单 2×复制；仍需人工确认，不能仅以此条件认定更好 |
| 控制对照变量 | 同角色、palette 初始一致、相同 96×96 占地、无 grid，并保留 1×原图 |
| 仅扩大画布 | 包围盒和内容覆盖率反映增加空白，不误报成细节提升 |
| 透明输出 | asset alpha 不丢失；preview 背景和 grid 只在显式路径出现 |
| 动画 | 区域、支点、方向、遮挡与边界正确；细节不因迁移消失或闪烁 |
| 参数 | 显式无效参数有提示；默认参数不误伤 shapes；非法尺寸、scale、pad 无 panic |
| 多入口 | shapes 渲染在 CLI、engine、实际 WASM 环境解码结果一致 |
| QA 适用性 | 低分辨率复古 profile 可合法通过；告警不自动变为美术失败 |

实施阶段建议先运行 `cargo test --locked -p pxl-core -p pxl-format -p pxl-gen -p pxl-ops -p pxl-export -p pxl-engine -p pxl-qa -p pxl-cli -p pxl-wasm`，再运行 `cargo test --locked --workspace` 与 `cargo fmt --all -- --check`。这些命令**本次未执行**；依赖或 target 缺失应报告，不自动重写 lockfile 或把跳过写成通过。实际 WASM 检验需在确认测试入口与 target 后另行执行。

## 8. 不推荐的捷径

- **继续增大 `scale`**：只会让像素块更大。
- **只将 `w/h` 改大**：通常只是增加透明空间。
- **所有坐标乘二后宣布高清化**：即便几何变大，也没有自动得到新的造型信息。
- **默认双线性/模糊/锐化**：可能减弱块感，但无法恢复缺失信息，还会破坏调色板与清晰边缘。
- **先改 PNG 压缩参数**：不影响原生细节。
- **先增加颜色和噪点**：增加数据量不等于增强主体可读性。
- **直接提高 64 上限**：48/64 尚未精绘验证前没有必要，同时会扩大预算和跨入口兼容风险。
- **以 QA 通过或图片尺寸很大作为画质完成证据**：必须有同展示尺寸对照与人工确认。

## 9. 完成定义

本轮完成需要同时具备：

1. 现有 24×24 基线可复现，尺寸与放大语义对使用者透明。
2. 48×48 原生精绘样板在 96×96 相同展示占地下经人工确认更清楚，且不牺牲目标像素风格。
3. 透明资产与编辑预览职责清晰；旧调用兼容，所有缩放和尺寸计算有安全边界。
4. 局部编辑与动画在更高原生分辨率下稳定，CLI/engine/WASM 的共享输入路径一致。
5. QA 分离工程合法性与视觉审核，不把尚未执行的检查标为通过。

## 10. 实施记录（2026-10-23）

M0、M1 已实施并经 48×48 样板人工确认；M2、M3 的新 QA 规则、M4 随后实施。

| 项 | 落地内容 | 位置 |
|---|---|---|
| M0 | `scaled_dims` 统一 64 位尺寸检查；`pack_sheet` 拒绝超 2048 并消除 `as u16` 回绕；`render_asset_png` 透明资产输出 | `crates/pxl-export/src/{raster,sheet,preview}.rs` |
| M0 | `OutputPurpose`（preview/asset）贯穿 engine/CLI/ascii/WASM；渲染结果带原生尺寸、倍率、grid、用途 | `crates/pxl-engine/src/{render,session}.rs`、`pxl-cli`、`pxl-wasm` |
| M1 | 48×48 原生精绘样板，动画 region 与位移按原生像素重排，预览 2× 无网格 | `examples/hero_falcon_48/` |
| M2 | 新增 `ellipse`（可选空心）、`polygon`（even-odd，i128 交点比较）、粗线（`line` + `width`，旧写法仍是 `line`） | `crates/pxl-format/src/model.rs`、`pxl-gen/src/shapes.rs` |
| M2 | 新增 `material`（按 ramp 归属在区域/掩码内换色）与 `detail`（位置哈希的确定性细节，带 inset） | `crates/pxl-format/src/ops.rs`、`pxl-ops/src/apply/pixels.rs` |
| M3 | QA 新增 `uniform_area_bps`、`block_corners`（Info 级度量）与可选 `content_coverage`（默认关闭，strict 下升级为 error） | `crates/pxl-qa/src/{resolution,config,validate}.rs` |
| M4 | `MAX_DIM` 64→128，新增 `MAX_FRAME_PIXELS`（128×128）预算；parts/bollinger/矩阵/线长跟随共享上限；mask 与 `patch_pixels` 由 16×16 放宽到画布上限；位移上限 ±128 | `crates/pxl-core/src/{lib,frame,text}.rs` 等 |

验证：`cargo test --workspace` 385 项通过、0 失败；既有黄金快照未更新。新增测试覆盖几何对称性与极端坐标、material/detail 的 ramp 归属与确定性、QA 的小图合法性与覆盖率告警、96×96 端到端构建。

仍未做或需注意：
- `rotate`（任意角度）依旧返回未实现；本轮 M2 只补了计划列出的几何与材质/细节能力。
- QA 不做展示倍率判断：帧本身不含"游戏会放大几倍"的信息，倍率由 `pxl render`/`build` 报告。
- 真实 WASM 运行测试仍只有原生包装层覆盖。
- 96×96 样例仅用于验证能力可用（`/tmp` 下构建），未提交为正式示例。

## 11. 截图专项：2×2 原生细化（仅 `crates`）

> 追加日期：2026-10-23。状态：按本节实施；实施结果记录在 11.9。
> 范围：只改 `crates/` 与 `examples/`。不涉及模型调用、成本预算与 `harness/`、`web/`。
> 决策：采用「2×2 真细化」为主方案；图像锐化、插值不做；Scale2x 类算法仅作为“草稿生成规则”的内部实现，不做成导出滤镜。

### 11.1 根因（已静态核验）

`examples/retro_sprites_48/` 的角色（`battlecity_tank`、`bomberman`、`castlevania_simon` 等）是 **16×16 字符布局，`cell:[3,3]`，`size:64`**：

- `crates/pxl-gen/src/ascii/mod.rs` `expand()`：Solid 分支把每个字符填成 3×3 同色像素，再由 `place()` 居中放入 64×64。
- 这些样例 `legend:{}`、`post:[]`，没有用 Pattern/Stamp 或后处理补 cell 内细节。
- 所以独立创作单元只有 16×16，主体约 48 像素宽。放大、锐化都不会增加结构信息。

“一块改成 2×2”必须是 **真细分**：创作网格从 16×16 变成 32×32，新增的子像素由作者独立编辑；仅复制成 2×2 不新增细节（基线）。

### 11.2 尺寸约定（避免再次混淆）

| 概念 | 旧样例 | 细化后 |
|---|---|---|
| 创作网格 | 16×16 字符 | 32×32 字符 |
| cell | 3×3 | **1×1** |
| 内容像素 | 48×48 | 32×32 |
| 画布 `size` | 64 | **32**（需要边距时 64，anchor 居中） |
| 同占地展示（96×96 主体） | 48@2× | 32@3× |

关键点：要让“块变小”，必须把 `cell` 从 3 降到 1；cell 为 3 的 32×32 网格会得到 96 像素，反而更大、仍是粗块。

### 11.3 现状与缺口（代码依据）

| 能力 | 状态 | 位置 |
|---|---|---|
| 32×32 创作网格、cell 1 | 已支持 | `AsciiSpec::validate` 允许 size 32/64，cell 1..=64；`pxl-format/src/ascii.rs` |
| 逐字符文本源（`::` 头、空行分块） | 已支持 | `pxl-format/src/ascii_text.rs` `parse_ascii_text` |
| `ascii import` / `ascii render` | 已支持 | `pxl-cli/src/cmd/ascii/mod.rs` |
| 把旧 16 网格变成 32 网格草稿 | **缺失** | 只能手工逐格改写，容易出错 |
| 草稿“非纯复制”（轮廓圆角） | **缺失** | 无 |
| 度量“是否只是 2× 复制” | 缺失（`block_corners` 只测局部） | `pxl-qa/src/resolution.rs` |

结论：不需要改 `pxl-core` 的 128 上限，也不需要放宽 ASCII 的 32/64 限制。缺的是一个把旧布局升级为细网格草稿的确定性工具，以及把真细化样例做出来并验证。

### 11.4 设计：`pxl ascii refine`

纯文本到文本的确定性转换，字符（调色板索引）不变，只改变网格密度。

```
pxl ascii refine <in.ascii.txt> -o <out.ascii.txt> [--mode corners|copy] [--charset digits|printable]
```

规则：

1. 解析沿用 `parse_ascii_text`（保留块名与 frame/layer 类型，输出时重写 `:: name [layer]` 头；空行分块）。
2. 每个字符 `P` 展开为 2×2 子块。
3. `copy`：四个子像素都等于 `P`。仅作为对照基线。
4. `corners`（默认）：EPX 规则，只在 `P` 的上/右/左/下邻居里选色，不引入新字符：

   ```
   A=上 B=右 C=左 D=下
   左上 = (C==A && C!=D && A!=B) ? A : P
   右上 = (A==B && A!=C && B!=D) ? B : P
   左下 = (D==C && D!=B && C!=A) ? C : P
   右下 = (B==D && B!=A && D!=C) ? D : P
   ```

   块外按透明处理；各 charset 的透明字符视为同一种（digits: `0`；printable: `.`、空格）。
5. 输出仍是合法 ASCII 文本源；之后由作者在 32×32 上手工逐像素精修（眼睛、手脚分离、斜线节奏、装备）。`corners` 草稿只负责去掉最明显的 2×2 方角，**不是最终细化**。
6. 行数/列数加倍；超过 `size` 上限的检查仍由 `ascii import` 完成，refine 本身不假设 size。
7. 不修改输入文件；默认拒绝覆盖已存在输出（沿用全局 `--overwrite`）。

落点：

- `crates/pxl-format/src/ascii_refine.rs`（新增，纯函数，无 IO）：`RefineMode`、`refine_text(&AsciiText, Charset, RefineMode)`、`format_ascii_text(&AsciiText)`。放在独立文件是因为 `ascii_text.rs` 已含解析与测试，保持单文件低于 600 行。
- `crates/pxl-format/src/lib.rs`：导出。
- `crates/pxl-cli/src/cmd/args.rs`：`AsciiRefineArgs`，并加入 `AsciiCmd::Refine`。
- `crates/pxl-cli/src/cmd/ascii/mod.rs`：`refine_cmd`，IO 在此，逻辑留在 `pxl-format`；JSON 输出含 `in_grid`、`out_grid`、`mode`、`hash`。

### 11.5 示例：三个真细化样本

新增 `examples/retro_sprites_32/`：`battlecity_tank`、`bomberman`、`castlevania_simon`。

流程：

1. 用 `pxl ascii refine --mode corners` 从 `retro_sprites_48/*.ascii.txt` 生成 32×32 草稿。
2. 手工精修草稿并提交 `.ascii.txt`；`.ascii.json` 由 `pxl ascii import --size 32 --cell 1 --palette <retro9 系调色板>` 生成。
3. 保持调色板与姿态不变，减少对照变量。
4. 重点修：坦克履带/炮管边缘；炸弹人头部圆形与脸；西蒙的头发、肩、鞭/盾轮廓。不做随机噪点。

调色板引用旧目录的 `shared/*.palette.json`，不复制文件（需验证 import 的相对路径解析；若不支持再复制）。

### 11.6 度量：区分“纯复制”与“真细化”

在 `crates/pxl-qa/src/resolution.rs` 增加 **Info 级度量** `frame.{name}.copy2x_bps`：当帧宽高均为偶数时，统计每个 2×2 对齐块内四个像素完全相同的块的占比（bps）。纯 2× 复制为 10000。仅度量，不报错，不进入 `qa.ok` 的失败判定；理由：合法的粗风格资产也可能很高。

对照用途：`copy` 草稿 → 10000；`corners` 草稿 → 低于 10000；精修后继续下降。

### 11.7 测试（先写后实现）

`crates/pxl-format`（单元，`ascii_refine.rs`）：

- `copy`：2×2 输入 → 4×4 输出，每个块逐像素等于原字符。
- `corners`：L 形/台阶形/孤立像素/横线/空输入的期望输出；输出字符集 ⊆ 输入字符集；确定性。
- 透明字符等价（printable 的 `.` 与空格）。
- 块名、layer 类型在 round-trip 后保留；`parse(format(x))` 的行列与内容一致。
- 边界：1×1、含 `::` 头、多块。

`crates/pxl-cli/tests/cli_cases/ascii.rs`（集成）：

- `ascii refine` 输出 32×32，且 `ascii import --size 32 --cell 1` 成功。
- `refine --mode copy` 编译后与 48 样例的“最近邻 2×”一致，`corners` 不一致。
- 拒绝覆盖已有输出；错误输入返回可读错误。
- 三个新示例都能 `ascii render`，画布 32×32，且不透明像素数大于对应 `copy` 草稿以外的差异（以与 `copy` 草稿的不同像素数下限断言）。

`crates/pxl-qa`：

- `copy2x_bps` 对 2× 复制帧为 10000，对奇数尺寸帧不输出。

回归：`cargo test -p pxl-format -p pxl-qa -p pxl-cli`；既有黄金哈希不得变化；`cargo fmt --all -- --check`。

### 11.8 任务与顺序

| 步骤 | 内容 | 退出条件 |
|---|---|---|
| T1 | `ascii_refine.rs` + 单元测试 | 测试通过 |
| T2 | CLI 子命令 + 集成测试 | `ascii refine` 与 import/render 贯通 |
| T3 | QA 度量 `copy2x_bps` | 测试通过，不影响现有 QA 结果 |
| T4 | 三个 32×32 精修示例 | 渲染成功；与 copy 草稿有实质差异 |
| T5 | 对照图（同 96×96 占地：旧 48@2×、新 32@3×、copy 草稿@3×） | 生成对照并交给用户做视觉确认 |

不做：128 上限改动、ASCII size 白名单扩展、导出滤镜（锐化/Scale2x）、`harness`/`web` 改动、批量改写其余 17 个角色（待三例确认后再推广）。

### 11.9 实施记录（2026-10-23）

T1–T4 已实施；T5 的对照图已生成并交用户确认，视觉结论待用户判定。

| 项 | 落地内容 | 位置 |
|---|---|---|
| T1 | 新增 `RefineMode`（`copy`/`corners`）、`refine_text`、`format_ascii_text`；EPX 只从四邻选取颜色，不新增调色板索引；透明字符按 charset 归一 | `crates/pxl-format/src/ascii_refine.rs`，导出在 `crates/pxl-format/src/lib.rs` |
| T2 | 新增 `pxl ascii refine <txt> -o <txt> [--mode copy\|corners] [--charset] [--no-directives]`；输出 `in_grid`/`out_grid`/`mode`/`blocks`/`hash` | `crates/pxl-cli/src/cmd/args.rs`、`crates/pxl-cli/src/cmd/ascii/mod.rs` |
| T3 | 新增 Info 级度量 `frame.{name}.copy2x_bps`：对齐 2×2 块四像素全同的占比（bps），奇数边长不输出；纯 2× 复制为 10000，仅度量不判定失败 | `crates/pxl-qa/src/resolution.rs` |
| T4 | 新增 `examples/retro_sprites_32/`：`battlecity_tank`、`bomberman`、`castlevania_simon`，`size:32`、`cell:[1,1]`，调色板引用本地 `shared/` 副本 | `examples/retro_sprites_32/` |

实测（相对同源 16 网格 `--mode copy` 的逐像素差异，共 1024 像素）：

| 样例 | 与纯 2× 复制的差异 | 说明 |
|---|---|---|
| `battlecity_tank` | 86 像素 | `corners` 草稿 28 + 手工补的履带分节/炮盾/尾部 |
| `bomberman` | 44 像素 | 仅 `corners` 草稿，未手工重绘 |
| `castlevania_simon` | 76 像素 | 仅 `corners` 草稿，未手工重绘 |

测试：`cargo test -p pxl-core -p pxl-format -p pxl-gen -p pxl-ops -p pxl-qa -p pxl-export -p pxl-engine -p pxl-wasm -p pxl-cli` 全部通过（含新增 11 项 refine 单元测试、3 项 CLI 集成测试、2 项 QA 度量测试）；`cargo fmt --all -- --check` 通过；既有黄金哈希未更新。

未做与已知不足（不掩盖）：

- `bomberman`、`castlevania_simon` 目前只是 `corners` 草稿。它们比纯 2× 复制细，但仍是“16 网格骨架 + 2×2 子像素”，尚未达到手工重绘 32 网格应有的细节量。真正达标需要在这两份草稿上逐部位精修（本轮手工重绘的尝试反而更差，已放弃）。
- 手工重绘仅完成坦克一例，且改动集中在履带分节、炮盾、尾部；车体比例仍沿用原布局。
- `refine` 只做 2× 文本栅格放大，不处理 `anims`、`post`、`delta patch` 与 `layers` 的坐标迁移；这些字段不属于 `.ascii.txt`，需在 spec 层另行调整。
- 其余 17 个角色未改写，等待三例视觉确认后再推广。
- 未运行视觉模型或人工美术评分；对照图的“更好看”结论未由本轮机器验证给出。
