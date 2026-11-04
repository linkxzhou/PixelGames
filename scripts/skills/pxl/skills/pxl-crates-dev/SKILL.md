---
name: pxl-crates-dev
description: Reference docs for the eight pxl-* Rust crates (core, gen, ops, qa, format, export, engine, wasm). Use only when reading or modifying code in the pxl-* crates themselves, not when running the CLI.
---

# pxl-crates-dev

Development reference for the `pxl-*` crates in `steps/step-300-asset-build/`. Read only when modifying crate internals, not for general CLI usage. The crates are no_std+alloc where possible.

| Reference | Crate | Read when |
| --- | --- | --- |
| [references/core.md](references/core.md) | `pxl-core` | `IndexFrame`, `Palette`, text matrix, PRNG, `PxlError` |
| [references/gen.md](references/gen.md) | `pxl-gen` | Bollinger, shapes, noise tiles, built-in palettes / templates |
| [references/ops.md](references/ops.md) | `pxl-ops` | Transform ops, animation macros, paper-doll compose |
| [references/qa.md](references/qa.md) | `pxl-qa` | `validate`, `QaConfig`, checks, adding rules |
| [references/format.md](references/format.md) | `pxl-format` | JSON docs, recipes, ASCII specs, schemas |
| [references/export.md](references/export.md) | `pxl-export` | PNG / GIF encoding, sheet packing |
| [references/engine.md](references/engine.md) | `pxl-engine` | IO-free `render`/`inspect`, `Resolver` |
| [references/wasm.md](references/wasm.md) | `pxl-wasm` | Browser bindings, wasm build |

When changing generator output, bump `GEN_VERSION` and regenerate golden hashes only on purpose: `UPDATE_GOLDEN=1 cargo test -p pxl-cli --test golden`.
