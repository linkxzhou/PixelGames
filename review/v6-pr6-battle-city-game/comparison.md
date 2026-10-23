# v6 battle-city game vs v4/v5 asset runs

v6 is the first playable slice. Design revision `d1792638056510` stayed approved and was reused. The harness run is `games/battle-city/runs/r1792726174642` (`pxl-harness run --until playtest --from build`). Assets were not requested in that run: there is still no approved assets revision, and v5 already showed a full DeepSeek authoring pass (about 4.9M tokens on v5b) can validate sheets and still fail the harness gate. This PR does not re-archive those runs and does not start another multi-million-token asset pass.

Model configuration for a future asset run is unchanged in kind: SiliconFlow `deepseek-ai/DeepSeek-V4.1-Flash`, `API_BASE_URL=https://api.siliconflow.cn/v1`, key in `.env` / `API_KEY` only. `.env` is gitignored. `events.jsonl` in this directory was scanned and does not contain the key.

## What this run checked

| gate | result |
|---|---|
| `skill.macroquad` | passed, `harness/skills/macroquad/SKILL.md` |
| `compile.native` | passed |
| `compile.wasm32` | passed (`wasm32-unknown-unknown`) |
| `playtest.frames` | 120 fixed 60 Hz steps, `panicked: false` |
| `playtest.palette` | 0 off-palette pixels in the three screenshots |
| `playtest.player_moves` | player left `(1,10)` |
| `playtest.shoots` | 6 shots |
| `playtest.destroys` | 5 bricks, 0 enemies (stage 2 replay) |
| `playtest.screenshot` | `frame_0000.png` and the x3 nearest-neighbour copies |

Screenshots here are nearest-neighbour x3 of the 320×240 logical buffer: spawn (`frame_0000.x3.png`), mid-lane (`frame_0060.x3.png`), and the cleared north column (`frame_0119.x3.png`). The 1× PNGs and the gate JSON also live under the run directory.

## Against v4 and v5

| | v4 | v5b | v6 |
|---|---|---|---|
| pipeline | assets, killed in round 2 | assets, SIGTERM in attempt 3 | design reused, assets not requested, build + playtest succeeded |
| `out/<id>` sheets that validate | 3/13 | 13/13 | no new sheets |
| harness asset gate | failed | failed (stale fingerprint sheets + queue) | not run |
| playable runtime | no | no | yes, macroquad window + headless replay |
| screenshots | asset x8 sheets | asset x8 sheets | in-game playtest frames |

v4/v5 failure modes that this branch does not reopen: recipe stem vs queue id, `running -> running` blocking repair rounds, and relative pxl paths. Those stay as they were on `ebb8850`. v6 did not change `find_sheets`, so a future asset run can still fail `pxl.validate` on an old fingerprint directory even after `out/<id>` is good. `task_wall_clock_secs` is still not read by the budget ledger.

The runtime draws with the battle-city palette (index 0 clear, the other indices opaque). It does not load the unfinished pxl sheets, so the playtest does not depend on a red asset gate. Stage 3 enemy destruction is covered by a library test; the committed replay is stage 2, which is the brick-and-shoot lesson in the approved design.
