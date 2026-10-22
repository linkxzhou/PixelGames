# v5 battle-city assets vs v4

Two assets-only runs on `games/battle-city`, design revision `d1792638056510` left approved. Model `deepseek-ai/DeepSeek-V4.1-Flash` via SiliconFlow. The harness was stopped on request during the second run. No later run was started. Macroquad was not started.

| run | id | binary | how it ended |
|---|---|---|---|
| v4 | `r1792639385414` | PR #4 harness (`b7ded1a`) | killed during round 2 |
| v5a | `r1792660166849` | `23d05c3` (main, before the fixes on this branch) | `E_SEMANTIC`: `illegal stage transition running -> running` after round 1 |
| v5b | `r1792661955007` | `95a645b` (repair-round, path, and stem fixes). `345cc39` was committed while this process was already running and is not in this run | SIGTERM during attempt 3 turn 5 (exit 143) |

v5b is the run to compare with v4. v5a is the first attempt on the optimized main, kept so the crash is visible.

Sheet counts below are `pxl validate --rules default` on `out/<id>/<id>.sheet.json` after the stop. That is the model output directory. It is a different number from the harness gate, which also walks fingerprint directories and the asset queue.

## Stats

Latency for v4 is the timestamp gap between `model_turn_started` and `model_turn_finished` (`elapsed_ms` was not logged). v5 latency is `elapsed_ms` on `model_turn_finished`. Wall time is the span from the first event to the last event. An unfinished call has no tokens and no latency.

| | v4 | v5a | v5b |
|---|---:|---:|---:|
| `out/<id>` sheets that validate | 3 / 13 | 12 / 13 | 13 / 13 |
| harness `assets.gates` | failed (5 checks) | failed (5 checks), then process error | failed (last written gate: 2 checks) |
| model calls finished | 78 | 60 | 125 |
| model calls started | 79 | 60 | 126 |
| turns | attempt unfinished logging; 60 finished in round 1, 18 finished in round 2, turn 18 started | attempt 1: 60, stop `max_turns` | attempt 1: 60, attempt 2: 60, attempt 3: 5 finished, turn 5 started |
| prompt tokens | 1,485,155 | 1,786,835 | 4,710,598 |
| completion tokens | 59,333 | 72,970 | 198,505 |
| total tokens | 1,545,488 | 1,859,805 | 4,909,103 |
| wall time | 1,157.7 s | 1,340.4 s | 4,705.1 s |
| latency min / median / mean / max (ms) | 2,392 / 6,546 / 14,813 / 165,425 | 2,927 / 7,391 / 22,291 / 208,865 | 2,085 / 7,315 / 37,603 / 660,505 |

v5b stalled calls (provider returned `tool_calls`, so these are slow responses, not harness hangs):

| attempt | turn | elapsed | prompt | completion |
|---:|---:|---:|---:|---:|
| 3 | 4 | 660,505 ms (11.0 min) | 21,077 | 2,056 |
| 1 | 26 | 618,747 ms (10.3 min) | 41,669 | 15,358 |
| 2 | 11 | 268,282 ms (4.5 min) | 24,227 | 14,808 |

v5a max is attempt 1 turn 21 at 208,865 ms. The v5b call that was in flight at SIGTERM (attempt 3 turn 5) has no `model_turn_finished` and no token count.

Token sums use `model_turn_finished` only. `candidate_written` repeats the parent turn and is not added.

## v5b per asset

Queue state is whatever `asset-queue.json` held at SIGTERM. Every id is `pending_build` with an empty `last_failure`, and each current input fingerprint is already in `built_fingerprints`. That is the demotion bug below, not a missing build. The last explicit `assets.build_candidate` for every required id returned ok. Post-stop `pxl validate --rules default` on `out/<id>/<id>.sheet.json` passes for all 13.

`hud_stage_label` was missing in v5a. v5b also wrote an extra `probe_explosion.recipe.json` (not a required id); its sheet validates. `provenance.not_template` passed.

| asset | queue at stop | `out/<id>` validate | last failing build in v5b, then the passing one |
|---|---|---|---|
| player_tank | pending_build, repairs 3 | pass | `E_QA` `frame.player_tank_fire_*.bbox` value 100%, limit 20%..95%. Later build ok |
| enemy_tank | pending_build, repairs 4 | pass | `E_QA` `frame.enemy_tank_fire_S_1.outline` value 2, limit 0. Later build ok |
| bullet | pending_build, repairs 1 | pass | `E_SCHEMA` `anim 'fly' has no macro or frames`. Later build ok |
| explosion | pending_build, repairs 4 | pass | `E_QA` `anim.burst.N.diff_2_3` value 38.28%, limit at most 35%. Later build ok |
| hq_base | pending_build, repairs 3 | pass | `E_QA` `frame.hq_base_destroyed_N_1.outline` value 2, limit 0. Later build ok |
| hud_life_icon | pending_build, repairs 0 | pass | no failed build |
| hud_stage_label | pending_build, repairs 1 | pass | explicit build ok. Auto-check copy `out/hud_stage_label.0c41505689e0/` still fails `frame.hud_stage_label_idle_N_0.bbox` value 0.00%, limit 20%..95% |
| b | pending_build, repairs 2 | pass | `E_QA` `frame.b_break_N_*.bbox` value 100%. Later `out/b` build ok. Auto-check copy `out/b.848f7a3cbbc5/` still fails three bbox checks at 100% |
| x | pending_build, repairs 1 | pass | `E_QA` `frame.x_idle_N_0.bbox` and `frame.x_spark_N_*.bbox` value 100%. Later build ok |
| w | pending_build, repairs 0 | pass | no failed build. Recipe uses two explicit `mask_fill` frames, not `pulse_glow` |
| h | pending_build, repairs 0 | pass | no failed build |
| g | pending_build, repairs 0 | pass | no failed build |
| t | pending_build, repairs 0 | pass | no failed build |

v5a `out/<id>` validate: pass for player_tank, enemy_tank, bullet, explosion, hq_base, hud_life_icon, b, x, w, h, g, t. `hud_stage_label` was never written. The queue at crash says `build_failed` / `E_IO` for every id that had a recipe, because auto-check then failed to open the recipe. Those `E_IO` lines are not pxl QA results. `provenance.not_template` passed.

The harness gate file for v5b is the round-2 gate (`events` seq 722), not a gate recomputed at SIGTERM. It failed `coverage.queue` (13 × `pending_build (never built)`) and `pxl.validate` on `b.sheet.json` and `hud_stage_label.sheet.json`. `find_sheets` validates every `*.sheet.json` under the attempt, including the fingerprint directories auto-check left behind. Those two stale sheets are the validate failures. `coverage.frames` was already `covered` (a `{id}_*` frame exists for all 13).

## Four v4 issues

1. Explosion frame diff above 35%. v4 `anim.burst.N.diff_0_1` was 42.96% and `anim.burst.N.diff_2_3` was 51.17% (shift-adjusted 50.78%). v5b still hit this once: `anim.burst.N.diff_2_3` value 38.28%. The tool result included that value and the 35% limit (`failed_checks` in `tool-results/`). The following build passed, and `out/explosion/explosion.sheet.json` validates. The delivered sheet is fixed. The model needed more than one try.

2. `w` `pulse_glow(steps=2)` was never built, so the schema error was never fed back. v5b's `w.recipe.json` does not call `pulse_glow`; the artist prompt now says the only argument is `ramp`, and the model wrote two explicit ripple frames. The one build succeeded. The feedback path itself: v5a auto-check did run on `w` and the error was the wrong one (`E_IO` cannot read a workspace-relative path, because pxl's cwd was the attempt directory). The process then died before a repair prompt. v5b's binary has the absolute-path fix. Its auto-checks (`hud_stage_label`, `b`) returned `E_QA` with check id, value, and limit, and attempt 2 started with that note. So the "error never fed back" hole is closed in the v5b binary. `w` did not exercise `pulse_glow` again.

3. Turn budget. `max_model_turns_per_task` is still 60. v4 round 1 used all 60. v5a used all 60 and never started round 2. v5b used all 60 in attempt 1 and all 60 in attempt 2, then was stopped in attempt 3. Per-round exhaustion is still there. What changed is that a repair round can start: v5b completed two rounds. `task_wall_clock_secs` is 1800 in `harness.config.json` and is not read by the budget ledger (`wall_clock_budget_secs` stays 0). That field was not fixed.

4. Build stdout missing from the log. `events.jsonl` still does not embed pxl stdout. v5 `tool_finished` adds `result_digest`, `elapsed_ms`, and `pxl_error_code`. The text the model saw, including `failed_checks` with value and limit, is in `tool-results/<call_id>.txt` (copied under each run directory here). Auto-check writes `build-reports/*.json` with the checks array. v4 had none of that. The jsonl line is still a digest, not the stdout.

## Harness bugs fixed on this branch

`95a645b` (`95a645bbdbf7a92e369973c39a95d22542240366`), in the v5b binary, not in v5a:

- Recipe stem. `Path::file_stem("player_tank.recipe.json")` is `player_tank.recipe`, so explicit builds never matched a required id and never updated the queue. v5a queue `build_attempts` stayed 0 until the broken auto-check. v5b recorded real build and repair counts during the run.
- Repair-round transition. After round 1 the stage was still `running` and called `set_stage(Running)` again. v5a exited with `running -> running`. The stage now enters `validating` before gates, matching the design stage. v5b ran attempt 2 and attempt 3.
- Auto-build paths. The recipe argument was `games/battle-city/runs/.../w.recipe.json` while pxl's cwd was the attempt directory, so every v5a auto-check was `E_IO` / `No such file` even though the file existed. v5b passes absolute paths. Its auto-checks report `E_QA`.

`345cc39` (`345cc39f945b78124c526d59f36a894892f06014`), committed during v5b, not executed by either run:

- Queue state vs fingerprint. `mark_dirty` set `PendingBuild` even when the input digest had not changed and that fingerprint was already in `built_fingerprints`. Auto-build then skipped the rebuild, and `coverage.queue` reported `pending_build (never built)` for assets whose sheets were already on disk. v5b's round-2 gate shows all 13 ids in that state, and the queue file at SIGTERM is the same shape. The fix keeps `Built` / `BuildFailed` / `Blocked` when the fingerprint is unchanged, and if a demoted `PendingBuild` is skipped, restores `Built` or `BuildFailed` from `last_error_code`.

Found in v5b and not changed:

- `pxl.validate` in the gate validates every `*.sheet.json` under the attempt, so a failed auto-check directory (`out/b.<fp>/`, `out/hud_stage_label.<fp>/`) keeps the gate red after `out/<id>/` has a passing sheet.
- `task_wall_clock_secs` is unused. See issue 3.

## Files

- `r1792660166849/` and `r1792661955007/`: verbatim `events.jsonl` (byte-identical to the run log; scanned, no API key), `assets.gates.json`, `asset-queue.json`, `tool-results/`, `build-reports/`. v5a also has its own x8 sheets.
- Root `*.sheet.x8.png`, `contact-sheet.png`, and `v4-vs-v5.png` are v5b. Nearest-neighbor x8 of indexed pixels, 0 index mismatches. `v4-vs-v5.png` is bullet, enemy_tank, explosion, and player_tank (the sheets present in both the v4 run and v5b).
