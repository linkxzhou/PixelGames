# miniagent 集成与坦克演示

## 1. miniagent 现状（2026-10-21 只读检查）
- `/tmp/miniagent` 是一个 **157 字节的 bash 包装脚本**：设置环境变量 `MINIAGENT_MODEL_NAME`（默认 `glm-5.3-flash-external`），然后执行 `python3 /Volumes/my/github/miniagent/python/miniagent.py "$@"`。
- 源码位于 `/Volumes/my/github/miniagent`：
  - Python 包 `python/miniagent/`（入口是 `miniagent.cli:main`，README 称零第三方依赖）。
  - 一个等价的 **Rust 实现**在 `rust/src/`（tools 目录下有 `shell.rs`、`fs.rs`、`todo.rs`、`web.rs`，另有 `mcp/`、`skill.rs`）。
  - 示例 skill 在 `skills/emailer.md`。
- **工具机制**：
  - 内置 `bash`、`read_file`/`list_dir`/`grep`/`glob` 等文件工具、`todo_write`、`web_fetch`。
  - MCP 通过 `~/.miniagent/mcp.json`（`mcpServers`，stdio + Content-Length framing）挂载，工具名为 `mcp_<server>_<tool>`，遵守 `readOnlyHint`。
  - Skill 是 Markdown + frontmatter（也支持 JSON），分三级：workspace `.miniagent/skills/`、user `$MINIAGENT_HOME/skills/`、builtin。激活后会覆盖 prompt、过滤可用工具、覆盖 temperature 和 max_steps。
- **CLI 参数**：
  - `-p` 一次性执行。
  - `--json` 让 stdout 只输出一行 `{"session_id","response"}`。
  - `--cwd` 设置工作区根目录。
  - `--approval ask|auto|readonly`。
  - `--skill NAME`、`--skills-dir`、`--max-steps`、`--goal` 等。
- **我没有读的部分**：工具函数的 schema 细节、bash 工具的超时和输出截断策略（有 `--bash-timeout` 参数）、模型是否支持图像输入。这些都【未验证】。

## 2. 集成方案
**方案 A（M1 采用）：bash + skill**
- 把 `pxl` 安装到 PATH（用 `cargo install --path crates/pxl-cli`）。
- 在演示工作区写一个 skill：`.miniagent/skills/pxl.md`，用 frontmatter 声明 `name: pxl`、`description`、`temperature: 0.2`。正文写明：
  1. 永远加 `--json` 参数。
  2. 先 `pxl list ...` 或 `pxl schema ...`，再写 JSON 文件。
  3. 每一步检查退出码，非 0 时读 `error.hint` 修正后重试，最多 3 次。
  4. 最后必须执行 `pxl validate`，退出码为 0 才算完成。
  5. 禁止手写 PNG，禁止用 python 处理图像。
- 运行方式：`/tmp/miniagent --cwd ~/PixelGames/demo/tank --approval auto --skill pxl --json -p "生成一个坦克移动演示素材……"`

**方案 B（M2）：`pxl mcp`**
- `pxl` 自带 stdio MCP server，暴露 `pxl_gen`、`pxl_op`、`pxl_anim`、`pxl_sheet`、`pxl_render`、`pxl_validate`、`pxl_inspect` 等工具，带 JSON Schema（由 schemars 生成）。`inspect`、`validate`、`render` 标注 `readOnlyHint: true`。
- 配置写在 `~/.miniagent/mcp.json`：`{"mcpServers":{"pxl":{"command":"pxl","args":["mcp"]}}}`。
- 好处：参数有 schema 约束，不用拼 shell 引号，对小模型（GLM flash）更稳。

## 3. 坦克演示：示例文件与调用序列

### 3.1 调色板 `tank.palette.json`
```json
{ "v":1, "colors":["#00000000","#14100c","#3b4a2a","#5a6e3a","#86a052","#b9c98a","#4a4038","#6e6258","#9a8c7a","#c9b994","#e0533d","#f2f2e6"],
  "ramps":{ "olive":[2,3,4,5], "tread":[1,6,7], "ground":[6,7,8,9] },
  "cycles":{ "tread":{ "indices":[6,7,1], "period":3 } },
  "special":{ "transparent":0, "outline":1, "flash":11 } }
```

### 3.2 车身形状 `tank_body.shapes.json`（朝上，即北，16×16）
```json
{ "w":16, "h":16, "palette":"tank.palette.json", "mirror":"x",
  "draw":[
    {"rect":[2,2,3,12],"c":6},
    {"rect":[3,2,1,12],"c":7,"pattern":"stripe_y:2"},
    {"rect":[5,4,3,8],"c":3},
    {"rect":[6,6,2,4],"c":4},
    {"circle":[8,8,3],"c":4,"mirror":false},
    {"rect":[7,0,2,6],"c":5,"mirror":false},
    {"outline":{"c":1,"mode":"outer"}} ] }
```

### 3.3 调用序列（agent 执行的命令）
```bash
pxl palette show tank.palette.json --json
pxl gen shapes --spec tank_body.shapes.json --seed 1 -o tank.pxl.json --json
# 可选：用 bollinger 给车身加一点随机纹理（只作用于车身区域的局部蒙版）
pxl op tank.pxl.json --op '{"op":"texture","gen":"bollinger","template":"panel_4x8","region":[5,4,3,8],"ramp":"olive","seed":42}' -o tank.pxl.json --overwrite --json
# 4 个方向：N 为原图，E = rotate90 cw，S = flip y，W = flip x(E)
pxl op tank.pxl.json --frame base --as N --op '{"op":"noop"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame N --as E --op '{"op":"rotate90","dir":"cw"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame N --as S --op '{"op":"flip","axis":"y"}' -o tank.pxl.json --overwrite --json
pxl op tank.pxl.json --frame E --as W --op '{"op":"flip","axis":"x"}' -o tank.pxl.json --overwrite --json
# 履带动画：每个方向 4 帧；履带区域按行错位，同时做 palette cycle；车身每 2 帧下沉 1px（震动）
pxl anim tank.pxl.json --name move --dirs N,E,S,W --macro 'tread_cycle(steps=4, region=auto_tread, cycle=tread, bob=1)' -o tank.pxl.json --overwrite --json
pxl anim tank.pxl.json --name idle --dirs N,E,S,W --macro 'idle_rumble(steps=2, amp=1)' -o tank.pxl.json --overwrite --json
# 地面 tile
pxl tile gen --kind dirt --size 16 --seed 7 --palette tank.palette.json --variants 4 -o ground.pxl.json --json
# 打包、预览、校验
pxl sheet tank.pxl.json ground.pxl.json --layout rows-by-anim --pad 1 -o out/tank.sheet.png --json
pxl render tank.pxl.json --anim move --dir E --scale 6 -o out/tank_move_E.gif --json
pxl render out/tank.sheet.png --scale 4 --grid -o out/preview.png --json
pxl validate out/tank.sheet.json --rules tank --json
```
> 以上全部步骤也可以写成一个 `tank.recipe.json`，用 `pxl build` 一条命令跑完。agent 首选逐步调用，便于纠错；最终把配方写入仓库，方便复现。

### 3.4 `tread_cycle` 宏展开（示意）
```json
[ [{"op":"row_shift","region":"tread_l","axis":"along_heading","dx":0},{"op":"palette_cycle","cycle":"tread","step":0}],
  [{"op":"row_shift","region":"tread_l","axis":"along_heading","dx":1},{"op":"palette_cycle","cycle":"tread","step":1},{"op":"shift","region":"hull","dy":1}],
  [{"op":"row_shift","region":"tread_l","axis":"along_heading","dx":2},{"op":"palette_cycle","cycle":"tread","step":2}],
  [{"op":"row_shift","region":"tread_l","axis":"along_heading","dx":3},{"op":"palette_cycle","cycle":"tread","step":0},{"op":"shift","region":"hull","dy":1}] ]
```
- `auto_tread`：取靠近左右边缘、颜色属于 `ramps.tread` 的列区域，并按方向旋转。做法是 N 方向先定义好 region，**再跟着方向变换一起旋转**，这样不用分别为每个方向推断。
- `along_heading`：N/S 时沿 y 方向错位，E/W 时沿 x 方向错位；错位以 wrap 方式在 region 内循环。

### 3.5 输出 `out/tank.sheet.json`（片段）
```json
{ "frames":{ "tank_move_N_0":{"frame":{"x":0,"y":0,"w":16,"h":16},"duration":100}, "...":{} },
  "meta":{ "image":"tank.sheet.png","size":{"w":290,"h":120},"format":"indexed8",
    "frameTags":[{"name":"move_N","from":0,"to":3,"direction":"forward"},{"name":"move_E","from":4,"to":7,"direction":"forward"}],
    "pxl":{ "version":"0.1.0","palette":"tank.palette.json","pivot":[8,8],"dirs":["N","E","S","W"],"fps":{"move":10,"idle":4},"tiles":{"ground":{"from":32,"to":35}} } } }
```

## 4. 坦克演示验收标准
1. **一条命令可复现**：在干净目录中运行 `pxl build tank.recipe.json -o out/` 两次，产物的 sha256 一致；Linux 和 macOS 上一致。
2. **miniagent 能完成任务**：`miniagent --approval auto --skill pxl -p "<任务描述>"` 在 ≤ 40 步内产出 `out/tank.sheet.png`、`out/tank.sheet.json`，且 `pxl validate` 返回 0。连续跑 5 次，至少 4 次成功【目标值】。
3. **sheet 内容**：4 方向 × move（≥ 4 帧）+ 4 方向 × idle（≥ 2 帧）+ ≥ 4 个地面 tile 变体；JSON 能被 Aseprite 格式解析器读取（用一个小的 Python 或 JS 脚本检查 frames 和 frameTags）。
4. **程序 QA（`--rules tank`）**：
   - 所有像素都在调色板内，没有半透明。
   - 4 个方向的包围盒尺寸两两互换一致（N/S 相同，E/W 为 N 的转置）。
   - move 动画相邻帧的像素差在 2%–25%；首尾帧可以无缝循环。
   - 履带区域在每一帧都有变化，车身主体（hull）的像素数波动 ≤ 5%。
   - 地面 tile 四边可以无缝平铺（左边与右边、上边与下边的像素一致或在 ramp 内相邻）。
5. **人工观感**：在 6× GIF 上判断，能认出是坦克，炮管朝向与方向一致，履带看起来在转动。
6. **性能**：`pxl build` 跑完整个坦克配方 < 200ms（release 构建）【估】。

## 5. 风险
- GLM flash 这类小模型在拼 JSON 和 shell 引号时容易出错：缓解办法是 skill 里给出模板、优先用文件而不是行内 JSON、`--json` 错误信息带上 `hint`，M2 改用 MCP。
- rotate90 对非方形精灵：规定坦克画布为方形（16×16 或 24×24），炮管居中。
- 斜向（8 方向）只靠 rotate90 和 flip 得不到：M1 只做 4 方向，8 方向放到以后（用 shapes 重画斜向，或用 cleanEdge 做 45° 旋转）。
