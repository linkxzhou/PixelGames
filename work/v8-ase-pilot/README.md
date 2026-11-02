# v8-ase-pilot

> 日期：2026-11-02（Asia/Shanghai）  
> 自造夹具（非 Aseprite 专有测试资源），由 `pxl-ase-io` example `write_fixture` 生成。

## 命令

```bash
cargo build -p pxl-cli --bin step-300
BIN=target/debug/step-300

$BIN ase inspect work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/anim.gif --frame-range 0-2
$BIN --overwrite ase convert work/v8-ase-pilot/mini_indexed.aseprite --save-as work/v8-ase-pilot/out/dither.png \
  --color-mode indexed --dithering-algorithm ordered --dithering-matrix bayer8x8 \
  --palette work/v8-ase-pilot/pilot.gpl
$BIN --overwrite ase list-slices work/v8-ase-pilot/mini_indexed.aseprite
$BIN --overwrite ase split-slices work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/slices
$BIN --overwrite ase sheet work/v8-ase-pilot/mini_indexed.aseprite --out work/v8-ase-pilot/out/sheet.png --layout horizontal
$BIN --overwrite ase filter work/v8-ase-pilot/mini_indexed.aseprite invert --out work/v8-ase-pilot/out/invert.png
$BIN --overwrite ase op work/v8-ase-pilot/mini_indexed.aseprite flip --axis h --out work/v8-ase-pilot/out/flipped.aseprite
```

产物见 `work/v8-ase-pilot/out/`。不修改 `games/`。
