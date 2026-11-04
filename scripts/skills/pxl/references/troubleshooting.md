# Troubleshooting

## Exit codes

step-300 only:
- 0 ok, 1 I/O error, 2 usage / CLI args, 3 schema (invalid JSON structure), 4 semantic (palette mismatch, op failure, macro signature), 5 QA (validate), 6 output already exists

Pipeline (steps 100, 200, 400, 500):
- 10 waiting approval (stop; user must `approve --digest <hex>`)
- 11 blocked or policy (unsupported, needs input, policy violation)
- 12 gates failed (checks did not pass)
- 13 budget exhausted (model turn limit)
- 14 lock held (another process)
- 15 integrity (digest mismatch, corrupt log)
- 16 tool or provider error (step-300 unavailable, API error, timeout)

## Missing platform binary

If the launcher reports "no step-N binary for platform X":

1. Check `bin/manifest.json` to see which platforms are bundled.
2. Build the missing platform with:
   ```bash
   # from repo root
   scripts/build-skill-bins.sh --target darwin-x64   # or linux-x64, windows-x64, etc.
   # or build all (needs cargo-zigbuild for linux, cargo-xwin for windows)
   scripts/build-skill-bins.sh --target all
   ```
3. Cross-compilation tools:
   - **Linux**: `cargo install cargo-zigbuild`, requires `zig` on `PATH`
   - **Windows**: `cargo install cargo-xwin`
   - macOS x64 from macOS arm64: no extra tool, `x86_64-apple-darwin` target only

If building from another machine, copy `scripts/skills/pxl/bin/<platform>/` into the skill.

## Audio rendering needs Node 22+

`step-300 audio validate` and `audio inspect` work without Node. `audio render` and `step-300 plan` with an audio spec need Node 22+ and the bundled `assets/audio-toolchain/`; set `PXL_AUDIO_NODE` or `PXL_AUDIO_ADAPTER` only for explicit overrides. No npm install, network access or global package is used. Missing Node/adapter blocks only audio rendering; it never switches synthesizer versions silently.

## Windows: curl not found

Step-200 / provider code calls `curl` for HTTPS. Windows 10+ includes `curl.exe` by default. If unavailable, install from https://curl.se or use WSL.

## Provider / .env

`--provider fake` is offline; uses `STEP_100_FAKE_TURNS` / `STEP_200_FAKE_TURNS` env vars pointing to a JSON transcript.

`--provider openai` needs `--env <file>` with `OPENAI_API_KEY=sk-...`. The skill never reads env vars or stores credentials.

## Startup verification

```bash
scripts/run.sh --platform           # prints detected platform
scripts/run.sh --verify             # checks manifest sha256
scripts/run.sh step-300 --version   # smoke test
```

## Dev mode (repo-local builds)

```bash
scripts/run.sh --dev step-300 ...   # uses target/release/step-300 instead of bundled binary
```

Requires `cargo build --release -p pxl-cli` (and other step crates) first.
