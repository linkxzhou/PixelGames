# CLI flag 覆盖矩阵（v8.0 / M0–M8）

> 日期：2026-11-02（Asia/Shanghai）  
> 对照：https://www.aseprite.org/docs/cli/

| Flag / 子命令 | 状态 | 说明 |
|---|---|---|
| `ase --help` | 已完成 | clap 子命令组 |
| `ase inspect <file>` | 已完成 | 尺寸/色模/层/帧/tags/slices |
| `ase convert` / `--save-as` | 已完成 | PNG / GIF / `.aseprite`；多输入批处理 |
| `ase list-layers` | 已完成 | |
| `ase list-tags` | 已完成 | 含 direction |
| `ase list-slices` | 已完成 | |
| `ase split-slices` / `convert --split-slices` | 已完成 | 每 slice 裁切 PNG |
| `ase sheet` | 已完成 | PNG+JSON；`--layout` horizontal/vertical/packed |
| `ase filter` | 已完成 | outline / replace-index / replace-color / invert / brightness-contrast |
| `ase op` | 已完成 | flip / rotate90 跨帧 |
| `-b` / `--batch` | 兼容忽略 | 默认无 UI |
| `--preview` / `-p` | 已完成 | convert 干跑 |
| `--tag` | 已完成 | convert/sheet 过滤 |
| `--frame-range` | 已完成 | `0,2` / `1-3` |
| `--oneframe` | 已完成 | inspect/convert/list/sheet |
| `--color-mode` | 已完成 | rgba/indexed/grayscale |
| `--palette` | 已完成 | `.gpl` / `.act` |
| `--dithering-algorithm` | 已完成 | none/ordered/old |
| `--dithering-matrix` | 已完成 | bayer2x2/4x4/8x8 |
| `--scale` | 已完成 | convert/split-slices |
| `--filename-format` | 部分 | `{frame}` `{tag}` `{layer}` `{title}` `{path}` `{fullname}` |
| `--ignore-empty` | 已完成 | convert/sheet |
| `--all-layers` / `--layer` | 已完成 | flatten 可见性 |
| `--data` | 已完成 | sheet sidecar JSON |
| sheet tag `direction` | 已完成 | meta.frameTags |
| `--script` / `--shell` | 后续 | M9 |
| `--trim*` / padding / extrude / crop / merge-duplicates | 未完成 | 见 08；不假装 PASS |
| 顺序敏感官方 CLI 解析器 | 未完成 | 当前为 clap 子命令结构化 |
