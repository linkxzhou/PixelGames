# CLI flag 覆盖矩阵（v8.0 / M0–M7）

> 日期：2026-11-02（Asia/Shanghai）  
> 对照：https://www.aseprite.org/docs/cli/

| Flag / 子命令 | 状态 | 说明 |
|---|---|---|
| `ase --help` | 已完成 | clap 子命令组 |
| `ase inspect <file>` | 已完成 | 打印尺寸/色模/层/帧/tags |
| `ase convert` / `--save-as` | 已完成 | 导出 flatten PNG（子集） |
| `ase list-layers` | 已完成 | |
| `ase list-tags` | 已完成 | |
| `ase --sheet` / `ase sheet` | 已完成 | 扁平帧拼 sheet PNG+简易 JSON |
| `-b` / `--batch` | 兼容忽略 | 默认无 UI |
| `--preview` | 部分 | convert 支持 `--preview` 干跑 |
| `--tag` | 部分 | convert/sheet 可按 tag 过滤帧 |
| `--oneframe` | 部分 | inspect/convert 支持 |
| `--color-mode` | 部分 | convert 支持 rgba/indexed/grayscale |
| `--script` / `--shell` | 后续 | M9 |
| 其余公开 flags | 未完成 | 见 08 分册；不假装 PASS |
