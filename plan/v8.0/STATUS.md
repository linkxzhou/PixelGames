# v8.0 完成状态表

> 日期：2026-11-02（Asia/Shanghai）  
> 诚实标准：验收项真正落地才写「已完成」；主路径有、边角缺写「部分完成」；M9 默认「后续」。

| 分册/里程碑 | 状态 | 证据 |
|---|---|---|
| M0 基线 | 已完成 | `plan/v8.0/checklists/EULA.md`、`cli-flag-matrix.md`、fixtures 目录、crate 名冻结 |
| M1 文档模型 `00`/`01` | 已完成 | crate `libs/pxl-ase-doc`；测试 linked cel / bridge hash / tag-slice-grid |
| M2 文件 IO `06` | 已完成 | crate `libs/pxl-ase-io`；roundtrip indexed+RGBA；fixture `tests/fixtures/mini_indexed.aseprite` |
| M3 颜色 `02` | 已完成 | `pxl-ase::color` convert+dither；CLI `--palette`/`--dithering-*`/`--color-mode`；GPL+ACT；`pxl-art` Bayer8 |
| M4 像素/滤镜 `03`/`04` | 部分完成 | filters: outline/replace-index/replace-color/invert/brightness-contrast；ops flip/rotate90 全帧；缺 flood-fill 等 |
| M5 合成导出 `05` | 已完成 | flatten；`ase convert` PNG/GIF；`ase sheet` layout 别名+tag direction/slices meta；`--filename-format` 子集 |
| M6 时间轴 `07` | 已完成 | Tag/Slice IO；`list-layers/tags/slices`；`split-slices`；`--tag`/`--frame-range`；linked cel 保持 |
| M7 CLI `08` | 部分完成 | 公开旗标子集已挂（见矩阵）；顺序敏感全集/trim/padding/script 未做 |
| M8 集成 `09` | 部分完成 | `work/v8-ase-pilot/` 扩展（GIF/dither/slices/filters）；`steps/README` ase 指针；未改 games/；import 契约未全做 |
| M9 脚本 `10` | 后续 | 未实现脚本宿主 |
| `00` 总览 | 部分完成 | 架构与 crate 地图已落地（合并 color/filters/ops/render 进 `pxl-ase`） |
| `README` | 已更新 | 里程碑总表已标注 |
