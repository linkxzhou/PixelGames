# v8.0 参考来源与代码证据

> 配套：本目录全部计划  
> 修订：2026-11-02（Asia/Shanghai）  
> 证据分层：公开文档 / 本地 Aseprite 树只读观察 / PixelGames `steps` 源码事实。三者不可互相冒充。  
> 本次为静态阅读与目录列举；**未**编译 Aseprite，**未**运行产品测试。实现验收均为 `NOT_ASSESSED`。

## 1. 法律与许可

| 路径 | 说明 |
|---|---|
| `thirdparty/aseprite/EULA.txt` | 主体 EULA：许可非出售；限制分发拷贝、逆向等；源码仅可个人目的编译/修改或向项目贡献 |
| `thirdparty/aseprite/README.md` | 产品介绍；指向 CLI / 色模 / 导出等公开文档 |
| `thirdparty/aseprite/src/doc/LICENSE.txt` | Document Library 本地 MIT 声明 |
| `thirdparty/aseprite/src/dio/LICENSE.txt` | Document IO 本地 MIT 声明 |
| `thirdparty/aseprite/src/render/LICENSE.txt` | Render 本地 MIT 声明 |
| `thirdparty/aseprite/src/view/README.md` | View 库说明指向 EULA |

**计划结论**：即便部分子树宣称 MIT，整包产品仍受 EULA 约束；PixelGames **不得**复制源文件/专有资源进产品路径。实现 = clean-room + 公开规格/公开 CLI 行为。

## 2. 公开 URL

| URL | 用途 |
|---|---|
| https://www.aseprite.org/docs/cli/ | 官方 CLI 全旗标与用例（本次 WebFetch 取得正文） |
| https://www.aseprite.org/docs/ | 色模、时间轴、导出等概念入口 |
| https://www.aseprite.org/docs/scripting/ | Lua 脚本（对应计划 10，后续） |

## 3. 本地 Aseprite 结构证据

### 3.1 分层总表

路径：`thirdparty/aseprite/src/README.md`

- Level 0：fixmath、flic、undo、observable、laf/*、steam…  
- Level 2：`doc/`、`ui/`（跳过）、`updater/`（跳过）  
- Level 3：`dio/`、`filters/`、`render/`、`view/`  
- Level 4：`app/`、`desktop/`（跳过）  
- Level 5：`main/`（跳过）

### 3.2 文档模型

目录：`thirdparty/aseprite/src/doc/`

关键文件（`ls` 核实）：`sprite.h`、`layer.h`、`cel.h`、`image.h`、`palette.h`、`tag.h`、`slice.h`、`grid.h`、`color_mode.h`、`blend_mode.h`、`remap.h`、`rgbmap*.h`、`algorithm/`（flip、floodfill、rotsprite…）、`file/{gpl,pal,act,col,hex}_file.*`

`color_mode.h`：`RGB` / `GRAYSCALE` / `INDEXED` / `BITMAP` / `TILEMAP`。

### 3.3 IO

- `thirdparty/aseprite/src/dio/`：`aseprite_decoder.*`、`aseprite_encoder.*`、`detect_format.*`；README 称 decode Document。  
- `thirdparty/aseprite/docs/ase-file-specs.md`：公开二进制规格（Header magic `0xA5E0`，色深 32/16/8 等）。  
- `thirdparty/aseprite/src/app/file/`：`ase_format`、`png_format`、`gif_format`、`tga_format`、`fli_format`、`psd_format` 等。

### 3.4 滤镜

`thirdparty/aseprite/src/filters/`：`outline_filter`、`replace_color_filter`、`hue_saturation_filter`、`invert_color_filter`、`brightness_contrast_filter`、`convolution_matrix_filter`、`median_filter`、`color_curve_filter` 等（CMakeLists 列出库成员）。

### 3.5 渲染

`thirdparty/aseprite/src/render/`：`render.*`、`quantization.*`、`ordered_dither.*`、`error_diffusion.*`、`rasterize.*`、`onionskin_*.h` 等。

### 3.6 CLI 开关源

- `thirdparty/aseprite/src/app/cli/app_options.h`：`saveAs`、`palette`、`scale`、`sheet*`、`split*`、`layer`、`tag`、`trim*`、`list*`…  
- `thirdparty/aseprite/src/app/cli/app_options.cpp`：注册字符串 `--batch`、`--save-as`、`--sheet`、`--color-mode`、`--dithering-algorithm`、`--filename-format`、`--list-layers` 等。  
- 同目录：`cli_processor.*`、`cli_open_file.*`、`default_cli_delegate.*`

### 3.7 明确跳过的树

`src/ui/`、`src/desktop/`、`src/steam/`、`src/updater/`、`src/net/`、`src/main/`（作为 GUI 入口）、以及 `app/commands` 中纯 UI 命令。

## 4. PixelGames steps 证据

根：`steps/step-300-asset-build/`

| Crate | 路径 | 核实要点 |
|---|---|---|
| pxl-core | `pxl-core/src/{frame,palette,outline,scale,color,lib}.rs` | `IndexFrame`、`Palette`、`paint_outline`、`MAX_DIM=256`、`MAX_ANIM_FRAMES=16` |
| pxl-format | `pxl-format/src/{ops,model,recipe,sprite,schema}.rs` | `Op` 枚举；`PxlDoc`；schema 含 `AsepriteHashSheet` 标题 |
| pxl-ops | `pxl-ops/src/{lib.rs,apply/*,macros/catalog.rs}` | `apply_op`；`MACRO_NAMES` |
| pxl-export | `pxl-export/src/{png,gif,sheet,lib}.rs` | PNG/GIF；`sheet_json` 写 `frameTags`；注释称 Aseprite hash-format |
| pxl-art | `pxl-art/src/{quantize,palette,rotsprite,…}.rs` | `median_cut`、`map_to_palette` |
| pxl-gen / pxl-svg / pxl-qa / pxl-engine / pxl-audio | 各自 `src/` | 生成/QA/音频等旁路能力 |
| pxl-cli | `pxl-cli/src/lib.rs`、`cmd/*` | 二进制名在 `Cargo.toml` 为 `step-300`；子命令见 `Cmd` 枚举 |

Workspace 成员登记：仓库根 `Cargo.toml` `[workspace.members]` 含上述 `libs/pxl-*`。

`rg` 结论：无 `.aseprite` 二进制编解码实现；仅 JSON sheet / 文档字符串级提及。

## 5. 本目录计划文件

见 [README.md](./README.md) 入口表。全部为 2026-11-02 文档交付；工程未实施。

## 6. 哪些是建议而非既成事实

- crate 切分名称（`pxl-ase-doc` 等）  
- `step-300 ase` 为 CLI 主方案  
- 里程碑 M0–M9 工期与并行度  
- Blend 模式实现子集顺序  
- 脚本宿主语言选择  

以上均可在 M0 冻结时微调，但不得撤销：EULA/clean-room、无 UI、steps 对照标注、公开 CLI 对齐责任。
