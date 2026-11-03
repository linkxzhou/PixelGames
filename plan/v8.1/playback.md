# Playback / tag export semantics（GB-4）

> 日期：2026-11-03（Asia/Shanghai）  
> 对照：`ase-file-specs.md` Tags chunk（direction / repeat）与 `doc/playback.h` / `frames_sequence.h` 概念；实现：`libs/pxl-ase/src/frames.rs`。

## 导出（batch / GIF / sheet）算法

UI「无限循环」不是导出倍增器。导出帧列表由 `expand_tag_frames` 决定：

### Direction

| TagDirection | 基序列 |
|---|---|
| Forward | `from..=to` |
| Reverse | `from..=to` 再反转 |
| PingPong | `from..=to`，若长度>2 再追加内部帧的反向（不含两端，避免双写端点）→ 一次「来回」 |
| PingPongReverse | 先 Reverse，再同上内部反向 |

### Repeat（导出）

规格：`0` = UI 无限 / **导出一次**；`1` = 播一次；`n` = 播 n 次。

对 **PingPong / PingPongReverse**：

| repeat | 导出 |
|---|---|
| `0` | 完整来回一次（与上表 PingPong 基序列相同） |
| `1` | **仅单程**（Forward 或 Reverse，不做回程）——对齐规格「plays just in one direction」 |
| `n ≥ 2` | 完整来回序列再重复 `n` 次（每次含来回） |

对 Forward/Reverse：`repeat≤1` → 基序列一次；`repeat>1` → 基序列重复 `repeat` 次。

### `--play-subtags`

模型无 parent 指针。子标签 = 范围严格落在所选 tag 内的其它 tag；按 `from` 排序展开子 tag（各自 direction/repeat），间隙用父范围剩余帧 **正向** 填充。  
**诚实限制**：重叠兄弟启发式；父 direction 不重套到拼接结果上；无真正层级元数据。

## GIF

`export_gif` 对 `expand_tag_frames` / `select_frames_opts` 给出的帧列表逐帧取 `frame_durations_ms` 写 delay；不在编码器侧再解释 repeat。
