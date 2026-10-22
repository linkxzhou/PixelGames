# Interim asset-stage failures

Run `games/battle-city/runs/r1792639385414`, frozen during repair round 2.
Round 1 gate: **failed** (`reports/assets.gates.json`). `provenance.not_template` passed.
`pxl validate` passed for the sheets that existed at the end of round 1.
Build counts are inferred from the event log: a `build_candidate` is charged to the asset whose shapes/recipe was written most recently.

Sheets that `pxl build` accepts now (exported x8): `player_tank`, `enemy_tank`, `bullet`.

| asset | build attempts (ok / fail) | last pxl error, verbatim | likely cause |
| --- | --- | --- | --- |
| player_tank | 1 / 9 | none — `pxl build` exits 0 | delivered |
| enemy_tank | 3 / 3 | none — `pxl build` exits 0 | delivered |
| bullet | 2 / 4 | none — `pxl build` exits 0. Earlier fails were `anim.fly.<dir>.frames` value `1` limit `>= 2 frames` | delivered after the model added a second fly frame |
| explosion | 0 / 6 | `E_QA` `build wrote outputs but 2 QA check(s) failed` / `anim.burst.N.diff_0_1: value=42.96% limit=at least 1.00%, and at most 35.00% unless a 1px shift explains it` / `anim.burst.N.diff_2_3: value=51.17% (shift-adjusted 50.78%) limit=at least 1.00%, and at most 35.00% unless a 1px shift explains it` | model — burst frames differ by more than the default 35% cap. The harness returned those checks; the spec was not revised enough |
| w | 0 / 1 | `E_SCHEMA` `macro pulse_glow has no argument 'steps'` / hint `pulse_glow takes: ramp` | model — recipe passed `steps` to a macro that only accepts `ramp`. Harness feedback is the pxl message itself |
| hq_base | 0 / 0 | not attempted | harness budget — round 1 hit `max_turns` (60) while earlier assets were still being repaired |
| hud_life_icon | 0 / 0 | not attempted | harness budget — same turn cap |
| hud_stage_label | 0 / 0 | not attempted | harness budget — same turn cap |
| b | 0 / 0 | not attempted | harness budget — same turn cap |
| x | 0 / 0 | not attempted | harness budget — same turn cap |
| h | 0 / 0 | not attempted | harness budget — same turn cap |
| g | 0 / 0 | not attempted | harness budget — same turn cap |
| t | 0 / 0 | not attempted | harness budget — same turn cap |
