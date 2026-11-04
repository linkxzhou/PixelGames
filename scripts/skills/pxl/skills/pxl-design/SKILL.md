---
name: pxl-design
description: step-100. Turn a brief.json into a design/v3 package with visual intent, approve requirements, or analyze run lessons. Use when running step-100 init / run / gate / approve / status / lessons or working with the brief to design stage.
---

# pxl-design

**step-100: brief to design/v3 package (LLM).**

Produces `asset-requirements.json`, `visual-intent.json`, optional `audio-requirements.json`, `style-guide.json`, `pipeline.json`, `knowledge.lock.json`, model turn logs and approval gates. Visual intent binds profile, native grid, palette, roles, materials, animation, scenes and review risks. Requires human approval before step-200 can proceed; legacy migration commands and old contract readers are not available.

## Commands

- `init --brief FILE --out DIR`: fold a brief into a new project workspace
- `run --in DIR`: invoke the designer (default: `--provider fake`)
- `gate --in DIR`: verify gates without a model; check approval digests
- `approve --in DIR --digest HEX`: record human approval (from `revision.manifest.json`)
- `status --in DIR`: print last run state from `step.json`
- `lessons --runs DIR --out DIR`: deterministically extract candidate lessons with evidence
- `lessons compare --baseline A.json --candidate B.json --runs DIR --out DIR`: compare locked experiments without claiming significance
- `lessons audit --runs visual-benchmark/manifest.json --out DIR`: audit holdout engineering vs human review status

Provider: `--provider fake` uses the env var `STEP_100_FAKE_TURNS` as a transcript. `--provider openai --env <file>` for real model calls (needs `OPENAI_API_KEY`). Other options: `--max-turns 24`, `--until draft` (skip gates), `--lang en|zh`.

Exit 10 means waiting for approval. Agent must stop and ask the user to `approve`.

`init` writes `pipeline.json` (workspace lock) and `.staging/` (model output). Acceptance becomes current; failures land in `.rejected`.

