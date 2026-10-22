# Missing assets in run r1792639385414

Source log: `events.jsonl` in this directory, copied verbatim from `games/battle-city/runs/r1792639385414/events.jsonl` (412 lines, seq 0–411). A scan for `sk-…`, `Bearer`, `AKIA…`, `siliconflow`, and `api_key=<value>` found nothing. No redaction. Provider call ids are tool-call ids, not credentials.

The log does **not** store tool stdout, pxl `checks`, or the chat text sent back to the model. `tool_started.arguments_digest` is sha256 of the raw arguments string. Every `assets.build_candidate` digest in this file matches one of four compact JSON shapes (`{"recipe": "….recipe.json", "out_dir": "out/…"}` or the same keys swapped). That match is how build targets below were recovered. It is not a new pxl run.

Pxl stdout for overwritten specs is gone. The only verbatim QA text in this note is for the two files still on disk whose digests match the last write: `w.recipe.json` and the last `explosion.recipe.json`. Those two strings come from a local `pxl build` of those files while the run was frozen, in the previous session. `w` was never passed to `assets.build_candidate`, so that schema error was **not** shown to the model.

## events.jsonl field schema

One JSON object per line. `event` is a serde internally tagged enum (`rename_all = snake_case`). Every line also has `seq` (u64, from 0) and `ts_ms` (unix milliseconds). Variants present in this file:

| event | fields |
|---|---|
| `run_created` | `harness_version`, `config_digest` |
| `stage_status` | `stage`, `status` |
| `model_turn_started` | `stage`, `task_id`, `turn` |
| `model_turn_finished` | `stage`, `task_id`, `turn`, `stop_reason`, `prompt_tokens`, `completion_tokens` |
| `tool_started` | `stage`, `task_id`, `attempt`, `tool`, `arguments_digest`, `call_id` |
| `tool_finished` | `stage`, `task_id`, `attempt`, `tool`, `ok`, `exit_code`, `artifact_digests`, `error_code`, `call_id` |
| `candidate_written` | `stage`, `task_id`, `attempt`, `tool`, `call_id`, `turn`, `path`, `digest`, `prompt_tokens`, `completion_tokens` |
| `gate_finished` | `stage`, `task_id`, `passed`, `failures`, `report_digest` |

`attempt` is the repair round, 1-based, not a per-asset counter. `turn` resets to 0 each round. Token fields on `candidate_written` copy the parent turn; summing them double-counts a turn that wrote two files. `tool_finished.error_code` is set only when the harness raises. A failed pxl build is `ok: false` and `error_code: null`, because the tool returns a normal outcome. Order inside a turn: `model_turn_finished`, then for each call `tool_started`, `tool_finished`, and `candidate_written` if that write returned ok.

## Summary

- Finished LLM calls: **78** (`model_turn_finished`). Seq 411 is a `model_turn_started` for round 2 turn 18 with no finish; the process was killed there.
- Rounds: **2**. Round 1 is turns 0–59 (hit `max_turns`). Round 2 finished turns 0–17.
- Tokens from `model_turn_finished` only: prompt **1486155**, completion **59333**, total **1545488**.
- Wall clock seq 0 → seq 411: **1157.7 s** (19.3 min).
- Model latency (`started` → `finished`): min **2392** ms, median **6546** ms, mean **14812** ms, max **165425** ms (round 1 turn 30, 7621 completion tokens, enemy + bullet shapes).
- Finished tool calls: write_candidate 46, build_candidate 29 (6 ok / 23 fail), read_candidate 12, list_capabilities 10, validate_candidate 3 (all ok), design.read_frozen 2, assets.read_requirements 1.
- Build targets, from argument digests: `player_tank` 10 (1 ok / 9 fail), `enemy_tank` 6 (4 ok / 2 fail), `bullet` 6 (1 ok / 5 fail), `explosion` 7 (0 ok / 7 fail), `w` 0.
- `budget.json` is still all zeros; the ledger was not flushed. Use the token totals above.
- Gate after round 1: **failed**, 5 checks, report digest `sha256:fd16a6f471fc31c4926f2ac81edc025bbb1cf6a1b8a9312e2368ad8dfdf6a217`. `provenance.not_template` passed. `pxl.validate` passed for the sheets that existed (player, enemy, bullet). Full file: `assets.gates.json`.

Turns that touched each missing asset. A turn counts if it wrote `{id}.shapes.json` or `{id}.recipe.json`, or issued `build_candidate` whose arguments name that recipe.

| asset | turns | round.turn |
|---|---|---|
| `explosion` | 17 | 1.57, 1.58, 1.59, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9, 2.10, 2.11, 2.12, 2.13, 2.14, 2.15, 2.16, 2.17 |
| `w` | 1 | 2.4 |
| `hq_base` | 0 | — |
| `hud_life_icon` | 0 | — |
| `hud_stage_label` | 0 | — |
| `b` | 0 | — |
| `x` | 0 | — |
| `h` | 0 | — |
| `g` | 0 | — |
| `t` | 0 | — |

Delivered assets, same definition: `player_tank` 17 turns, `enemy_tank` 15, `bullet` 13. Round 1 turns 0–2 only called `design.read_frozen` and `assets.list_capabilities`. Palette rewrites on turns 8, 10, 13, 14, 16, 18, and 24 sit before the first successful player build (turn 27).

## Per-turn timeline

Latency is model time only. Tools are the calls that follow `model_turn_finished` and precede the next start. Build lines quote the argument string whose sha256 matches `arguments_digest`.

| round | turn | latency_ms | prompt | completion | stop | tools after the turn |
|---|---|---|---|---|---|---|
| 1 | 0 | 2392 | 3634 | 69 | tool_calls | design.read_frozen ok 01a161a2259f9b6e39cebcda7605c082; assets.list_capabilities ok 01a161a2259f9b6e39cebcda7605c083 |
| 1 | 1 | 2822 | 9405 | 85 | tool_calls | assets.list_capabilities ok 01a161a230eca9b90aad02cfce8858ce; assets.list_capabilities ok 01a161a230eca9b90aad02cfce8858cf |
| 1 | 2 | 2916 | 9727 | 74 | tool_calls | assets.list_capabilities ok 01a161a23c724215df4ef49554e9a617; assets.list_capabilities ok 01a161a23c724215df4ef49554e9a618 |
| 1 | 3 | 49454 | 9997 | 3673 | tool_calls | write palette.json 01a161a2fcdbc0616aa37dce77c915be; write player_tank.shapes.json 01a161a2fcdbc0616aa37dce77c915bf |
| 1 | 4 | 11033 | 10595 | 833 | tool_calls | write player_tank.recipe.json 01a161a328a1dd170a1d67e13924683b |
| 1 | 5 | 2804 | 11055 | 66 | tool_calls | build fail {"recipe": "player_tank.recipe.json", "out_dir": "out/player_tank"} 01a161a383d70c13e7650fa06361f921 |
| 1 | 6 | 13754 | 11200 | 690 | tool_calls | write player_tank.recipe.json 01a161a3b99a8cbea198d6be02f27dbf |
| 1 | 7 | 3089 | 11646 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a37568f88114f05e1cb98bb589 |
| 1 | 8 | 9491 | 11790 | 419 | tool_calls | write palette.json 01a161a3eac0c6806ba8773d167e2adc |
| 1 | 9 | 3006 | 12062 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a3f681d5fda05b4613ba2c2dea |
| 1 | 10 | 4758 | 12213 | 319 | tool_calls | write palette.json 01a161a3b966e872c386b70caec50fe8 |
| 1 | 11 | 2911 | 12561 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a3c498d768d22059502b594370 |
| 1 | 12 | 5742 | 12712 | 241 | tool_calls | assets.read_candidate ok 01a161a3db502e0acb0be992334de1bd |
| 1 | 13 | 7682 | 13047 | 472 | tool_calls | write palette.json 01a161a3f92a19745634983838503c10 |
| 1 | 14 | 11783 | 13288 | 762 | tool_calls | write palette.json 01a161a4272bd6c13935bd08c604dc90 |
| 1 | 15 | 2887 | 13556 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a482540aac74f276c4ac68e457 |
| 1 | 16 | 6300 | 13695 | 320 | tool_calls | write palette.json 01a161a49b30608729a1ed3903273110 |
| 1 | 17 | 2684 | 13958 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a45593bd1a95fc262aa6664eb6 |
| 1 | 18 | 24004 | 14111 | 978 | tool_calls | write palette.json 01a161a5037138408b5974b984bb805e |
| 1 | 19 | 2595 | 14371 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a4bded61c3fb239cfba339cba9 |
| 1 | 20 | 23852 | 14871 | 1693 | tool_calls | write player_tank.shapes.json 01a161a51a47de1f12978ff166b64035 |
| 1 | 21 | 18922 | 15223 | 1242 | tool_calls | write player_tank.recipe.json 01a161a565315813df990fb0b9ad6e1a |
| 1 | 22 | 5732 | 15669 | 280 | tool_calls | write player_tank.shapes.json 01a161a5cb50a46f53710dfdc1c941d4 |
| 1 | 23 | 2638 | 16002 | 66 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a585745226086cbb55d07edeb4 |
| 1 | 24 | 10956 | 16144 | 700 | tool_calls | assets.read_candidate ok 01a161a5b06d3d8dc3b230afe6a6a640; write palette.json 01a161a5b06d3d8dc3b230afe6a6a641 |
| 1 | 25 | 6546 | 16704 | 302 | tool_calls | build fail {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a5ca4dd888d0025983e3c25f55 |
| 1 | 26 | 7683 | 16990 | 588 | tool_calls | write player_tank.recipe.json 01a161a5e85baac8fd1f0efd9c6f41eb |
| 1 | 27 | 2789 | 17443 | 66 | tool_calls | build ok {"out_dir": "out/player_tank", "recipe": "player_tank.recipe.json"} 01a161a5f36c2172b6230bfb7850d59d |
| 1 | 28 | 5923 | 17593 | 191 | tool_calls | assets.validate_candidate ok 01a161a65a8717e13eb3122ac8f2a2dd |
| 1 | 29 | 4088 | 17707 | 86 | tool_calls | assets.read_candidate ok 01a161a669783bef1cbd20b12d56a7f7 |
| 1 | 30 | 165425 | 23123 | 7621 | tool_calls | write enemy_tank.shapes.json 01a161a8efb09407e00cf8006fec8c5a; write bullet.shapes.json 01a161a8efb09407e00cf8006fec8c5b |
| 1 | 31 | 23895 | 23690 | 1179 | tool_calls | write enemy_tank.recipe.json 01a161a94de535e04f3f2f10c184f712; write bullet.recipe.json 01a161a94de535e04f3f2f10c184f713 |
| 1 | 32 | 2961 | 24476 | 67 | tool_calls | build fail {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161a909cc243ab5cf8b95866a9d5f |
| 1 | 33 | 27756 | 25065 | 1081 | tool_calls | write enemy_tank.shapes.json 01a161a9c61450c84968505b47e96e36 |
| 1 | 34 | 16767 | 25435 | 889 | tool_calls | build fail {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161a9b7c236e99e7994bbbf9b3a38 |
| 1 | 35 | 109911 | 25752 | 4113 | tool_calls | write enemy_tank.shapes.json 01a161abb49ffeebfd8c88b7a107e504 |
| 1 | 36 | 46940 | 26148 | 2854 | tool_calls | write enemy_tank.shapes.json 01a161ac1c41c95121900860073f41f6 |
| 1 | 37 | 2966 | 26545 | 67 | tool_calls | build ok {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161ac2856b0f63a70359fc7d45e1b |
| 1 | 38 | 9714 | 26699 | 414 | tool_calls | write enemy_tank.shapes.json 01a161ac9e37a18ea6243a125e008755 |
| 1 | 39 | 2966 | 27097 | 67 | tool_calls | build ok {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161ac59c69718f13f90c180a7b3c5 |
| 1 | 40 | 10371 | 27251 | 441 | tool_calls | write enemy_tank.shapes.json 01a161acd26d2efaf2f576bec30348bf |
| 1 | 41 | 3585 | 27668 | 67 | tool_calls | build ok {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161ac90496673da0817084fd19758 |
| 1 | 42 | 8481 | 27822 | 236 | tool_calls | assets.read_candidate ok 01a161ad01a51f2b636d647d6ea1c04c |
| 1 | 43 | 11061 | 28238 | 307 | tool_calls | build fail {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161ad2cbf210daadc64de5f2e3d46 |
| 1 | 44 | 16131 | 28378 | 677 | tool_calls | write enemy_tank.shapes.json 01a161ad6b761c6f697c5e20797c8680 |
| 1 | 45 | 8260 | 28745 | 531 | tool_calls | write bullet.recipe.json 01a161ad3c591f6fa4eb35a4a92124a9 |
| 1 | 46 | 7276 | 29097 | 476 | tool_calls | build fail {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161ad58518a5ffa4df4f5cd0a4078; write enemy_tank.shapes.json 01a161ad58518a5ffa4df4f5cd0a4077 |
| 1 | 47 | 8504 | 29643 | 553 | tool_calls | write bullet.recipe.json 01a161ad7a19d39ca8f2a9822cbcf9fc |
| 1 | 48 | 3390 | 29995 | 100 | tool_calls | build fail {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161ad87880844b786e17e27d47fa7 |
| 1 | 49 | 29631 | 30161 | 1081 | tool_calls | write bullet.recipe.json 01a161ae4af3f5fb446d0d60a4e91f34 |
| 1 | 50 | 3029 | 30514 | 63 | tool_calls | build fail {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161ae0707fe63d029d21caa242a9d |
| 1 | 51 | 22896 | 30796 | 795 | tool_calls | write bullet.recipe.json 01a161aeb0060a8f6ef59ac1e2ee5c30 |
| 1 | 52 | 3131 | 31148 | 63 | tool_calls | build fail {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161ae6d16acf88305deeb07aebd98 |
| 1 | 53 | 14162 | 31291 | 897 | tool_calls | write bullet.recipe.json 01a161aea41c098bf2851be4d09a6595 |
| 1 | 54 | 3479 | 31663 | 63 | tool_calls | build ok {"out_dir": "out/bullet", "recipe": "bullet.recipe.json"} 01a161aeb1c5978984c08cde072e3a25 |
| 1 | 55 | 6385 | 31800 | 158 | tool_calls | assets.read_candidate ok 01a161af1ae2ed067cd9a21eb946ff62 |
| 1 | 56 | 6183 | 32203 | 309 | tool_calls | assets.validate_candidate ok 01a161aee31afec5d00a4e0e542169cb; assets.validate_candidate ok 01a161aee31afec5d00a4e0e542169cc |
| 1 | 57 | 23866 | 32413 | 1326 | tool_calls | build ok {"out_dir": "out/enemy_tank", "recipe": "enemy_tank.recipe.json"} 01a161af3faf49b8ebcc4a5451db13eb; write explosion.shapes.json 01a161af3fb0b10a84a5d76a3019f47b |
| 1 | 58 | 7968 | 32828 | 482 | tool_calls | write explosion.recipe.json 01a161af5f8b9d45251968622b19d1f8 |
| 1 | 59 | 14069 | 33143 | 898 | tool_calls | write explosion.shapes.json 01a161af96acdb0b6c2ea6aecfba2169; write explosion.recipe.json 01a161af96acdb0b6c2ea6aecfba216a |
| 2 | 0 | 2624 | 2043 | 53 | tool_calls | design.read_frozen ok 01a161aff0e3c67fa1249b3bca0db61a; assets.read_requirements ok 01a161aff0e3c67fa1249b3bca0db61b |
| 2 | 1 | 7240 | 8185 | 307 | tool_calls | assets.list_capabilities ok 01a161b00d5ea5b144247c7bddb75838; assets.list_capabilities ok 01a161b00d5ea5b144247c7bddb75839; assets.list_capabilities ok 01a161b00d5ea5b144247c7bddb7583a; assets.list_capabilities ok 01a161b00d5ea5b144247c7bddb7583b; assets.list_capabilities ok 01a161b00d5ea5b144247c7bddb7583c |
| 2 | 2 | 3724 | 8862 | 167 | tool_calls | assets.read_candidate ok 01a161afcc9cde4a18fc6b5ae63964b2; assets.read_candidate ok 01a161afcc9cde4a18fc6b5ae63964b3 |
| 2 | 3 | 6871 | 9592 | 316 | tool_calls | assets.read_candidate ok 01a161b0374f4433cd4b96509815e42c; assets.read_candidate ok 01a161b0374f4433cd4b96509815e42d; assets.read_candidate ok 01a161b0374f4433cd4b96509815e42e; assets.read_candidate ok 01a161b0374f4433cd4b96509815e42f; assets.read_candidate ok 01a161b0374f4433cd4b96509815e430 |
| 2 | 4 | 125669 | 11349 | 9127 | tool_calls | write explosion.shapes.json 01a161b1d0d1feea596bd3310a1cf4d1; write explosion.recipe.json 01a161b1d0d1feea596bd3310a1cf4d2; write w.shapes.json 01a161b1d0d1feea596bd3310a1cf4d3; write w.recipe.json 01a161b1d0d1feea596bd3310a1cf4d4 |
| 2 | 5 | 4180 | 12541 | 64 | tool_calls | build fail {"recipe": "explosion.recipe.json", "out_dir": "out/explosion"} 01a161b1e2c16e33f637c24291e31cb8 |
| 2 | 6 | 37458 | 12888 | 2259 | tool_calls | write explosion.recipe.json 01a161b274ac5eb2507a7361f644c04a |
| 2 | 7 | 3140 | 13312 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b2d10ffb4b7a7a54eaa6f25ffb |
| 2 | 8 | 5733 | 13505 | 435 | tool_calls | write explosion.recipe.json 01a161b29819019fc2ff789c6699a90b |
| 2 | 9 | 3634 | 13914 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b2f608af6239d943260a81a4bf |
| 2 | 10 | 22040 | 14091 | 1293 | tool_calls | write explosion.recipe.json 01a161b2fc22f585384b23db133bcc36 |
| 2 | 11 | 3211 | 14507 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b308a33bc76c6ca3fb0e8a02db |
| 2 | 12 | 4947 | 14698 | 362 | tool_calls | write explosion.recipe.json 01a161b31c2c2ba81df8039a3385a703 |
| 2 | 13 | 2913 | 15116 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b377986eeadd5709ea5aaf6d73 |
| 2 | 14 | 12717 | 15265 | 531 | tool_calls | write explosion.recipe.json 01a161b3a8ffe490950f68fc2ce981f8 |
| 2 | 15 | 3350 | 15687 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b3b682d4b0160609781fe9406c |
| 2 | 16 | 20945 | 16051 | 1549 | tool_calls | write explosion.shapes.json 01a161b3b80a74d550fa989fed790561; write explosion.recipe.json 01a161b3b80a74d550fa989fed790562 |
| 2 | 17 | 2609 | 16753 | 64 | tool_calls | build fail {"out_dir": "out/explosion", "recipe": "explosion.recipe.json"} 01a161b3c2bdcc7a267f04d145996c12 |

## Where the single-letter ids come from

`game-design.json` `requirement_ids` is:

`["player_tank", "enemy_tank", "bullet", "explosion", "hq_base", "hud_life_icon", "hud_stage_label", "b", "x", "w", "h", "g", "t"]`

The letters are the level-grid alphabet, then repeated as asset ids. Rule statements:

- `r_move_blocked`: "The player tank moves one tile step per input on tiles whose semantics are not blocking (floor f, grass t, goal pad g); brick b, steel x and water w stop it."
- `r_brick_destructible`: "A bullet impact on a brick tile b destroys that brick tile, removes the bullet and spawns an explosion; brick tiles are the only destructible wall type."
- `r_steel_indestructible`: "Steel tiles x stop both tanks and bullets; a bullet hitting steel is removed with a small explosion and the steel tile survives."
- `r_water_impassable`: "Water tiles w are impassable for the player tank and for enemy tanks but do not stop bullets."
- `r_hazard_lose`: "Standing on a hazard tile h destroys the player tank and loses the stage immediately."
- `r_reach_exit`: "Stepping onto a goal pad tile g completes the stage; every stage exit is reachable from the spawn tile by walking only on non-blocking tiles."

`levels/level_1.json` `tile_semantics` (level_2 and level_3 copy the same seven rows):

```json
[
  {
    "tile_id": "f",
    "behaviour": "floor",
    "blocks": false
  },
  {
    "tile_id": "b",
    "behaviour": "wall",
    "blocks": true
  },
  {
    "tile_id": "x",
    "behaviour": "wall",
    "blocks": true
  },
  {
    "tile_id": "w",
    "behaviour": "water",
    "blocks": true
  },
  {
    "tile_id": "h",
    "behaviour": "hazard",
    "blocks": false
  },
  {
    "tile_id": "g",
    "behaviour": "goal_pad",
    "blocks": false
  },
  {
    "tile_id": "t",
    "behaviour": "decor",
    "blocks": false
  }
]
```

Each level `grid` is twelve strings of twelve of these letters. `f` is floor and is not a required asset. `asset-requirements.json` then sets `asset_id` to the letter, `kind` `tile`, `strategy` `tile_gen`. The artist instructions say to write `{asset_id}.shapes.json` and `{asset_id}.recipe.json`, so brick would be the filenames `b.shapes.json` and `b.recipe.json`.

## Feedback the harness actually sent

Two channels. Only the second can be reconstructed. Neither is stored in `events.jsonl`.

1. Live tool result. `harness/src/agent/runner.rs` appends a `tool` message: `[failed]` or `[ok]`, the summary, then compact JSON. For `assets.build_candidate` that JSON has `ok`, `out_dir`, `outputs`, `message`, `error_code`, and up to 8 `failed_checks`. `tool_finished` keeps only `ok`, `call_id`, and a null `error_code`.
2. Round-2 user prompt, built from the gate file. Template: `The asset package is not acceptable yet. Fix the recipes, rebuild, revalidate, then assets.request_publish.` Then one bullet per failing check. Empty `failures` becomes `{id} (value=…, limit=…)`, and the bullet repeats the id. Applying that function to `assets.gates.json` gives:

```
The asset package is not acceptable yet. Fix the recipes, rebuild, revalidate, then assets.request_publish.
- files.manifest: files.manifest (value=missing, limit=asset-manifest.json parses)
- coverage.frames: coverage.frames: missing explosion; missing hq_base; missing hud_life_icon; missing hud_stage_label; missing b; missing x; missing w; missing h; missing g; missing t
- coverage.png: coverage.png (value=png, limit=sheet png exists and is a non-empty PNG)
- coverage.manifest: coverage.manifest: manifest missing
- provenance.authored_specs: provenance.authored_specs: missing hq_base.shapes.json; missing hq_base.recipe.json; missing hud_life_icon.shapes.json; missing hud_life_icon.recipe.json; missing hud_stage_label.shapes.json; missing hud_stage_label.recipe.json; missing b.shapes.json; missing b.recipe.json; missing x.shapes.json; missing x.recipe.json; missing w.shapes.json; missing w.recipe.json; missing h.shapes.json; missing h.recipe.json; missing g.shapes.json; missing g.recipe.json; missing t.shapes.json; missing t.recipe.json
```

That prompt has no pxl percentage and no schema hint. `coverage.png` is failed with `value` `png` and an empty `failures` list, because the check is `png bytes ok AND no missing ids` and only the second half failed. `explosion` is in `coverage.frames` (no sheet frame yet) and not in `provenance.authored_specs` (both files existed). `w` is in both, because it was written only in round 2. Round 2 turns 0–3 then called `design.read_frozen`, `assets.read_requirements`, five `list_capabilities`, and seven `read_candidate` before the next write.

## Per missing asset

### `explosion`

Design: asset-requirements.json: asset_id "explosion", kind "effect", required true, size_px [16, 16], directions ["N"], anims [{"name":"burst","fps":16,"looped":false,"frames":4,"dirs":["N"]}], semantics "four frame expanding dust puff used for brick hits, tank kills and base destruction", strategy "template", priority 2.

Root cause: **model**. Seven build_candidate calls, all `ok: false`, all naming `explosion.recipe.json`. The recipe digest changed between those calls, so the model was editing, and the file left on disk still misses the default 35% frame-diff cap. In-run `failed_checks` text was not logged. The round-2 repair prompt did not contain the percentages either (it only said the id was missing a frame), so the only QA detail the model could have seen was the unlogged tool result.

| round.turn | call id | tokens in/out (whole turn, not this call) | spec | digest | pxl/QA error |
|---|---|---|---|---|---|
| 1.57 | `01a161af3fb0b10a84a5d76a3019f47b` | 32413 / 1326 | `explosion.shapes.json` | `sha256:aa421f4bb2fed61442ef6608cb45b613fce411a92a38925305eba54266356e78` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 1.58 | `01a161af5f8b9d45251968622b19d1f8` | 32828 / 482 | `explosion.recipe.json` | `sha256:4c472060d294c53028206a3fc62f2ed43070616e53333ef5dd888f0a5d743093` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 1.59 | `01a161af96acdb0b6c2ea6aecfba2169` | 33143 / 898 | `explosion.shapes.json` | `sha256:a5f623c325346a327bd541e50b26cd9a86c085f3add02622a157fbcf3b058522` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 1.59 | `01a161af96acdb0b6c2ea6aecfba216a` | 33143 / 898 | `explosion.recipe.json` | `sha256:53645ba6c350b43efd567721651025cae5c2a6cf1f21e869433c8ea4f3be14ff` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.4 | `01a161b1d0d1feea596bd3310a1cf4d1` | 11349 / 9127 | `explosion.shapes.json` | `sha256:e98e79575501bfc23ae1e49188a6cb2862b7d71f6f3c0f0c1f1f22f06ecc1833` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.4 | `01a161b1d0d1feea596bd3310a1cf4d2` | 11349 / 9127 | `explosion.recipe.json` | `sha256:1c998f310688949bd5074861109474828217560b52edb462e99abcc91cfbd300` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.6 | `01a161b274ac5eb2507a7361f644c04a` | 12888 / 2259 | `explosion.recipe.json` | `sha256:b9eb8fe2418b122c8a1892f99eb3e772bf720ba0dc9d961781c391891a1663e1` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.8 | `01a161b29819019fc2ff789c6699a90b` | 13505 / 435 | `explosion.recipe.json` | `sha256:63b9eff971a625e7e504f538144ed7fbd9aa311dbd5a3fc841bf3b0551ce6865` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.10 | `01a161b2fc22f585384b23db133bcc36` | 14091 / 1293 | `explosion.recipe.json` | `sha256:77672f8aab770299e604058f77d5de6256b8920e8b677b56db43586d44375bbb` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.12 | `01a161b31c2c2ba81df8039a3385a703` | 14698 / 362 | `explosion.recipe.json` | `sha256:e2045d26761effa98177b04d05cf8154f33de381aec6f13b2912f3c3d529ffbe` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.14 | `01a161b3a8ffe490950f68fc2ce981f8` | 15265 / 531 | `explosion.recipe.json` | `sha256:2890f1cd45dacf835415b39efb2ea69cd3095bf1e5244af6f96656302bfa1a4c` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.16 | `01a161b3b80a74d550fa989fed790561` | 16051 / 1549 | `explosion.shapes.json` | `sha256:4e464c9c2ab97ed5c3416f6125ac98be63b16828d4acc831814909de527ed8d1` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.16 | `01a161b3b80a74d550fa989fed790562` | 16051 / 1549 | `explosion.recipe.json` | `sha256:fa9eb4080882eca1a9cc853f68315e6ead61a0bb110722f211ba7187c799ad42` | Post-freeze local build of this exact digest, which is the file the last build call named: E_QA. message: build wrote outputs but 2 QA check(s) failed. hint: read checks and adjust the recipe. anim.burst.N.diff_0_1 value=42.96% limit=at least 1.00%, and at most 35.00% unless a 1px shift explains it. anim.burst.N.diff_2_3 value=51.17% (shift-adjusted 50.78%) limit=at least 1.00%, and at most 35.00% unless a 1px shift explains it. The in-run tool result for call 01a161b3c2bdcc7a267f04d145996c12 was not logged; this is the check that file produces. |

Builds whose arguments name this recipe:

| round.turn | build call id | arguments | ok | tokens in/out |
|---|---|---|---|---|
| 2.5 | `01a161b1e2c16e33f637c24291e31cb8` | `{"recipe": "explosion.recipe.json", "out_dir": "out/explosion"}` | false | 12541 / 64 |
| 2.7 | `01a161b2d10ffb4b7a7a54eaa6f25ffb` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 13312 / 64 |
| 2.9 | `01a161b2f608af6239d943260a81a4bf` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 13914 / 64 |
| 2.11 | `01a161b308a33bc76c6ca3fb0e8a02db` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 14507 / 64 |
| 2.13 | `01a161b377986eeadd5709ea5aaf6d73` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 15116 / 64 |
| 2.15 | `01a161b3b682d4b0160609781fe9406c` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 15687 / 64 |
| 2.17 | `01a161b3c2bdcc7a267f04d145996c12` | `{"out_dir": "out/explosion", "recipe": "explosion.recipe.json"}` | false | 16753 / 64 |

Feedback text for those builds: **not logged**. Shape of what the runner would have appended is in the feedback section. The round-2 prompt is the only recoverable harness text, and it does not include the QA lines.

### `w`

Design: asset-requirements.json: asset_id "w", kind "tile", required true, size_px [16, 16], directions ["N"], anims [{"name":"idle","fps":4,"looped":true,"frames":2,"dirs":["N"]}], semantics "impassable water with two-frame ripple animation", strategy "tile_gen", seamless true, priority 1. game-design.json r_water_impassable: "Water tiles w are impassable for the player tank and for enemy tanks but do not stop bullets." level tile_semantics: {"tile_id":"w","behaviour":"water","blocks":true}.

Root cause: **model**. One shapes file and one recipe file, then no `build_candidate` whose arguments name `w.recipe.json`. The next build (round 2 turn 5, call `01a161b1e2c16e33f637c24291e31cb8`) is `{"recipe": "explosion.recipe.json", "out_dir": "out/explosion"}`. The surviving recipe uses `pulse_glow(steps=2)` even though the design says `strategy` `tile_gen` and a 2-frame ripple. Classification is model, not design-id: the id `w` was used correctly as the filename.

| round.turn | call id | tokens in/out (whole turn, not this call) | spec | digest | pxl/QA error |
|---|---|---|---|---|---|
| 2.4 | `01a161b1d0d1feea596bd3310a1cf4d3` | 11349 / 9127 | `w.shapes.json` | `sha256:e817a41108189a5a882759cada7127060e18d13ca8cf420ed461c2a9ef44edea` | not logged. Spec was overwritten or never built. events.jsonl has no pxl stdout for this digest. |
| 2.4 | `01a161b1d0d1feea596bd3310a1cf4d4` | 11349 / 9127 | `w.recipe.json` | `sha256:6754d5889e9e509bcc46c534304fc5a5049ceeb30d86307cb48fb8c2965fca47` | No in-run pxl call. Post-freeze local build of this exact digest: E_SCHEMA. message: macro pulse_glow has no argument 'steps'. hint: pulse_glow takes: ramp. The model never received that string. |

Builds: none. Feedback text: the id appears in the reconstructed round-2 prompt only. No tool result about this spec was produced.

### `hq_base`

Design: asset-requirements.json: asset_id "hq_base", kind "sprite", required true, size_px [16, 16], directions ["N"], anims idle (frames 1, fps 2) and destroyed (frames 2, fps 4), semantics "eagle HQ base emblem that reads as the thing to defend", strategy "template", priority 1. game-design.json entity: "The eagle HQ base prop present on every stage; it blocks nothing, but its destruction by an enemy bullet immediately loses the stage."

Root cause: **budget**. No write and no build. Round 1 used all 60 turns on the palette, the two tanks, and the bullet. Round 2 was killed while still looping on explosion.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `hud_life_icon`

Design: asset-requirements.json: asset_id "hud_life_icon", kind "ui", required true, size_px [8, 8], directions ["N"], anims [{"name":"idle","fps":2,"looped":true,"frames":1,"dirs":["N"]}], semantics "tiny tank silhouette repeated in the top HUD strip for remaining lives", strategy "template", priority 3.

Root cause: **budget**. No write and no build. Round 1 used all 60 turns on the palette, the two tanks, and the bullet. Round 2 was killed while still looping on explosion.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `hud_stage_label`

Design: asset-requirements.json: asset_id "hud_stage_label", kind "ui", required true, size_px [64, 8], directions ["N"], anims idle 1 frame, semantics "monochrome bitmap digits and letters for STAGE 1-3 and the objective caption", strategy "shapes", priority 3.

Root cause: **budget**. No write and no build. Round 1 used all 60 turns on the palette, the two tanks, and the bullet. Round 2 was killed while still looping on explosion.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `b`

Design: asset-requirements.json: asset_id "b", kind "tile", required true, size_px [16, 16], anims idle (1 frame) and break (2 frames, fps 12), semantics "destructible brick wall; brick pattern with visible mortar lines", strategy "tile_gen", seamless true. r_brick_destructible: "A bullet impact on a brick tile b destroys that brick tile, removes the bullet and spawns an explosion; brick tiles are the only destructible wall type."

Root cause: **budget**. No write and no build. Same turn cap as the named assets above. The id is a one-letter grid character (see the design section), which would have become the filename `b.shapes.json`, but this run never got there, so design-id is not why it is missing.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `x`

Design: asset-requirements.json: asset_id "x", kind "tile", required true, anims idle and spark (2 frames, fps 12), semantics "indestructible steel block with riveted plate corners", strategy "tile_gen", seamless true. r_steel_indestructible: "Steel tiles x stop both tanks and bullets; a bullet hitting steel is removed with a small explosion and the steel tile survives."

Root cause: **budget**. No write and no build. Same turn cap as the named assets above. The id is a one-letter grid character (see the design section), which would have become the filename `x.shapes.json`, but this run never got there, so design-id is not why it is missing.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `h`

Design: asset-requirements.json: asset_id "h", kind "tile", required true, anims idle 2 frames at fps 8, semantics "deadly hazard tile, clearly distinct from water with a cross-hatch warning motif", strategy "tile_gen". r_hazard_lose: "Standing on a hazard tile h destroys the player tank and loses the stage immediately."

Root cause: **budget**. No write and no build. Same turn cap as the named assets above. The id is a one-letter grid character (see the design section), which would have become the filename `h.shapes.json`, but this run never got there, so design-id is not why it is missing.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `g`

Design: asset-requirements.json: asset_id "g", kind "tile", required true, anims idle 2 frames at fps 6, semantics "goal pad marking the stage exit with a pulsing chevron", strategy "tile_gen". r_reach_exit: "Stepping onto a goal pad tile g completes the stage; every stage exit is reachable from the spawn tile by walking only on non-blocking tiles."

Root cause: **budget**. No write and no build. Same turn cap as the named assets above. The id is a one-letter grid character (see the design section), which would have become the filename `g.shapes.json`, but this run never got there, so design-id is not why it is missing.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

### `t`

Design: asset-requirements.json: asset_id "t", kind "tile", required true, anims idle 1 frame, semantics "walkable grass decor drawn above tanks for the classic occlusion look", strategy "tile_gen", priority 4. r_move_blocked lists "grass t" as non-blocking.

Root cause: **budget**. No write and no build. Same turn cap as the named assets above. The id is a one-letter grid character (see the design section), which would have become the filename `t.shapes.json`, but this run never got there, so design-id is not why it is missing.

Attempts: none. No `candidate_written` path equals `{id}.shapes.json` or `{id}.recipe.json`, and no build arguments name it.

| turn | call id | tokens in/out | spec | digest | pxl/QA error | feedback text sent to the model |
|---|---|---|---|---|---|---|
| — | — | — | — | — | not attempted | only the reconstructed round-2 prompt, which lists this id under `coverage.frames` and `provenance.authored_specs` |

## Notes that affect the turn budget

- Round 1 turn 30 is the latency outlier: 165425 ms, 7621 completion tokens, two shape files (enemy and bullet) in one call.
- Design `bullet` anim is `name` `fly`, `frames` 1. Default pxl QA requires at least 2 frames in each direction. The log shows 5 failed bullet builds and then 1 success. That is not one of the 10 missing ids, and it is a large share of why round 1 ended before `hq_base`.
- `coverage.png` failing with an empty `failures` array is a harness-feedback bug: the repair bullet cannot say the pngs were valid.
- On-disk `w.recipe.json` (digest `sha256:6754d5889e9e509bcc46c534304fc5a5049ceeb30d86307cb48fb8c2965fca47`) still contains `"macro": "pulse_glow(steps=2)"`. On-disk `explosion.recipe.json` (digest `sha256:fa9eb4080882eca1a9cc853f68315e6ead61a0bb110722f211ba7187c799ad42`) is four explicit `burst` frames using `squash`, which is what the 42.96% and 51.17% checks measure.

