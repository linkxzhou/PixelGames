# v8.0 计划索引

> 修订：2026-11-02（Asia/Shanghai）  
> 主题：**以 clean-room 方式，用 Rust CLI 对齐 Aseprite 的无头文档模型 / 像素与调色板 / I/O / 滤镜 / 合成导出 / 时间轴能力**  
> 范围：只写本目录计划与 `plan/README.md` 索引；**不修改** `games/`、`steps/` 源码、`thirdparty/`；不 commit / 不 push。  
> 状态：工程已启动（2026-11-02 Asia/Shanghai）。里程碑见下表；细节与证据见 [STATUS.md](./STATUS.md)。

## 文档入口

| 文档 | 作用 |
|---|---|
| [00-总览与架构.md](./00-总览与架构.md) | 目标、crate 地图、与 Aseprite 层对应、CLI-first、依赖顺序、非目标、EULA |
| [01-文档模型.md](./01-文档模型.md) | Sprite/Layer/Cel/Frame/Tag/Slice/Grid/色模式 |
| [02-颜色与调色板.md](./02-颜色与调色板.md) | palette、remap、量化、色空间/profile 的 CLI 化部分 |
| [03-像素算法与编辑操作.md](./03-像素算法与编辑操作.md) | 无头等价的 brush/transform/selection → op DSL / 命令 |
| [04-滤镜.md](./04-滤镜.md) | 对齐 `src/filters/` |
| [05-合成渲染与导出.md](./05-合成渲染与导出.md) | 对齐 `src/render/` + spritesheet/gif/png |
| [06-文件格式IO.md](./06-文件格式IO.md) | `.aseprite` 优先；PNG/GIF/TGA/FLI 等 |
| [07-时间轴动画标签切片.md](./07-时间轴动画标签切片.md) | frames/tags/slices/linked cels |
| [08-CLI命令面与批处理.md](./08-CLI命令面与批处理.md) | 对齐公开 CLI；主方案 `step-300 ase` |
| [09-与steps集成与迁移.md](./09-与steps集成与迁移.md) | 挂入 step-300、契约、试点、缺口清单 |
| [10-脚本与扩展.md](./10-脚本与扩展.md) | 可选脚本宿主（部分完成） |
| [sources.md](./sources.md) | 本地路径证据 + 公开 URL |

建议阅读顺序：`README` → `00` → `01`/`06`（模型与 IO 地基）→ `02`/`03`/`04` → `05`/`07` → `08`/`09`；`10` 不阻塞主线。

## 一句话结论

**不做 Aseprite GUI 克隆，不做源码搬运。** 对照本地 `thirdparty/aseprite/src/README.md` 分层与公开 [CLI 文档](https://www.aseprite.org/docs/cli/)、公开 [文件格式规格](../../thirdparty/aseprite/docs/ase-file-specs.md)，用 Rust 在 `step-300` 旁增量建设 **文档模型 + CLI 批处理**；`pxl-core` / `pxl-ops` / `pxl-export` / `pxl-cli` 已有能力必须标注复用，缺口单独立里程碑。主体产品受 EULA 约束：**禁止复制 Aseprite 源文件/资源进产品**；实现采用 clean-room 功能对齐。

## 里程碑总表（跨文档）

| 代号 | 主题 | 依赖 | 状态 |
|---|---|---|---|
| M0 | 基线：清单、golden 夹具目录、契约草图、EULA 检查表 | 无 | **已完成** — EULA.md + fixtures + crate 名 |
| M1 | `pxl-ase-doc` 文档模型（RGBA/Indexed/Grayscale） | M0 | **已完成** — `libs/pxl-ase-doc` 单测通过 |
| M2 | `.aseprite` 读（写可选紧随）+ PNG 往返 | M1 | **已完成** — `pxl-ase-io` roundtrip + PNG 导出 |
| M3 | 调色板/量化/remap + 色模式转换 CLI | M1–M2 | **已完成** — dither/GPL/ACT + `--palette`/`--dithering-*` |
| M4 | 像素 op / 滤镜无头批处理 | M1, M3 | **已完成** — flood-fill/hue-sat/crop/canvas-size + 既有滤镜/ops；选区 mask 延期 |
| M5 | 合成渲染 + sheet/gif/png 导出对齐公开 CLI | M1–M2, 部分 M4 | **已完成** — PNG/GIF/sheet layout+meta + filename-format 子集 |
| M6 | 时间轴 tags/slices/linked cels + list-* | M1–M2 | **已完成** — list-slices/split-slices/frame-range/tag direction |
| M7 | `step-300 ase` 命令面与现有 `sheet`/`op`/`anim` 共存 | M2–M6 | **已完成** — trim-by-grid / play-subtags / 最小 `ase run` 顺序 argv（非官方全集） |
| M8 | step-300 集成试点与契约；缺口关闭清单 | M7 | **已完成** — `ase import` + pilot；§4.3 P0 关闭或显式延期 |
| M9 | （可选）脚本宿主 / 扩展 API | M7+ | **部分完成** — Rhai + `ase script`；shell/MCP 后续 |

## 与 v6 / v7 / steps 的关系

- **v6.0**：通用像素美术与 V6-B 回流继续独立；本版提供可批处理的「接近 Aseprite 文档语义」工具，不替代美术计划。
- **v7.0**：现代 / NES 规格与素材族合同继续独立；v8 的文档模型可为后续「从 `.aseprite` 导入再编译进 recipe」提供入口。交叉引用见 [v7.0 README](../v7.0/README.md)。
- **steps**：生产主链仍是 `step-100`→`200`→`300`→`400`→`500`。本版增量落在 `step-300-asset-build` 旁的 `pxl-ase-*` crates 与 `pxl-cli` 的 `ase` 子命令组；**不改**现有 `gen`/`build`/`plan` 语义除非 09 明确写兼容策略。

## 边界（全集）

- **不做**：`src/ui`、`src/app` 编辑器 UI、桌面集成（`desktop/`）、Steam（`steam/`）、updater、net 检查更新、多编辑器工作区、压力笔/手绘会话、实时洋葱皮预览窗口。
- **不做**：把 `thirdparty/aseprite` 源码、资源、`data/` 皮肤复制进仓库产品路径。
- **不做**：宣称「像素级复刻所有内部算法」；公开行为与格式/CLI 语义对齐优先，内部实现可不同。
- **验收**：未跑检查、未留证据的条目一律 `NOT_ASSESSED`，不得写 PASS。

## 法律摘要

Aseprite 发行物主体为 [EULA.txt](../../thirdparty/aseprite/EULA.txt)（禁止分发拷贝、限制逆向等）。部分子库在其自身 `LICENSE.txt` 下声明 MIT（如本地核实的 `src/doc/LICENSE.txt`、`src/dio/LICENSE.txt`、`src/render/LICENSE.txt`），**不等于**整包可嵌入。本计划约定：

1. **Clean-room**：按公开文档与可观察 CLI/文件行为实现；不粘贴 C++ 实现进 Rust。
2. 可参考公开模块分层概念与公开格式字段名；接口命名避免与受版权保护的独特表述混淆时，优先使用自有前缀 `AseDoc` / `pxl_ase`。
3. 产品分发路径不得包含 Aseprite 二进制或专有资源。
