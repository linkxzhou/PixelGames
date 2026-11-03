# EULA / clean-room 检查表（M0）

> 日期：2026-11-02（Asia/Shanghai）  
> 状态：工程基线已落地；法务最终确认仍为维护者责任。

## 可做

- [x] 阅读公开规格 `thirdparty/aseprite/docs/ase-file-specs.md` 并按字段实现编解码
- [x] 对照公开 CLI 文档行为实现 `step-300 ase` 子集
- [x] 参考本地分层 README 做架构对应（不复制实现）
- [x] 自造最小合法 `.aseprite` 夹具（程序生成，非专有测试资源）

## 禁止

- [x] 不把 `thirdparty/aseprite/src/**` 的 C++/头文件粘贴进 `libs/`
- [x] 不拷贝 `data/` 皮肤、图标、专有资源到产品路径
- [x] 不分发 Aseprite 二进制
- [x] 产品代码无 `include!` / bindgen 勾进第三方源树

## 冻结 crate 名（M0）

| Crate | 路径 | 职责 |
|---|---|---|
| `pxl-ase-doc` | `libs/pxl-ase-doc` | 文档模型 |
| `pxl-ase-io` | `libs/pxl-ase-io` | `.aseprite` 读写 |
| `pxl-ase` | `libs/pxl-ase` | facade：color/filters/render/ops + 批处理编排 |
| `pxl-cli` | `libs/pxl-cli` | `step-300 ase` 子命令组 |

合并说明：`pxl-ase-color` / `pxl-ase-filters` / `pxl-ase-ops` / `pxl-ase-render` 以模块形式并入 `pxl-ase`，逻辑边界保留。

## 依赖边界

- `libs/pxl-ase-*` **不得** path 依赖 `steps/`
- 复用 `pxl-core` / `pxl-export` / `pxl-ops` / `pxl-art`
