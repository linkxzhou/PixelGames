# CLI flag 覆盖矩阵（v8.0 / M0–M9）

> 日期：2026-11-03（Asia/Shanghai）  
> 对照：https://www.aseprite.org/docs/cli/ + 本地 `thirdparty/aseprite/src/app/cli/app_options.h`  
> v8.1 **GC-1**：补全对照 `app_options.h` 曾缺行的旗标（状态与 `libs/pxl-cli/src/cmd/ase/` 核对）

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


## v8.1 GC-1 · `app_options.h` 曾缺行旗标

> 证据：`thirdparty/aseprite/src/app/cli/app_options.h` / `app_options.cpp`；实现核验：`libs/pxl-cli/src/cmd/ase/{args,convert,ops,mod}.rs`。  
> 状态取值：已实现 / 部分 / 缺失 / 不做。扩展别名须写明映射。

| Flag | 状态 | 证据 / 映射 | 说明 |
|---|---|---|---|
| `--split-layers` | 已实现 | `args.rs` + `convert::convert_split`；`{layer}` 插入 `--save-as` | 按 image 层各写一文件；GC-2 |
| `--split-tags` | 已实现 | `convert::convert_split`；`{tag}` 插入路径 | 与 `--split-layers` 互斥；GC-2 |
| `--split-grid` | 缺失 | `splitGrid()`；CLI 无同名 arg | 按 grid 拆；GC-2 / 07 |
| `--split-slices` | 已实现 | `args.rs` `split_slices`；`ase split-slices` / `convert --split-slices` | 矩阵上表已有；此处对照确认 |
| `--slice <name>` | 已实现 | `convert` 按 slice 首 key 裁切文档 | ≠ `split-slices`；GC-2 |
| `--ignore-layer` | 已实现 | convert/sheet 可重复；`gc_flags::apply_ignore_layers` | 与 `--layer` 可组合；GC-2 |
| `--shrink-to` | 缺失 | `shrinkTo()`；08 §6 标「做」；CLI 无 arg | GB-5 / GC-2 外的缩放适配 |
| `--format`（`json-hash` / `json-array`） | 已实现 | `ase sheet --format`；默认 `json-hash`；`sheet_json_format` | GC-3；hash 消费者默认路径不变 |
| `--data` | 已实现 | `AseSheetArgs.data` → sheet sidecar | 上表已有；与 `--format` 分工：我方用 `--data` 路径，未暴露 format 枚举 |
| `--tagname-format` | 缺失 | `tagnameFormat()`；仅有 `--filename-format` 子集 | GC-4 |
| `--filename-format` | 部分 | 上表已列占位符子集 | 冷门占位符不做 |
| `--power-of-two-size` | 缺失 | `powerOfTwoSize()`；CLI 无 | GC-4；依赖 pack 能力 |
| `--trim-sprite` | 已实现 | convert/sheet 同名 flag → `ops::trim_sprite`；与 `--trim` 同文档裁语义 | sheet 另有 per-cell `--trim`；GC-2 |
| `--trim` | 已实现 | convert=文档裁；sheet=cell bbox | 上表已有 |
| `--trim-by-grid` | 已实现 | convert/sheet | 上表已有 |
| `--border-padding` | 已实现 | `SheetPadding.border` → `pack_sheet_padded` 图集外缘 | 与 shape/inner 分立；GC-2/本轮 |
| `--shape-padding` | 已实现 | `SheetPadding.shape`（缺省回落 `--padding`） | 单元格间距；本轮 |
| `--inner-padding` | 已实现 | `SheetPadding.inner` 内容内缩、placement 含内边 | 本轮 |
| `--padding` | 已实现 | 扩展别名（非官方同名）；sheet | 映射官方三类 padding 的均匀近似 |
| `--sheet-type` | 部分 | 官方 `sheetType()`；我方扩展 **`--layout`**=`horizontal\|vertical\|packed\|rows\|grid` | `packed`≈`--sheet-pack`；非官方全集 type 名；GC-4 |
| `--sheet-pack` | 部分 | 官方；我方 `--layout packed` | 无独立 `--sheet-pack` 开关 |
| `--sheet-width` / `--sheet-height` | 缺失 | `sheetWidth/Height()`；CLI 无 | GC-4 |
| `--sheet-columns` / `--sheet-rows` | 缺失 | `sheetColumns/Rows()`；`--layout rows\|grid` 无固定行列数 | GC-4 |
| `--layout` | 已实现 | 扩展别名 → `pxl-export` layouts | 对应 `--sheet-type` 子集 |
| `--list-layers` | 已实现 | `ase list-layers` | 上表已有 |
| `--list-layer-hierarchy` | 缺失 | `listLayerHierarchy()`；仅扁平 `list-layers` | GC-2/树形输出未做 |
| `--export-tileset` | 缺失 | `exportTileset()`；tileset IO 仍跳过（GE-5） | 矩阵如实：缺失；实现依赖 tilemap P2 |
| `--layer` / `--all-layers` | 已实现 | convert/sheet 可见性 | 上表已有；非 `--ignore-layer` |

### 扩展别名速查

| 官方 | 我方 | 关系 |
|---|---|---|
| `--sheet-type` | `--layout` | 子集别名；packed/rows/grid/horizontal/vertical |
| `--sheet-pack` | `--layout packed` | 等价意图 |
| `--border-padding` / `--shape-padding` / `--inner-padding` | `--padding` | 单一均匀 pad；未拆三路 |
| `--format json-hash` | `--data <path>` | 侧车路径；未暴露 format 枚举 |
| `--trim-sprite` | `convert --trim` / `ase op trim` | 语义近；旗标名不同 |
