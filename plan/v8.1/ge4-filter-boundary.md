# GE-4 滤镜目标通道与平铺边界策略

> 日期：2026-11-03（Asia/Shanghai）  
> 状态：文档化（实现仍为整幅 RGBA；无 per-channel mask CLI）

## Target channel（对照 `filters/target.h`）

- 官方可按 R/G/B/A 掩码选择写入通道。
- **本实现**：convolution / median / color-curve **只改 RGB，保留源 alpha**；indexed/gray 缓冲跳过。
- 未暴露 `--target-channel`；若需要可在后续以位掩码扩展 `Kernel`/`median` 写回。

## Tiled mode（对照 `tiled_mode.h`）

- 官方：滤镜采样可在文档边界外按 X/Y/both 平铺回卷。
- **本实现**：边界策略为 **clamp**（重复边缘像素），不是 wrap。
- 原因：当前无 document tiled flags 写回路；clamp 对单帧 sprite 导出最安全。

## 验收口径

- 奇异 convolution 核（偶数边 / divisor=0）拒绝。
- median 半径 `> MAX_MEDIAN_RADIUS(3)` 拒绝。
