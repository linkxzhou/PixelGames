# GE-4 滤镜目标通道与平铺边界策略

> 日期：2026-11-03（Asia/Shanghai）  
> 状态：**已实现**（`FilterOpts` + CLI `--channels` / `--tiled`）

## Target channel

- `ChannelMask`：`r/g/b/a` 布尔；默认 **RGB**（保留源 alpha）。
- CLI：`ase filter … --channels rgb|rgba|r|g|b|a…`
- 作用于 `convolve` / `median` / `curve`：仅写回被选中的通道。

## Tiled / boundary

- `BoundaryMode`：`Clamp`（默认）| `WrapX` | `WrapY` | `Wrap`（both）。
- CLI：`ase filter … --tiled clamp|x|y|both`
- 采样在核/邻域越界时按模式 clamp 或环绕；不再仅文档声明。

## 验收

- 奇异 convolution 核拒绝；median 半径 `>3` 拒绝。
- `--channels r` 时 G/B/A 不变；`--tiled both` 时角点采样环绕。
