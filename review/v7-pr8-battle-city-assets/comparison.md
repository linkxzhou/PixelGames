# v7 battle-city assets vs v4 and v5

Full assets stage on `games/battle-city`, design revision `d1792638056510` left approved. Model `deepseek-ai/DeepSeek-V4.1-Flash` via SiliconFlow. The harness is `main` at `4c57795` (PR #6), which already has the stem lookup, unchanged-fingerprint queue behavior, and pxl error text in tool results. No macroquad or playtest stage was started.

The published revision is `a1792735985822`. `provenance.not_template` passed: authored JSON is not a bundled template document. `examples/battle_city/` and `review/v2e-battle-city-samples/` were references only.

## Runs

| run | id | how it ended |
|---|---|---|
| v7 abort | `r1792727918505` | `E_PROVIDER`, curl exit 28, harness exit 16, during attempt 1 turn 16. Log: `aborted-r1792727918505/events.jsonl` |
| v7 | `r1792735985822` | attempt 2 stopped on the model's own `stop`; `assets.gates` passed. This directory's `events.jsonl` is that run, verbatim |

The abort used uncommitted `.env` `API_TIMEOUT_SECS=900` (`API_MAX_RETRIES=1`). Turn 16 never finished. The only recipe on disk was `t.recipe.json`, and its last build was `E_QA` (tile full-bleed and seam). That run is not the deliverable.

The successful run used uncommitted `.env` `API_TIMEOUT_SECS=1500` and `API_MAX_RETRIES=1`. `harness.config.json` still says `request_timeout_secs` 600; `from_dotenv` replaces that with `API_TIMEOUT_SECS` when the variable is set. `.env` is not committed.

Sheet counts are `pxl validate --rules default` on `out/<id>/<id>.sheet.json` in revision `a1792735985822`.

## Stats

Latency is `elapsed_ms` on `model_turn_finished`. Wall time is the first event to the last event. v4 and v5b numbers are copied from `review/v5-pr5-battle-city-assets/comparison.md`.

| | v4 | v5b | v7 |
|---|---:|---:|---:|
| `out/<id>` sheets that validate | 3 / 13 | 13 / 13 | 13 / 13 |
| harness `assets.gates` | failed | failed | passed (9 checks, 0 failures) |
| model calls finished | 78 | 125 | 111 |
| model calls started | 79 | 126 | 111 |
| turns | round 1: 60 finished, round 2 unfinished | attempt 1: 60, attempt 2: 60, attempt 3: 5 finished | attempt 1: 60 (`max_turns`, gate failed, 5 checks), attempt 2: 51 (50 `tool_calls`, then `stop`) |
| prompt tokens | 1,485,155 | 4,710,598 | 3,872,461 |
| completion tokens | 59,333 | 198,505 | 302,055 |
| total tokens | 1,545,488 | 4,909,103 | 4,174,516 |
| wall time | 1,157.7 s | 4,705.1 s | 32,257.2 s |
| latency min / median / mean / max (ms) | 2,392 / 6,546 / 14,813 / 165,425 | 2,085 / 7,315 / 37,603 / 660,505 | 1,911 / 11,194 / 161,529 / 1,840,731 |

The v7 mean is pulled by slow calls. The longest finished call is attempt 1 turn 4 at 1,840,731 ms. That span is a 1,500 s curl timeout plus a retry that returned `tool_calls` (25,385 completion tokens). Every started call has a `model_turn_finished`. No call was in flight at exit.

Attempt 1's failing gate report was replaced when attempt 2 passed. The event log keeps `failures: 5` and digest `sha256:0f0f4d996657c2691b2f3b07e53217d810203004de5bb3e67a34a2977fee39e1`. The file `assets.gates.json` is the passing report (`sha256:9fda5f569e035a1c4643ebdbf90ed617058563b8d94160427b9604ea9c44cdc8`).

## v7 per asset

Final queue is `asset-queue.json`. Every required id is `built`, `last_error_code` is null, and the current input fingerprint is in `built_fingerprints`. `pxl validate --rules default` passes for every `out/<id>/<id>.sheet.json`.

| asset | queue | `out/<id>` validate | repairs | last failing build, then the passing sheet |
|---|---|---|---:|---|
| player_tank | built | pass | 7 | `E_QA` `anim.fire.N.diff_0_1` value 0.00%, limit at least 1%. Also blocked once: `E_SEMANTIC` unknown palette cycle `tread` (rejected copy `out/player_tank.5b4d30329d99/`). Later build ok |
| enemy_tank | built | pass | 2 | `E_QA` `frame.enemy_tank_fire_S_1.bbox` value 100%, limit 20%..95%. Later build ok |
| bullet | built | pass | 0 | no failed build |
| explosion | built | pass | 2 | `E_QA` `anim.burst.N.effect_decays` value `34>43>60>94`, limit last frame smaller than the peak. Later build ok |
| hq_base | built | pass | 2 | `E_QA` `frame.hq_base_damaged_N_0.bbox` value 100%, limit 20%..95%. Later build ok |
| hud_life_icon | built | pass | 0 | no failed build |
| hud_stage_label | built | pass | 0 | no failed build |
| b | built | pass | 3 | `E_SCHEMA` `anim 'idle' has no macro or frames`, then `E_QA` `tile.b_idle_0.seam`. Later build ok |
| x | built | pass | 0 | no failed build |
| w | built | pass | 0 | no failed build. Two explicit `palette_swap` frames. The recipe does not call `pulse_glow` |
| h | built | pass | 1 | `E_QA` `tile.h_idle_0.seam` value no edge pixel on ramp `hazard`. Later build ok |
| g | built | pass | 0 | no failed build |
| t | built | pass | 0 | no failed build |

Extra sheets the model wrote: `tankprobe` validates. `probe` is under `out/probe/.rejected/` and is not a deliverable.

No required asset is still failing.

## Prior harness bugs

1. `running -> running` on a repair round. Absent from this event log. Attempt 2 started after attempt 1's gate, which is a legal transition.
2. Auto-build `E_IO` from a workspace-relative pxl cwd. Absent. Build failures that remain are `E_QA`, `E_SCHEMA`, and one `E_SEMANTIC`, and the tool result includes `failed_checks` with value and limit.
3. Recipe stem mismatch (`player_tank.recipe.json` vs id `player_tank`). Absent. Those stems built.
4. Unchanged-fingerprint demotion to `pending_build` while `built_fingerprints` already contains that fingerprint, so `coverage.queue` says never built. The final queue does not do that: each required id is `built` and `coverage.queue` is `0 gaps`. Mid-run `pending_build` lines happened when a palette or recipe write changed the input fingerprint (`build_attempts` reset to 0, older fingerprints kept). That is the changed-input path.
5. `find_sheets` still walks every `*.sheet.json` except `.staging` and `.rejected`, including fingerprint directories. This revision still has `out/b.86ac9a31bed4/` and `out/hud_life_icon.e521f67d21e2/`. Both pass `pxl validate --rules default`, so `pxl.validate` stayed green. The v5 failure mode (a stale sheet fails the gate after `out/<id>` is good) did not fire. The walker is unchanged, so a bad fingerprint copy can still fail a later run.
6. `task_wall_clock_secs` is 1800 in `harness.config.json` and is still not read by the budget ledger. This run's wall time is 32,257 s.
7. `events.jsonl` still stores `result_digest`, not pxl stdout. The text is in the run's `tool-results/`.
