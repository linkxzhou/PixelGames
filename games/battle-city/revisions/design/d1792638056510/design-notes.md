# battle_city — design package

Single-screen top-down tank slice, monochrome, 16x16 tiles, 320x240 logical
resolution with nearest-neighbour scaling. Three logical stages with rising
difficulty. JSON contracts are authoritative; this file only explains them.

## Stage progression

| Stage | id | Teaches | Grid | Spawn | Exit | Enemies |
|---|---|---|---|---|---|---|
| 1 | `level_1` | movement only, reach the exit | 12x12 | (1,10) | (9,10) | none |
| 2 | `level_2` | shooting + destructible brick | 12x12 | (1,10) | (6,0) | none |
| 3 | `level_3` | enemy tanks, water, steel, base defense | 12x12 | (1,0) | (10,11) | 3 |

Every exit is walkable from spawn on non-blocking tiles, so no stage is
soft-locked even if the player never fires. Stage 2's exit sits behind brick,
but the left edge lane (x=0) and row y=0 stay open; stage 3's long route runs
x=0 down to y=5, right to x=5, down to y=11, then along row y=10 to (10,11).

## Tile ids

`f` floor, `b` destructible brick, `x` steel, `w` water (blocks), `h` hazard
(non-blocking, lethal), `g` goal pad, `t` grass decor (non-blocking, drawn over
tanks). All non-floor, non-decor tiles and every entity have an asset
requirement with a matching `asset_id`.

## Rules worth calling out

- Win: player tank reaches tile `g`. Lose: player hit by bullet, player on `h`,
  or `hq_base` destroyed.
- `hq_base` is a prop entity placed on one floor tile per stage via
  `placements`; it never blocks movement.
- `explosion` is authored as `kind: prop` because the contract's entity kind
  list has no `effect` value (the template uses `effect`); it maps to an
  `effect` asset requirement. If the schema later gains `effect`, switch it.
- Powerups, ice tiles, two-player and audio are out of scope per the brief.

## Asset strategies

Tanks, bullet, explosion and HQ reuse the genre template shape recipes
(`strategy: template`). Brick, steel, water, hazard, goal and grass are
generated tiles (`strategy: tile_gen`). HUD label glyphs use `shapes`.

## Acceptance

`acceptance-plan.json` covers each required asset plus engine, playtest and
human checks for the rules, the win condition, HUD and style consistency.
