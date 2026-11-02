# v8.0 完成状态表

> 日期：2026-11-02（Asia/Shanghai）  
> 诚实标准：验收项真正落地才写「已完成」；主路径有、边角缺写「部分完成」；M9 默认「后续」。

| 分册/里程碑 | 状态 | 证据 |
|---|---|---|
| M0 基线 | 已完成 | `plan/v8.0/checklists/EULA.md`、`cli-flag-matrix.md`、fixtures 目录、crate 名冻结 |
| M1 文档模型 `00`/`01` | 已完成 | crate `libs/pxl-ase-doc`；测试 linked cel / bridge hash / tag-slice-grid |
| M2 文件 IO `06` | 已完成 | crate `libs/pxl-ase-io`；roundtrip indexed+RGBA；fixture `tests/fixtures/mini_indexed.aseprite` |
| M3 颜色 `02` | 部分完成 | `pxl-ase::color` convert/remap/quantize；CLI `--color-mode`；缺完整 dither CLI 矩阵 |
| M4 像素/滤镜 `03`/`04` | 部分完成 | `filters::outline_flatten` / `replace_index`；`ops::apply_op_to_frame`；CLI `ase filter`；非全滤镜集 |
| M5 合成导出 `05` | 部分完成 | `render::flatten_frame_rgba`；`ase convert` PNG；`ase sheet`；未挂 ase→gif 子命令 |
| M6 时间轴 `07` | 部分完成 | Tag/Slice 读写与 `list-layers`/`list-tags`；`--tag` 过滤；缺 split-slices 等 |
| M7 CLI `08` | 部分完成 | `step-300 ase --help` + inspect/convert/sheet/list-*/filter；见 `cli-flag-matrix.md` |
| M8 集成 `09` | 部分完成 | `work/v8-ase-pilot/`；`steps/README` 提及 `ase`；未改 games/；契约扩展未全做 |
| M9 脚本 `10` | 后续 | 未实现脚本宿主 |
| `00` 总览 | 部分完成 | 架构与 crate 地图已落地（合并 color/filters/ops/render 进 `pxl-ase`） |
| `README` | 已更新 | 里程碑总表已标注 |
