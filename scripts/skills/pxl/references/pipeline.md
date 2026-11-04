# Workspace layout and contracts

## Directory structure

```
work/
├── 100-design/        # step-100 output
│   ├── asset-requirements.json   ⟶ 200
│   ├── style-guide.json          ⟶ 200
│   ├── pipeline.json             ⟶ all
│   ├── revision.manifest.json    (approval)
│   └── step.json
├── 200-art-spec/      # step-200 output
│   ├── spec-manifest.json        ⟶ 300
│   ├── <asset-id>/
│   │   ├── <asset-id>.recipe.json  (or .ascii.json, .svg, .shapes.json)
│   │   └── palette.json
│   └── step.json
├── 300-build/         # step-300 output
│   ├── index.json                ⟶ 400
│   ├── <asset-id>/
│   │   ├── <asset-id>.pxl.json
│   │   ├── <asset-id>.sheet.png
│   │   ├── <asset-id>.sheet.json
│   │   └── palette.json
│   └── step.json
├── 400-package/       # step-400 output
│   ├── asset-package.json        ⟶ 500
│   ├── asset-manifest.json
│   ├── coverage.json
│   ├── revision.manifest.json    (approval)
│   └── step.json
└── 500-engine/        # step-500 output (JS mode)
    ├── web/                   # static browser directory
    ├── engine-build.json      # file sha256 manifest and input digest
    ├── engine-check.json
    ├── playtest/report.json
    ├── playtest/frames/
    └── step.json
```

## Handoff key files

- **pipeline.json**: written once by `step-100 init`, read by all steps. Holds workspace root, game id, slice definition, and optional `runtime.mode` (`js` by default, or `rust`). Do not switch it in an existing workspace.
- **asset-requirements.json / audio-requirements.json**: visual and optional audio needs; audio uses stable IDs, kinds, triggers and loop flags.
- **spec-manifest.json**: where the art specs and optional `audio/audio.spec.json` live, dependency digests, scope (all or slice), frozen `visual-intent.snapshot.json`, `visual-plan.json` and `knowledge.lock.json`.
- **index.json**: `asset-build/v4` result summary for each recipe and optional audio render; step-400 reads this to find sheets and `audio-manifest.json`.
- **asset-package.json**: `asset-package/v3` content-addressed package with sheets, palettes, levels, visual reports/metadata/previews/channels and optional WAV audio for the engine.
- **step.json**: every step writes this file, schema is `contracts/step.schema.json`. Reports status, inputs, exit code, contract version.

## Approval digest

`revision.manifest.json` holds `manifest_digest` (sha256). `gate` or `run` computes it; human runs `approve --digest <hex>`.

## Verdict four-value

`FAIL > CONCERNS > NOT_ASSESSED > PASS`. If a check could not run, it is `NOT_ASSESSED`, never `PASS`.

## Scope: all vs slice

`--scope slice` restricts to the assets in `pipeline.json` `slice.assets[]`. Vertical slice workflow: 200 → 300 → 400 → 500 on slice assets first, then a gate calls `step-500 slice`, and only if verdict is `PROCEED` the agent proceeds to a full `--scope all` run.
