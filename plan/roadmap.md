# 路线图（v2：CLI 优先）

> 当前：**只做规划，不写代码。** 下列里程碑在你确认后开始实施。前端和 WASM 都已推迟。
>
> **状态（2026-10-22）：** M0 完成（CLI 核心 + 黄金测试）；M1 完成（坦克演示 + skill 已落地）；**M2 部分**——parts 纸娃娃、cleanEdge 外宏已做，`pxl mcp` 未实现（MCP 客户端已在 harness V3-B 就绪）；M3/M4 未开始。质量改进见 pxl-quality-improvement-plan.md（Q0–Q2 已插入并执行 Q2）。

## M0：CLI 核心（约 1–1.5 周）
- 内容：
  - workspace 和各个 crate 的骨架。
  - pxl-core：调色板、IndexFrame、文本矩阵、PRNG。
  - pxl-format：schema 定义。
  - CLI 子命令：`inspect`、`palette show/apply/swap/cycle`、`op`（shift、row_shift、flip、rotate90、palette_swap、palette_cycle、outline、shade、dissolve、noop）、`render`（PNG/GIF，含缩放和网格）、`sheet`、`validate`（default 规则）、`schema`、`list`。
  - 生成器：`gen shapes`、`gen bollinger`（按 zfedoran 版 MIT 算法 clean-room 重写，输出索引）。
  - `--json` 输出和退出码。
- 验收：
  1. 每个 op 都有单元测试；flip、rotate90、平移与 Piskel `TransformUtils.js` 的语义对照测试通过。
  2. 黄金测试 ≥ 30 条，在 macOS 和 Linux 上 sha256 一致。
  3. 所有错误路径都输出 JSON，并返回规定的退出码。
  4. `pxl schema ops` 输出的 schema 能校验所有示例文件。

## M1：坦克演示（由 miniagent 驱动，约 1 周）
- 内容：`gen noise-tile`；op `texture`；宏 `tread_cycle`、`idle_rumble`；`validate --rules tank`；`build` 配方；`demo/tank/` 下的示例文件和 miniagent skill `pxl.md`。
- 验收：见 miniagent-integration.md 第 4 节（可复现、miniagent 5 次中至少 4 次成功、sheet 内容和 QA、观感、性能）。

## M2：Agent 体验与扩展（约 1–2 周）
- `pxl mcp`（stdio MCP server，配 JSON Schema 和 readOnlyHint），接入 miniagent 的 `~/.miniagent/mcp.json`。
- 部件化（parts、pivot、part_transform），cleanEdge 小角度旋转（可用于 8 方向或炮塔独立旋转）。
- 更多宏：walk_side、hurt_flash、death_dissolve、spin_projectile、muzzle_flash（坦克开火）。
- 可选：如果 miniagent 使用的模型支持看图，加入视觉 QA（`render` 产出图，交给模型评分）。

## M3：tile 与 autotile（约 2 周）
- blob 47 / Wang 16 autotile 展开、无缝检查、等距 tile。验收和 v1 的 M4 相同。

## M4+：面向目标游戏（持续）
- 4/8 方向角色、立绘、弹幕、法线（v1 的 M5 内容），以及按游戏分列的素材需求（见下表）。

## Deferred（推迟）
- Web 编辑器、`pxl-wasm` 绑定、浏览器内实时预览、在线分享。前提条件是 pxl-core 保持 no_std + alloc 友好，不依赖文件系统（IO 只放在 cli 层）。

## 按目标游戏的素材需求（沿用 v1）
| 目标风格 | 关键素材 | 本方案覆盖程度 | 降级手段 |
|---|---|---|---|
| Celeste | 8×8 tile + autotile、小角色、粒子 | 高 | 头发链做成运行时效果 |
| Terraria | 16px tile + autotile、矿石变体、大量物品图标、敌人 | 中高 | 模板库 + 换色 + patch_pixels |
| Undertale | 弹幕、怪物立绘、4 方向小人、头像 | 弹幕高；立绘和头像低到中 | 部件化脸部；外部生成 |
| Stardew | 农场 tile、作物阶段、物品、NPC 4 方向、头像、季节 | 季节高；NPC 和头像低到中 | LPC 风格部件 |
| Dead Cells | 流畅的打击动作、法线、VFX | 动作低；VFX 中 | 部件 + 骨骼模板；外部动画 |
| Octopath HD-2D | billboard 角色 + 法线 + 3D 场景 | 中低 | 法线推导；场景以后再说 |
| **坦克 / 俯视射击（新增）** | 车身、炮塔、4/8 方向、履带、炮弹、爆炸、地面 | **高**（几何结构 + 变换非常契合） | 8 方向用 cleanEdge 或重画 |

## 风险
1. 小模型（GLM flash）拼 JSON 容易出错 → 提供 skill 模板、文件优先、hint 提示，M2 改用 MCP。
2. 纯变换动画观感机械 → 坦克题材天然机械，风险低；角色类题材到 M2 再验证。
3. 确定性在不同平台间漂移 → 黄金测试在 macOS 和 Linux 的 CI 中同时跑。
