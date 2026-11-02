# CLI flag 覆盖矩阵（v8.0 / M0–M9）

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
| `ase sheet` | 已完成 | PNG+JSON；`--layout` horizontal/vertical/packed；`--padding`/`--extrude`/`--trim`/`--merge-duplicates` |
| `ase filter` | 已完成 | outline / replace-index / replace-color / invert / brightness-contrast / hue-saturation |
| `ase op` | 已完成 | flip / rotate90 / flood-fill / crop / canvas-size / trim |
| `ase import` | 已完成 | ase-import/v1 manifest + schema-valid Recipe + palette/frames/sheet |
| `-b` / `--batch` | 兼容忽略 | 默认无 UI |
| `--preview` / `-p` | 已完成 | convert 干跑 |
| `--tag` | 已完成 | convert/sheet 过滤 |
| `--frame-range` | 已完成 | `0,2` / `1-3` |
| `--oneframe` | 已完成 | inspect/convert/list/sheet/import |
| `--color-mode` | 已完成 | rgba/indexed/grayscale |
| `--palette` | 已完成 | `.gpl` / `.act` |
| `--dithering-algorithm` | 已完成 | none/ordered/old |
| `--dithering-matrix` | 已完成 | bayer2x2/4x4/8x8 |
| `--scale` | 已完成 | convert/split-slices |
| `--filename-format` | 部分 | `{frame}`/`{frame1}`/`{frame01}`/`{frame001}`/`{frame0001}` `{tag}` `{innertag}` `{layer}` `{title}` `{name}` `{path}` `{fullname}` `{slice}` `{duration}` `{tagframe}` `{innertagframe}` `{startframe}` `{endframe}`；非官方全集的冷门占位符不做 |
| `--ignore-empty` | 已完成 | convert/sheet |
| `--merge-duplicates` | 已完成 | convert/sheet；连续相同扁平帧去重 |
| `--all-layers` / `--layer` | 已完成 | flatten 可见性 |
| `--data` | 已完成 | sheet sidecar JSON |
| `--padding` | 已完成 | sheet；映射 `pack_sheet` pad |
| `--extrude` | 已完成 | sheet；边缘像素外扩 |
| `--trim` | 已完成 | convert=文档级透明边裁；sheet=每 cell bbox 裁 |
| `--trim-by-grid` | 已完成 | convert/sheet：内容 bbox 后向外对齐 `sprite.grid`；`grid.w/h==0` 时退化为普通 trim |
| `--crop` | 已完成 | convert：`x,y,w,h`；亦见 `ase op crop` |
| sheet tag `direction` | 已完成 | meta.frameTags |
| `--play-subtags` | 部分 | Tag 模型无 parent；按范围包含推断子标签并展开 direction；重叠兄弟启发式；见 08 |
| `--script` / `ase script` | 部分 | Rhai：`ase script <path> --script-param k=v`；`--shell` REPL **不做/后续**；MCP 未做 |
| 顺序敏感官方 CLI 解析器 / `ase run` | 部分 | 最小 `AseBatchParser`：open / `--save-as` / `--sheet` / `--tag` / `--trim` / `--trim-by-grid` / `--play-subtags` / `--filter replace-color|invert` / `--op trim|flip`；**非**官方全集 |
