# 架构设计（architecture，v2：CLI 优先）

> v2：先做 `pxl-core`（库）和 `pxl-cli`（二进制 `pxl`）。WASM 和 Web 编辑器**推迟**，以后复用 `pxl-core`。

## 1. Cargo workspace 布局（v2）
```
PixelGames/
├─ crates/
│  ├─ pxl-core     # 调色板、IndexFrame、Region/Mask、PRNG、文本矩阵序列化；no_std + alloc，便于以后编译 WASM
│  ├─ pxl-gen      # 生成器：bollinger（clean-room 重写）、shapes、noise-tile；后续加 parts、autotile
│  ├─ pxl-ops      # 变换 op 与动画宏：shift、row_shift、flip、rotate90、palette_swap、palette_cycle、outline、shade、dissolve、texture…
│  ├─ pxl-qa       # validate 规则集（default、tank…）
│  ├─ pxl-format   # serde 模型 + schemars 生成的 JSON Schema（pxl、ops、recipe、palette、sheet）
│  ├─ pxl-export   # PNG（png crate，indexed）、GIF（gif crate）、sheet 打包 + Aseprite 兼容 JSON
│  ├─ pxl-cli      # 二进制 `pxl`（clap），--json 输出、退出码；M2 加 `pxl mcp` 子命令
│  └─ (deferred) pxl-wasm   # 以后：wasm-bindgen 绑定 pxl-core/gen/ops/export
├─ assets/{palettes,templates,shapes}/
├─ demo/tank/{tank.palette.json,tank_body.shapes.json,tank.recipe.json,.miniagent/skills/pxl.md}
├─ tests/golden/   # recipe → sha256
├─ plan/
└─ thirdparty/     # 只读参考
```
依赖：`serde`、`serde_json`、`schemars`、`clap`、`png`、`gif`、`sha2` 或 `blake3`、`insta`。不引入 `image` crate，避免体积和不确定性。

## 2. 以后的 WASM（推迟）

- `wasm-bindgen` 加 `serde-wasm-bindgen`。主要接口：
  - `generate(recipe_json) -> AssetHandle`
  - `apply_ops(handle, ops_json)`
  - `frame_rgba(handle, anim, i, scale) -> Uint8ClampedArray`（零拷贝视图）
  - `qa(handle) -> json`
  - `export_png` / `export_gif` / `export_sheet -> Uint8Array`
- 性能目标：32×32 角色加 8 帧动画，生成加 QA 在 5ms 以内；64 个候选的拼接图在 50ms 以内【估】。
- 体积目标：`px-wasm` 经 wasm-opt 后 ≤ 300KB（不含 png/gif 编码时 ≤ 150KB）【估】。
- 同一套 crate 同时编译成 native 版（px-cli），服务器端用它渲染 PNG 给 Claude 看，保证和浏览器结果一致。

## 3. Agent 集成（v2 见 miniagent-integration.md；以下为 v1 的 Claude 方案，保留供参考）
- 浏览器 → `server/`（转发 Claude Messages API，使用 tool use）→ Claude 发出工具调用 → **工具在 server 端用 px-cli 或 wasmtime 执行**，渲染出 PNG，作为图片 content block 回传给 Claude。
- 结果配方同步回浏览器，浏览器用 WASM 本地重新生成（因为是确定性的，结果和服务器一致）。
- Prompt caching：模板目录、op 文档、示例配方放进缓存前缀。
- 也可以做纯前端的 BYOK 模式（工具在浏览器里执行），后续再做。

## 4. 第三方复用与许可

| 来源 | 许可 | 复用方式 | 注意事项 |
|---|---|---|---|
| zfedoran/pixel-sprite-generator（上游，未放在 thirdparty 中） | MIT（API 核实） | 按算法在 Rust 中重写 | 保留算法出处致谢 |
| MaartenGr/Sprite-Generator（thirdparty） | **无 LICENSE** | **只读理解，不复制代码** | 文中指出的 `is_vertical_gradient` 缺陷不要照搬 |
| Pixelorama（thirdparty） | MIT | 可以移植着色器算法（OffsetPixels、OutlineInline、Palettize、IndexMap、FlatToIsometric）为 Rust CPU 整数实现 | 在 `THIRD_PARTY_NOTICES` 中保留 MIT 版权声明 |
| cleanEdge（Pixelorama 内，© torcado 2022） | MIT（文件头声明） | 移植为 `rotate algo=cleanedge` | 保留文件头 |
| OmniScale（Pixelorama 内，出自 RetroArch 着色器） | 【未验证具体许可】 | 暂时不移植 | 先核实上游许可 |
| SmearRotxel（作者 Azagaya） | 【未验证】（随 Pixelorama MIT 分发，单独许可未知） | 暂时不移植 | — |
| Piskel（thirdparty） | Apache-2.0 | 参考 TransformUtils、Dithering、OnionSkin 的思路，用 Rust 重写；可以借用其测试用例的思路 | 如果移植代码，需要附 Apache NOTICE 并标明修改 |
| spritebrew（未下载） | AGPL-3.0 | **不复用代码**；只参考产品功能（切片、导出到多个引擎） | — |
| Celeste Player.cs（NoelFB，MIT） | MIT | 以后做物理时参考 | 与素材阶段无关 |

## 5. 技术选型
- crate：`serde`、`schemars`（生成 JSON Schema 给 Claude 的 tool `input_schema`）、`png`、`gif`、`rand_core` 或自写 PCG32、`wasm-bindgen`、`insta`（快照测试）。
- 不引入：图像大库（`image` crate 太大，可选）、GPU 依赖（素材生成全部在 CPU 上做整数运算）。
