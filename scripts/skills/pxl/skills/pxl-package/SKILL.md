---
name: pxl-package
description: step-400. Check coverage of built assets against asset-requirements.json, content-address them and write an asset-package, verify it, and record package approval. Deterministic, no model and no pixel work. Use when running step-400, packaging build output, checking coverage or digests, or approving a package revision.
---

# pxl-package

step-400 reads the `step-300 plan` output (`index.json` and per-asset directories) plus `asset-requirements.json`. It only reads files, computes digests and compares; it never touches pixels. Call it as `scripts/run.sh step-400 ...` from the `pxl` skill directory.

## Commands

```bash
run.sh step-400 run --in <300-build> --requirements <asset-requirements.json> --out <400-package> \
    [--audio-requirements <audio-requirements.json>] [--game-id <id>] [--levels <dir>] [--scope all|slice] [--pipeline <pipeline.json>] [--catalog-lock <catalog.lock.json>]
run.sh step-400 check   --in <400-package>                       # recompute digests, verify self-consistency
run.sh step-400 approve --in <400-package> --digest <manifest_digest>
run.sh step-400 status  --in <400-package>
```

- `--scope slice` needs `--pipeline` and keeps only `slice.assets[]`.
- Output uses `asset-package/v3` and includes `asset-package.json`, `asset-manifest.json`, `coverage.json`, `revision.manifest.json`, `engine-check.json` and `step.json`. Visual `delivered` must equal `required`; visual reports, metadata, previews and declared channels are digest-bound. Audio coverage is checked separately in `coverage.audio`.
- When audio requirements exist, every required ID needs a passing `audio-manifest/v1` entry. WAV and the sub-manifest are copied to `assets/audio/` and bound into the package/revision digests; older package formats are rejected rather than adapted.
- Pass `--levels <dir>` when level files exist. Without it step-500's `level_tiles_in_sheets` check is `NOT_ASSESSED` and `step-500 check` exits 11.
- `approve` is a human decision (verdicts APPROVED / NEEDS REVISION / REJECTED) and requires a current `visual-review/v1` with two independent reviewers. Accepted CONCERNS stay CONCERNS. Ask the user to run it; do not run it yourself unless told to.
- `--catalog-lock` copies the frozen prototype closure to `catalog.lock.json`, records it under `catalog_lock` in the package and binds it into the revision digest. It is refused unless every entry is human-approved and every dependency is present. `check` re-verifies the file, so editing a prototype or the lock after packaging invalidates the approval.
