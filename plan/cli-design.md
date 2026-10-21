# `pxl` CLI 设计

## 1. 原则
1. **确定性**：相同的输入文件、`--seed` 和 `pxl` 版本，输出的字节完全一致（PNG 编码参数固定，JSON 键有序）。
2. **对 agent 友好**：
   - 每个子命令都支持 `--json`：stdout **只输出一行 JSON**，日志写到 stderr。
   - 出错时也输出 JSON：`{"ok":false,"error":{"code":"E_SCHEMA","message":"...","path":"/frames/2/0/dx","hint":"dx must be in -8..8"}}`。
   - 不交互、不进入分页器、不着色（`--json` 隐含 `--no-color`）。
3. **文件优先，也支持管道**：输入可以是路径，也可以是 `-`（stdin）；输出用 `-o PATH`，`-o -` 表示 stdout。
4. **纯文本可读**：中间产物用 `.pxl.json`（索引矩阵加元数据），agent 可以直接读或用 `pxl inspect` 读。
5. **小而正交**：一个子命令只做一件事，组合交给 agent 或 recipe。

## 2. 文件格式
| 扩展名 | 内容 |
|---|---|
| `*.palette.json` | 调色板：`colors`（hex，索引 0 为透明）、`ramps`、`cycles`（用于 palette cycling）、`special` |
| `*.template.json` | 蒙版模板（Bollinger 取值 0/1/2 加上扩展取值 3/4/5，`mirror`、`legend`） |
| `*.pxl.json` | **精灵文档**：`{ "v":1, "palette":"tank.palette.json", "w":16, "h":16, "frames":{"base":["....1111....", ...]}, "anims":{...}, "meta":{"seed":42,"recipe":"tank.recipe.json","pxl_version":"0.1.0"} }` |
| `*.ops.json` | op 列表（见 asset-pipeline.md 第 2 节），例如 `[{"op":"rotate90","dir":"cw"}]` |
| `*.recipe.json` | 完整配方：从 base（生成）到 post（后处理），再到 directions、anims、sheet，一次性描述整个素材 |
| `*.sheet.png` + `*.sheet.json` | 输出的 spritesheet，JSON 兼容 Aseprite 的 `frames` 和 `meta.frameTags` 格式，附加 `pxl` 扩展字段（方向、pivot、fps） |

## 3. 子命令
```
pxl gen       <generator> [opts] -o out.pxl.json      # 生成静态帧
pxl op        <in.pxl.json> --ops ops.json|--op '<json>' [--frame base] [--as newname] -o out.pxl.json
pxl anim      <in.pxl.json> --name idle --macro 'tread_cycle(steps=4)' | --frames frames.json -o out.pxl.json
pxl palette   list|show|apply|swap|cycle  ...         # 调色板管理与映射
pxl tile      gen|autotile ...                        # 地面 tile 与 autotile（M1 只做 gen 和简单无缝检查）
pxl sheet     <a.pxl.json>... [--layout grid|rows-by-anim] [--pad 1] -o out.sheet.png  # 同时写出 out.sheet.json
pxl render    <in.pxl.json> [--frame f | --anim a] [--scale 4] [--grid] -o out.png|out.gif   # 预览
pxl inspect   <in.pxl.json> [--frame f]               # 打印尺寸、色数、包围盒、ASCII 矩阵
pxl validate  <in.pxl.json|out.sheet.json> [--rules default|tank]   # 程序 QA，非零退出表示未通过
pxl build     <recipe.json> -o outdir/                # 一次跑完整个配方（gen → op → anim → sheet → validate）
pxl schema    <recipe|ops|pxl|palette>                # 输出 JSON Schema，供 agent 参考
pxl list      generators|ops|macros|templates|palettes
pxl mcp                                              # （M2）stdio MCP server，复用以上能力
```

### 3.1 生成器（`pxl gen`）
- `bollinger --template tank_body_8x16 --seed 42 --palette tank.palette.json --ramp olive [--mirror x]`
- `shapes --spec shapes.json`：形状程序（rect、circle、line、mirror、outline），用于需要明确结构的物体，比如坦克的车身、炮塔、炮管。
- `noise-tile --kind grass|dirt|sand --size 16 --seed 7 --palette ...`
- `parts --spec parts.json`：部件合成（M2）。

> 坦克这种"有明确结构的机械"建议用 **shapes 画骨架 + bollinger 加细节纹理（作为部件局部填充）**。纯 Bollinger 会生成不对称、像昆虫一样的轮廓，不适合坦克。

### 3.2 全局参数
`--json`、`--seed N`（覆盖输入里的 seed）、`--quiet`、`--strict`（警告也视为错误）、`--pxl-version-lock 0.1`、`--overwrite`（默认不覆盖已存在的文件，返回 `E_EXISTS`）。

### 3.3 `--json` 成功输出（示例）
```json
{"ok":true,"cmd":"op","out":"tank_n.pxl.json","w":16,"h":16,"frames":["base"],"colors_used":6,"warnings":[],"hash":"b3:9f1c…"}
```
`render` 和 `sheet` 还会额外返回 `png`、`png_w`、`png_h`、`frames_count`。`validate` 返回 `checks:[{id,pass,value,limit}]`。

### 3.4 退出码
| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 一般错误（IO 等） |
| 2 | 参数或用法错误（`E_USAGE`） |
| 3 | 输入不符合 schema（`E_SCHEMA`，带 JSON pointer） |
| 4 | 语义错误（越界、未知帧或调色板等，`E_SEMANTIC`） |
| 5 | `validate` 未通过（`E_QA`） |
| 6 | 输出文件已存在（`E_EXISTS`） |

### 3.5 确定性实现约定
- PRNG：SplitMix64 派生种子 + PCG32；每个 op 的子种子为 `hash(seed, op_index, frame_name)`。
- 不使用浮点超越函数；需要角度时只允许 `rotate90`，或者通过整数查表实现 `rotate` 的 cleanEdge 版本（M2）。
- PNG：用 `png` crate，固定压缩级别和过滤器；有可能的话输出 indexed PNG（PLTE + tRNS）。
- JSON：用 serde 输出，键顺序固定，不写入时间戳。
