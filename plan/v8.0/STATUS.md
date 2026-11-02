# v8.0 完成状态表

> 日期：2026-11-02（Asia/Shanghai）  
> 诚实标准：验收项真正落地才写「已完成」；主路径有、边角缺写「部分完成」；M9 默认「后续」。

| 分册/里程碑 | 状态 | 证据 |
|---|---|---|
| M0 基线 | 已完成 | `plan/v8.0/checklists/EULA.md`、`cli-flag-matrix.md`、fixtures 目录、crate 名冻结 |
| M1 文档模型 `00`/`01` | 已完成 | crate `libs/pxl-ase-doc`；测试 linked cel / bridge hash / tag-slice-grid |
| M2 文件 IO `06` | 已完成 | crate `libs/pxl-ase-io`；roundtrip indexed+RGBA；fixture `tests/fixtures/mini_indexed.aseprite` |
| M3 颜色 `02` | 已完成 | `pxl-ase::color` convert+dither；CLI `--palette`/`--dithering-*`/`--color-mode`；GPL+ACT；`pxl-art` Bayer8 |
| M4 像素/滤镜 `03`/`04` | 已完成 | flood-fill；hue-saturation；crop/canvas-size/trim；既有 invert/replace/brightness-contrast/outline；ops flip/rotate90；选区 expand/contract/任意 mask **延期**（非 P0 CLI 主路径） |
| M5 合成导出 `05` | 已完成 | flatten；`ase convert` PNG/GIF；`ase sheet` layout 别名+tag direction/slices meta；`--filename-format` 子集 |
| M6 时间轴 `07` | 已完成 | Tag/Slice IO；`list-layers/tags/slices`；`split-slices`；`--tag`/`--frame-range`；linked cel 保持 |
| M7 CLI `08` | 部分完成 | P0 导出旗标：`--trim`/`--padding`/`--extrude`/`--crop`/`--merge-duplicates`、filename-format 扩展；`--trim-by-grid`/`--play-subtags` **延期**；顺序敏感官方 argv 解析器 **延期**（clap 子命令子集，见矩阵） |
| M8 集成 `09` | 已完成 | `ase import` → `import-manifest.json` + schema-valid `asset.recipe.json` + palette/frames/sheet；pilot 覆盖 flood-fill/hue-sat/flags/import；`steps/README` 已更新；未改 `games/`；P0 缺口勾选见 `09` §4.3 |
| M9 脚本 `10` | 后续 | 未实现脚本宿主；`--script` 留 M9 |
| `00` 总览 | 部分完成 | 架构与 crate 地图已落地（合并 color/filters/ops/render/import 进 `pxl-ase`） |
| `README` | 已更新 | 里程碑总表已标注 |
