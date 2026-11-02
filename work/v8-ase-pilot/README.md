# v8-ase-pilot

> 日期：2026-11-02（Asia/Shanghai）  
> 自造夹具（非 Aseprite 专有测试资源），由 `pxl-ase-io` example `write_fixture` 生成。

## 命令

```bash
cargo run -p pxl-cli --bin step-300 -- ase inspect work/v8-ase-pilot/mini_indexed.aseprite
cargo run -p pxl-cli --bin step-300 -- --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/frame0.png
cargo run -p pxl-cli --bin step-300 -- --overwrite ase sheet work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/sheet.png
```

不修改 `games/`。
