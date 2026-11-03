# pxl-ase scripting (M9)

> Date: 2026-11-02 (Asia/Shanghai)

## Engine

- **Rhai** (Rust-native). Lua is not in the workspace; Rhai is the chosen host.
- CLI: `step-300 ase script <path.rhai> [--script-param key=value ...] [--workdir DIR] [--allow-fs]`
- Library: `pxl_ase::script::run_script_file` / `run_script_src`.

## API surface (prototype)

| Function | Meaning |
|---|---|
| `open(path)` | Load `.aseprite` under workdir |
| `save(path)` | Write `.aseprite` or `.png` (frame 0) |
| `replace_color(from_hex, to_hex)` | Exact RGBA replace (flattens to RGBA if needed) |
| `set_pixel(frame, x, y, hex)` | Set one RGBA pixel |
| `flatten(frame)` | Returns pixel count after flatten |
| `width()` / `height()` | Canvas size |
| `params.key` | From `--script-param key=value` |

## Sandbox policy

1. **FS**: paths must canonicalize under `--workdir` (default: process cwd). Escape → error. `--allow-fs` opts out (dangerous).
2. **Network**: none registered.
3. **Steps**: `Engine::set_max_operations` (default 200_000).
4. **Canvas**: reject open if width or height > `MAX_CANVAS` (4096).
5. **Audit**: CLI prints `script ok` / JSON `{ok:true}`; failures go to stderr via normal CLI error path.

## Non-goals

- Aseprite Lua API parity
- `--shell` REPL (deferred; matrix: 不做/后续)
- MCP (still unimplemented; do not claim done)

## Example

```rhai
// replace_red.rhai
open(params["in"]);
replace_color("#ff0000", params["to"]);
save(params["out"]);
```

```bash
step-300 ase script replace_red.rhai \
  --workdir work/v8-ase-pilot \
  --script-param in=mini.aseprite \
  --script-param out=out/scripted.aseprite \
  --script-param to=#00ff00
```
